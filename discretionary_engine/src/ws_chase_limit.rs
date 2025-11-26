/// WebSocket-based chase-limit execution
///
/// This module implements a patient order execution strategy that:
/// 1. Places ONE limit order for the full quantity
/// 2. Continuously amends the price to stay one tick better than market
/// 3. Monitors fills via WebSocket order events
/// 4. When duration expires, cancels and market-fills remaining quantity
///
/// All operations use WebSocket for low latency and reliability.
use std::sync::Arc;

use color_eyre::eyre::{Context, Result, bail};
use futures_util::{StreamExt, pin_mut};
use nautilus_bybit::{
	common::{
		credential::Credential,
		enums::{BybitEnvironment, BybitOrderSide, BybitOrderType, BybitProductType, BybitTimeInForce},
	},
	websocket::{
		client::BybitWebSocketClient,
		messages::{BybitWsAmendOrderParams, BybitWsPlaceOrderParams, NautilusWsMessage},
	},
};
use nautilus_model::identifiers::InstrumentId;
use tokio::time::{Duration, sleep};
use tracing::info;
use ustr::Ustr;
use v_utils::{log, trades::Timeframe};

/// Executes an order using WebSocket-based chase-limit strategy
///
/// # Arguments
/// * `raw_client` - Raw HTTP client for initial price fetch
/// * `credential` - API credentials for WebSocket authentication
/// * `environment` - Bybit environment (mainnet/testnet)
/// * `symbol` - Trading symbol (Bybit format, e.g., "BTCUSDT")
/// * `instrument_id` - Nautilus instrument ID for ticker subscription
/// * `side` - Order side (\"Buy\" or \"Sell\")
/// * `target_qty` - Total quantity to execute
/// * `qty_step` - Minimum quantity increment
/// * `price_tick` - Minimum price increment
/// * `duration` - Optional duration for patient execution
pub async fn execute_ws_chase_limit(
	raw_client: &nautilus_bybit::http::client::BybitRawHttpClient,
	credential: Credential,
	environment: BybitEnvironment,
	symbol: &str,
	instrument_id: InstrumentId,
	side: &str,
	target_qty: f64,
	_qty_step: f64,
	price_tick: f64,
	duration: Option<Timeframe>,
) -> Result<f64> {
	log!("Starting WebSocket chase-limit execution for {} {} {}", side, target_qty, symbol);

	// Get initial price via HTTP to start immediately
	use nautilus_bybit::{common::enums::BybitProductType, http::query::BybitTickersParamsBuilder};

	let ticker_params = BybitTickersParamsBuilder::default()
		.category(BybitProductType::Linear)
		.symbol(symbol.to_string())
		.build()
		.context("Failed to build ticker params")?;

	let ticker_response: nautilus_bybit::http::models::BybitTickersLinearResponse = raw_client
		.get_tickers::<nautilus_bybit::http::models::BybitTickersLinearResponse>(&ticker_params)
		.await
		.context("Failed to fetch initial ticker data")?;

	let ticker = ticker_response.result.list.get(0).ok_or_else(|| color_eyre::eyre::eyre!("No ticker data found for {}", symbol))?;

	let initial_bid: f64 = ticker.bid1_price.parse().context("Failed to parse bid price")?;
	let initial_ask: f64 = ticker.ask1_price.parse().context("Failed to parse ask price")?;

	log!("Initial market: bid={}, ask={}", initial_bid, initial_ask);

	// Calculate execution parameters based on duration
	let (update_interval, end_time) = if let Some(duration_tf) = duration {
		let total_duration_ms = duration_tf.0;
		let update_interval_ms = 1000; // Check/update every 1 second
		let end_time = std::time::Instant::now() + Duration::from_millis(total_duration_ms);
		log!("Patient execution over {:?}: update_interval={}ms", duration_tf, update_interval_ms);
		(Duration::from_millis(update_interval_ms), Some(end_time))
	} else {
		// Aggressive execution: update quickly
		log!("Aggressive execution: 500ms updates");
		(Duration::from_millis(500), None)
	};

	// Create trade WebSocket client for order operations
	let mut trade_client = BybitWebSocketClient::new_trade(
		environment,
		credential.clone(),
		None, // url
		None, // heartbeat
	);

	// Create market data WebSocket client for ticker
	let mut market_client = BybitWebSocketClient::new_public_with(
		BybitProductType::Linear,
		environment,
		None, // url
		None, // heartbeat
	);

	// Connect both clients
	log!("Connecting to WebSocket...");
	trade_client.connect().await.context("Failed to connect trade WebSocket - check API credentials")?;
	log!("Trade WebSocket connected successfully");

	market_client.connect().await.context("Failed to connect market data WebSocket")?;
	log!("Market data WebSocket connected successfully");

	// Subscribe to order events
	log!("Subscribing to order events...");
	match trade_client.subscribe_orders().await {
		Ok(()) => log!("Successfully subscribed to order events"),
		Err(e) => {
			log!("Failed to subscribe to orders: {:?}", e);
			bail!("Failed to subscribe to order events - this usually means invalid API credentials: {}", e);
		}
	}

	// Subscribe to ticker for bid/ask updates
	log!("Subscribing to ticker for {}...", instrument_id);
	match market_client.subscribe_ticker(instrument_id).await {
		Ok(()) => log!("Successfully subscribed to ticker"),
		Err(e) => {
			log!("Failed to subscribe to ticker: {:?}", e);
			bail!("Failed to subscribe to ticker: {}", e);
		}
	}

	// Give subscriptions a moment to establish
	log!("Waiting for subscriptions to establish...");
	sleep(Duration::from_millis(1000)).await;

	// Get message streams
	log!("Creating message streams...");
	let trade_stream = trade_client.stream();
	let market_stream = market_client.stream();
	pin_mut!(trade_stream);
	pin_mut!(market_stream);
	log!("Message streams created and pinned");

	// Calculate initial limit price
	let initial_limit_price = match side {
		"Buy" => {
			let improved_price = initial_bid + price_tick;
			if improved_price >= initial_ask { initial_bid } else { improved_price }
		}
		"Sell" => {
			let improved_price = initial_ask - price_tick;
			if improved_price <= initial_bid { initial_ask } else { improved_price }
		}
		_ => bail!("Invalid side: {}", side),
	};

	log!("Calculated initial limit price: {} (bid={}, ask={})", initial_limit_price, initial_bid, initial_ask);

	// Place initial order immediately
	let order_link_id = format!("chase-{}", uuid::Uuid::new_v4());
	let bybit_side = match side {
		"Buy" => BybitOrderSide::Buy,
		"Sell" => BybitOrderSide::Sell,
		_ => bail!("Invalid side: {}", side),
	};

	let initial_order = BybitWsPlaceOrderParams {
		category: BybitProductType::Linear,
		symbol: Ustr::from(symbol),
		side: bybit_side,
		order_type: BybitOrderType::Limit,
		qty: target_qty.to_string(),
		market_unit: None,
		price: Some(initial_limit_price.to_string()),
		time_in_force: Some(BybitTimeInForce::PostOnly),
		order_link_id: Some(format!("{}", order_link_id)),
		reduce_only: None,
		close_on_trigger: None,
		trigger_price: None,
		trigger_by: None,
		trigger_direction: None,
		tpsl_mode: None,
		take_profit: None,
		stop_loss: None,
		tp_trigger_by: None,
		sl_trigger_by: None,
		sl_trigger_price: None,
		tp_trigger_price: None,
		sl_order_type: None,
		tp_order_type: None,
		sl_limit_price: None,
		tp_limit_price: None,
	};

	log!("Placing initial order: {} {} @ {}", side, target_qty, initial_limit_price);
	match trade_client.place_order(initial_order).await {
		Ok(()) => log!("Initial order request sent successfully"),
		Err(e) => {
			log!("Failed to place initial order: {:?}", e);
			bail!("Failed to place initial order: {}", e);
		}
	}

	let mut current_order_price = Some(initial_limit_price);
	let mut order_placed = true;
	let mut last_amend_price = Some(initial_limit_price);
	let mut filled_qty = 0.0;
	let mut iteration = 0;

	log!("Entering event loop...");

	loop {
		iteration += 1;

		if iteration % 10 == 0 {
			log!("[{}] Still waiting for events... order_placed={}", iteration, order_placed);
		}

		// Check if duration has expired
		if let Some(end) = end_time {
			if std::time::Instant::now() >= end {
				log!("Duration expired, placing final market order for remaining quantity");

				// Cancel existing limit order
				if order_placed {
					use nautilus_bybit::websocket::messages::BybitWsCancelOrderParams;
					let cancel_params = BybitWsCancelOrderParams {
						category: BybitProductType::Linear,
						symbol: Ustr::from(symbol),
						order_id: None,
						order_link_id: Some(format!("{}", order_link_id)),
					};

					match trade_client.cancel_order(cancel_params).await {
						Ok(()) => log!("Cancelled existing order"),
						Err(e) => log!("Failed to cancel order (may already be filled): {}", e),
					}
				}

				// Place market order for remaining quantity
				let remaining_qty = target_qty - filled_qty;
				if remaining_qty > 0.0 {
					let market_params = BybitWsPlaceOrderParams {
						category: BybitProductType::Linear,
						symbol: Ustr::from(symbol),
						side: bybit_side,
						order_type: BybitOrderType::Market,
						qty: remaining_qty.to_string(),
						market_unit: None,
						price: None,
						time_in_force: Some(BybitTimeInForce::Ioc),
						order_link_id: Some(format!("{}-final", order_link_id)),
						reduce_only: None,
						close_on_trigger: None,
						trigger_price: None,
						trigger_by: None,
						trigger_direction: None,
						tpsl_mode: None,
						take_profit: None,
						stop_loss: None,
						tp_trigger_by: None,
						sl_trigger_by: None,
						sl_trigger_price: None,
						tp_trigger_price: None,
						sl_order_type: None,
						tp_order_type: None,
						sl_limit_price: None,
						tp_limit_price: None,
					};

					match trade_client.place_order(market_params).await {
						Ok(()) => log!("Final market order placed for {}", remaining_qty),
						Err(e) => log!("Failed to place final market order: {}", e),
					}
				}

				break;
			}
		}

		// Select between market updates and trade events with timeout
		tokio::select! {
			// Market data updates (ticker)
			Some(market_msg) = market_stream.next() => {
				log!("[{}] Received market message type: {:?}", iteration, std::mem::discriminant(&market_msg));
				match market_msg {
					NautilusWsMessage::Data(data_vec) => {
						log!("[{}] Received {} data items", iteration, data_vec.len());
						// Extract bid/ask from quote data
						for data in data_vec {
							if let nautilus_model::data::Data::Quote(quote) = data {
								let bid_price = quote.bid_price.as_f64();
								let ask_price = quote.ask_price.as_f64();

								// Calculate our limit price
								let limit_price = match side {
									"Buy" => {
										let improved_price = bid_price + price_tick;
										// Don't cross the spread
										if improved_price >= ask_price { bid_price } else { improved_price }
									}
									"Sell" => {
										let improved_price = ask_price - price_tick;
										// Don't cross the spread
										if improved_price <= bid_price { ask_price } else { improved_price }
									}
									_ => continue,
								};

								info!("[{}] Market: bid={}, ask={}, target {} limit @ {}", iteration, bid_price, ask_price, side, limit_price);

								// Place initial order if not yet placed
								if !order_placed {
									let place_params = BybitWsPlaceOrderParams {
										category: BybitProductType::Linear,
										symbol: Ustr::from(symbol),
										side: bybit_side,
										order_type: BybitOrderType::Limit,
										qty: target_qty.to_string(),
										market_unit: None,
										price: Some(limit_price.to_string()),
										time_in_force: Some(BybitTimeInForce::PostOnly),
										order_link_id: Some(format!("{}", order_link_id)),
										reduce_only: None,
										close_on_trigger: None,
										trigger_price: None,
										trigger_by: None,
										trigger_direction: None,
										tpsl_mode: None,
										take_profit: None,
										stop_loss: None,
										tp_trigger_by: None,
										sl_trigger_by: None,
										sl_trigger_price: None,
										tp_trigger_price: None,
										sl_order_type: None,
										tp_order_type: None,
										sl_limit_price: None,
										tp_limit_price: None,
									};

									match trade_client.place_order(place_params).await {
										Ok(()) => {
											log!("[{}] Initial order placed: {} {} @ {}", iteration, side, target_qty, limit_price);
											order_placed = true;
											current_order_price = Some(limit_price);
											last_amend_price = Some(limit_price);
										}
										Err(e) => {
											log!("Failed to place initial order: {}, will retry", e);
										}
									}
								} else {
									// Amend existing order if price changed significantly
									let should_amend = match last_amend_price {
										Some(last_price) => (limit_price - last_price).abs() > price_tick * 0.5,
										None => true,
									};

									if should_amend && filled_qty < target_qty {
										let remaining_qty = target_qty - filled_qty;
										let amend_params = BybitWsAmendOrderParams {
											category: BybitProductType::Linear,
											symbol: Ustr::from(symbol),
											order_id: None,
											order_link_id: Some(format!("{}", order_link_id)),
											qty: Some(remaining_qty.to_string()),
											price: Some(limit_price.to_string()),
											trigger_price: None,
											take_profit: None,
											stop_loss: None,
											tp_trigger_by: None,
											sl_trigger_by: None,
										};

										match trade_client.amend_order(amend_params).await {
											Ok(()) => {
												info!("[{}] Order amended: price {} -> {}, qty {}", iteration, last_amend_price.unwrap_or(0.0), limit_price, remaining_qty);
												last_amend_price = Some(limit_price);
												current_order_price = Some(limit_price);
											}
											Err(e) => {
												log!("Failed to amend order: {}, will retry", e);
											}
										}
									}
								}
							}
						}
					}
					NautilusWsMessage::Error(e) => {
						log!("Market data error: {:?}", e);
					}
					_ => {}
				}
			}

			// Trade events (order updates, fills)
			Some(trade_msg) = trade_stream.next() => {
				log!("[{}] Received trade message: {:?}", iteration, std::mem::discriminant(&trade_msg));
				match trade_msg {
					NautilusWsMessage::OrderStatusReports(reports) => {
						for report in reports {
							// Check if this is our order
							if let Some(ref coid) = report.client_order_id {
								if coid.to_string().starts_with("chase-") {
									log!("[{}] Order update: {:?} filled_qty={}", iteration, report.order_status, report.filled_qty.as_f64());

									// Update filled quantity
									filled_qty = report.filled_qty.as_f64();

									// If fully filled, we're done
									if filled_qty >= target_qty - 0.0001 {
										log!("Order fully filled: {}", filled_qty);
										break;
									}
								}
							}
						}
					}
					NautilusWsMessage::FillReports(fills) => {
						for fill in fills {
							if let Some(ref coid) = fill.client_order_id {
								if coid.to_string().starts_with("chase-") {
									log!("[{}] Fill: qty={} @ price={}", iteration, fill.last_qty.as_f64(), fill.last_px.as_f64());
								}
							}
						}
					}
					NautilusWsMessage::OrderRejected(rejected) => {
						if rejected.client_order_id.to_string().starts_with("chase-") {
							log!("[{}] Order rejected: {}", iteration, rejected.reason);
							// If PostOnly rejected, we'll retry on next ticker update
							order_placed = false;
						}
					}
					NautilusWsMessage::OrderModifyRejected(rejected) => {
						log!("[{}] Amend rejected: {}", iteration, rejected.reason);
						// Will retry on next ticker update
					}
					NautilusWsMessage::Error(e) => {
						log!("Trade event error: {:?}", e);
					}
					_ => {}
				}

				// Check if fully filled
				if filled_qty >= target_qty - 0.0001 {
					log!("Order fully filled: {}", filled_qty);
					break;
				}
			}

			// Timeout to prevent blocking forever
			_ = sleep(update_interval) => {
				if iteration % 10 == 0 {
					log!("[{}] Timeout - no messages received in {}ms", iteration, update_interval.as_millis());
				}
			}
		}

		// Safety check: don't run forever
		if iteration > 10000 {
			log!("Max iterations reached, stopping");
			break;
		}
	}

	// Close connections
	trade_client.close().await.context("Failed to close trade WebSocket")?;
	market_client.close().await.context("Failed to close market WebSocket")?;

	log!("Chase-limit execution completed: filled {} out of {}", filled_qty, target_qty);
	Ok(filled_qty)
}
