use std::sync::Arc;

use color_eyre::eyre::{Context, Result, bail};
use nautilus_bybit::{
	common::enums::{BybitPositionSide, BybitProductType},
	http::query::BybitPositionListParamsBuilder,
};
use tracing::info;
use v_exchanges::Ticker;
use v_utils::{log, trades::Timeframe};

use crate::{bybit_common::*, config::AppConfig};

#[derive(clap::Args, Debug)]
pub(crate) struct NukeArgs {
	/// Ticker to close position for.
	ticker: Ticker,

	/// Optional duration over which to close the position (for MM trailing strategy)
	#[arg(short, long)]
	duration: Option<Timeframe>,
}

pub(crate) async fn main(args: NukeArgs, config: Arc<AppConfig>, testnet: bool) -> Result<()> {
	log!("Nuke command for ticker: {:?}", args.ticker);

	// Create Bybit HTTP client
	let (_raw_client, client) = create_bybit_clients(&config, args.ticker.exchange_name, testnet)?;

	// Convert symbol format (twt-usdt.p -> TWTUSDT)
	let symbol_raw = args.ticker.symbol.to_string();
	let symbol = convert_symbol_to_bybit(&symbol_raw);

	if args.duration.is_some() {
		log!("Duration: {:?} (chase-limit strategy)", args.duration);

		// Get current position
		let params = BybitPositionListParamsBuilder::default()
			.category(BybitProductType::Linear)
			.symbol(symbol.clone())
			.build()
			.context("Failed to build position list params")?;

		let position_response = client.get_positions(&params).await.context("Failed to fetch positions")?;

		if position_response.result.list.is_empty() {
			log!("No position to close for {}", symbol);
			return Ok(());
		}

		let position = &position_response.result.list[0];
		let position_size: f64 = position.size.parse().context("Failed to parse position size")?;

		if position_size == 0.0 {
			log!("No position to close for {}", symbol);
			return Ok(());
		}

		log!("Current position: {:?} {} {}", position.side, position_size, symbol);

		// Determine order side (opposite of position side)
		let order_side = if position.side == BybitPositionSide::Buy { "Sell" } else { "Buy" };

		// Get instrument info for qty step
		use nautilus_bybit::http::query::BybitInstrumentsInfoParamsBuilder;

		let instruments_params = BybitInstrumentsInfoParamsBuilder::default()
			.category(BybitProductType::Linear)
			.symbol(symbol.clone())
			.build()
			.context("Failed to build instruments info params")?;

		let instruments_response: nautilus_bybit::http::models::BybitInstrumentLinearResponse = _raw_client
			.get_instruments::<nautilus_bybit::http::models::BybitInstrumentLinearResponse>(&instruments_params)
			.await
			.context("Failed to fetch instrument info")?;

		let instrument = instruments_response
			.result
			.list
			.get(0)
			.ok_or_else(|| color_eyre::eyre::eyre!("No instrument info found for {}", symbol))?;

		let qty_step: f64 = instrument.lot_size_filter.qty_step.parse().context("Failed to parse qtyStep")?;
		let tick_size: f64 = instrument.price_filter.tick_size.parse().context("Failed to parse tickSize")?;

		// Execute using chase-limit
		let filled_qty = crate::chase_limit::execute_chase_limit(&_raw_client, &client, &symbol, order_side, position_size, qty_step, tick_size, args.duration)
			.await
			.context("Chase-limit execution failed")?;

		println!("✅ Position closed using chase-limit!");
		println!("   Closed: {:.6} {}", filled_qty, symbol);
		Ok(())
	} else {
		// Market close: get current position and close it using nautilus client
		log!("Closing position with market order");

		// Get current position
		let params = BybitPositionListParamsBuilder::default()
			.category(BybitProductType::Linear)
			.symbol(symbol.clone())
			.build()
			.context("Failed to build position list params")?;

		let position_response = client.get_positions(&params).await.context("Failed to fetch positions")?;

		if position_response.result.list.is_empty() {
			log!("No position to close for {}", symbol);
			return Ok(());
		}

		let position = &position_response.result.list[0];
		let position_size: f64 = position.size.parse().context("Failed to parse position size")?;

		if position_size == 0.0 {
			log!("No position to close for {}", symbol);
			return Ok(());
		}

		log!("Current position: {:?} {} {}", position.side, position_size, symbol);

		// Determine order side (opposite of position side)
		let order_side = if position.side == BybitPositionSide::Buy { "Sell" } else { "Buy" };

		// Place reduce-only market order to close
		let order_request = serde_json::json!({
			"category": "linear",
			"symbol": symbol,
			"side": order_side,
			"orderType": "Market",
			"qty": position_size.to_string(),
			"timeInForce": "IOC",
			"orderLinkId": format!("nuke-{}", uuid::Uuid::new_v4()),
			"reduceOnly": true,
		});

		log!("Submitting market {} order to close {} {}", order_side, position_size, symbol);

		let order_response = client.place_order(&order_request).await.context("Failed to place order")?;

		if order_response.ret_code == 0 {
			println!("✅ Position closed successfully!");
			if let Some(order_id) = order_response.result.order_id {
				println!("   Order ID: {}", order_id);
			}
			println!("   Closed: {} {}", position_size, symbol);
			Ok(())
		} else {
			bail!("Order failed: {} (code: {})", order_response.ret_msg, order_response.ret_code);
		}
	}
}
