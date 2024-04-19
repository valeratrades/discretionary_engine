use crate::api::order_types::{ConceptualOrder, ConceptualOrderPercents, ProtocolOrderId};
use crate::api::{binance, Symbol};
use crate::protocols::{FollowupProtocol, ProtocolOrders, ProtocolType};
use anyhow::Result;
use std::collections::HashMap;
use std::str::FromStr;
use std::sync::{Arc, Mutex};
use tokio::select;
use tracing::{info, instrument};
use uuid::Uuid;
use v_utils::trades::Side;

/// What the Position _*is*_
#[derive(Debug, Clone)]
pub struct PositionSpec {
	pub asset: String,
	pub side: Side,
	pub size_usdt: f64,
}
impl PositionSpec {
	pub fn new(asset: String, side: Side, size_usdt: f64) -> Self {
		Self { asset, side, size_usdt }
	}
}

#[derive(Debug)]
pub struct PositionAcquisition {
	__spec: PositionSpec,
	target_notional: f64,
	acquired_notional: f64,
	protocols_spec: Option<String>, //Vec<AcquisitionProtocol>,
}
impl PositionAcquisition {
	//dbg
	pub async fn dbg_new(spec: PositionSpec) -> Result<Self> {
		Ok(Self {
			__spec: spec,
			target_notional: 10.0,
			acquired_notional: 10.0,
			protocols_spec: None,
		})
	}

	pub async fn do_acquisition(spec: PositionSpec) -> Result<Self> {
		// is this not in config?
		let full_key = std::env::var("BINANCE_TIGER_FULL_KEY").unwrap();
		let full_secret = std::env::var("BINANCE_TIGER_FULL_SECRET").unwrap();
		//let position = Position::new(Market::BinanceFutures, side, symbol.clone(), usdt_quantity, protocols, Utc::now());
		let coin = spec.asset.clone();
		let symbol = Symbol::from_str(format!("{coin}-USDT-BinanceFutures").as_str())?;
		info!(coin);

		let (current_price, quantity_precision) = tokio::join! {
			binance::futures_price(&coin),
			binance::futures_quantity_precision(&coin),
		};
		let factor = 10_f64.powi(quantity_precision? as i32);
		let coin_quantity = spec.size_usdt / current_price?;
		let coin_quantity_adjusted = (coin_quantity * factor).round() / factor;

		let mut current_state = Self {
			__spec: spec.clone(),
			target_notional: coin_quantity_adjusted,
			acquired_notional: 0.0,
			protocols_spec: None,
		};

		let order_id = binance::post_futures_order(
			full_key.clone(),
			full_secret.clone(),
			"MARKET".to_string(),
			symbol.to_string(),
			spec.side.clone(),
			coin_quantity_adjusted,
		)
		.await?;
		//info!(target: "/tmp/discretionary_engine.lock", "placed order: {:?}", order_id);
		loop {
			let order = binance::poll_futures_order(full_key.clone(), full_secret.clone(), order_id, symbol.to_string()).await?;
			if order.status == binance::OrderStatus::Filled {
				let order_notional = order.origQty.parse::<f64>()?;
				current_state.acquired_notional += order_notional;
				break;
			}
		}

		Ok(current_state)
	}
}

#[derive(Debug)]
pub struct PositionFollowup {
	_acquisition: PositionAcquisition,
	protocols_spec: Vec<FollowupProtocol>,
	closed_notional: f64,
}

/// Internal representation of desired orders. The actual orders are synchronized to this, so any details of actual execution are mostly irrelevant.
/// Thus these orders have no actual ID; only being tagged with what protocol spawned them.
#[derive(Debug, Default)]
struct TargetOrders {
	stop_orders_total_notional: f64,
	normal_orders_total_notional: f64,
	market_orders_total_notional: f64,
	//total_usd: f64,
	orders: Vec<ConceptualOrder>,
}
impl TargetOrders {
	//TODO!!!!!!!!!: after updating orders internally, send a channeled message with new state of target_orders right from here \
	// vec of actual orders can be created on the spot, as we don't care if we accidentially close exposure openned by a different order.
	// If the distribution of orders to exact exchanges doesn't pertain after the start, there will just be a decision layer for whether we move an existing order in price, or open a new one on a different exchange.
	// there are also some edge-cases where the order could be too small, and this should be handled on the exchange_api side.
	// equally so, the maximum update frequency of orders set by exchange shall too be tracked by the execution algorithm.

	// if we get an error because we did not pass the correct uuid from the last fill message, we just drop the task, as we will be forced to run with a correct value very soon.
	/// Never fails, instead the errors are sent over the channel.
	async fn update_orders(&mut self, orders: Vec<ConceptualOrder>, position_callback: PositionCallback) {
		for order in orders.into_iter() {
			match order {
				ConceptualOrder::StopMarket(_) => self.stop_orders_total_notional += order.notional(),
				ConceptualOrder::Limit(_) => self.normal_orders_total_notional += order.notional(),
				ConceptualOrder::Market(_) => self.market_orders_total_notional += order.notional(),
			}
			self.orders.push(order);
		}
		if let Ok(sender) = crate::SENDER.lock() {
			match sender.send((self.orders.clone(), position_callback)).await {
				Ok(_) => {},
				Err(e) => {
					info!("Error sending orders: {:?}", e);
				},
			};
		}
	}
	//TODO!!!!!!!!!!!!!!!!: fill channel. Want to receive data on every fill alongside the protocol_order_id, which is required when sending the update_orders() request, defined right above this.
}

#[derive(Debug, Clone)]
pub struct PositionCallback {
	pub sender: tokio::sync::mpsc::Sender<Vec<(ProtocolOrderId, f64)>>, // stands for "this nominal qty filled on this protocol order"
	pub position_uuid: Uuid,
}
impl PositionCallback {
	fn new(sender: tokio::sync::mpsc::Sender<Vec<(ProtocolOrderId, f64)>>, position_uuid: Uuid) -> Self {
		Self { sender, position_uuid }
	}
}

impl PositionFollowup {
	#[instrument]
	pub async fn do_followup(acquired: PositionAcquisition, protocols: Vec<FollowupProtocol>) -> Result<Self> {
		let mut counted_subtypes: HashMap<ProtocolType, usize> = HashMap::new();
		for protocol in &protocols {
			let subtype = protocol.get_subtype();
			*counted_subtypes.entry(subtype).or_insert(0) += 1;
		}

		let (tx_orders, mut rx_orders) = tokio::sync::mpsc::channel::<ProtocolOrders>(32);
		for protocol in protocols.clone() {
			protocol.attach(tx_orders.clone(), &acquired.__spec)?;
		}

		let position_id = Uuid::new_v4();
		let (tx_fills, mut rx_fills) = tokio::sync::mpsc::channel::<Vec<(ProtocolOrderId, f64)>>(32);
		let position_callback = PositionCallback::new(tx_fills, position_id);

		let all_requested: Arc<Mutex<HashMap<String, ProtocolOrders>>> = Arc::new(Mutex::new(HashMap::new()));
		let all_requested_unrolled: Arc<Mutex<HashMap<String, Vec<ConceptualOrder>>>> = Arc::new(Mutex::new(HashMap::new()));
		let mut closed_notional = 0.0;
		let mut target_orders = TargetOrders::default();

		let all_fills: Arc<Mutex<HashMap<Uuid, f64>>> = Arc::new(Mutex::new(HashMap::new()));

		let update_unrolled = |update_on: String| {
			let protocol = FollowupProtocol::from_str(&update_on).unwrap();
			let subtype = protocol.get_subtype();
			let size_multiplier = 1.0 / *counted_subtypes.get(&subtype).unwrap() as f64;
			let total_controlled_size = acquired.acquired_notional * size_multiplier;

			let mut mask = all_requested.lock().unwrap()[&update_on].empty_mask();
			for (key, _value) in mask.clone() {
				if all_fills.lock().unwrap().contains_key(&key) {
					mask.insert(key, *all_fills.lock().unwrap().get(&key).unwrap());
				}
			}
			let order_batch = all_requested.lock().unwrap()[&update_on].apply_mask(mask, total_controlled_size);
			all_requested_unrolled.lock().unwrap().insert(update_on, order_batch);
		};

		macro_rules! recalculate_target_orders {
			() => {{
				let mut market_orders = Vec::new();
				let mut stop_orders = Vec::new();
				let mut limit_orders = Vec::new();
				for (_key, value) in all_requested_unrolled.lock().unwrap().clone() {
					value.into_iter().for_each(|o| match o {
						ConceptualOrder::StopMarket(_) => stop_orders.push(o),
						ConceptualOrder::Limit(_) => limit_orders.push(o),
						ConceptualOrder::Market(_) => market_orders.push(o),
					});
				}

				let mut left_to_target_full_notional = acquired.acquired_notional - closed_notional;
				let (mut left_to_target_spot_notional, mut left_to_target_normal_notional) = (left_to_target_full_notional, left_to_target_full_notional);
				let mut new_target_orders: Vec<ConceptualOrder> = Vec::new();

				// orders should be all of the same conceptual type (no idea how to enforce it)
				let mut update_target_orders = |orders: Vec<ConceptualOrder>| {
					for order in orders {
						let notional = order.notional();
						let compare_against = match order {
							ConceptualOrder::StopMarket(_) => left_to_target_spot_notional,
							ConceptualOrder::Limit(_) => left_to_target_normal_notional,
							ConceptualOrder::Market(_) => left_to_target_full_notional,
						};
						let mut order = order.clone();
						if notional > compare_against {
							order.cut_size(compare_against);
						}
						new_target_orders.push(order.clone());
						match order {
							ConceptualOrder::StopMarket(_) => left_to_target_spot_notional -= notional,
							ConceptualOrder::Limit(_) => left_to_target_normal_notional -= notional,
							ConceptualOrder::Market(_) => {
								//NB: in the current implementation if market orders are ran after other orders, we could go negative here.
								left_to_target_full_notional -= notional;
								left_to_target_spot_notional -= notional;
								left_to_target_normal_notional -= notional;
							}
						}
						assert!(
							left_to_target_spot_notional >= 0.0,
							"I messed up the code: Market orders must be ran through here first"
						);
						assert!(
							left_to_target_normal_notional >= 0.0,
							"I messed up the code: Market orders must be ran through here first"
						);
					}
				};

				//NB: market-like orders MUST be ran first!
				update_target_orders(market_orders);

				match acquired.__spec.side {
					Side::Buy => {
						stop_orders.sort_by(|a, b| b.price().unwrap().partial_cmp(&a.price().unwrap()).unwrap());
						limit_orders.sort_by(|a, b| a.price().unwrap().partial_cmp(&b.price().unwrap()).unwrap());
					}
					Side::Sell => {
						stop_orders.sort_by(|a, b| a.price().unwrap().partial_cmp(&b.price().unwrap()).unwrap());
						limit_orders.sort_by(|a, b| b.price().unwrap().partial_cmp(&a.price().unwrap()).unwrap());
					}
				}
				update_target_orders(stop_orders);
				update_target_orders(limit_orders);

				target_orders.update_orders(new_target_orders, position_callback.clone()).await;
			}};
		}

		//TODO!: figure out abort when all closed.
		loop {
			select! {
				Some(protocol_orders) = rx_orders.recv() => {
					info!("{:?} sent orders: {:?}", protocol_orders.produced_by, protocol_orders.apply_mask(protocol_orders.empty_mask(), 0.0)); //dbg
					all_requested.lock().unwrap().insert(protocol_orders.produced_by.clone(), protocol_orders.clone());
					update_unrolled(protocol_orders.produced_by.clone());
					recalculate_target_orders!();
				},
				Some(fills_vec) = rx_fills.recv() => {
					info!("Received fills: {:?}", fills_vec);
					for f in fills_vec {
						let (protocol_order_id, filled_notional) = f;
						closed_notional += filled_notional;
						all_fills.lock().unwrap().insert(protocol_order_id.uuid, filled_notional);
						update_unrolled(protocol_order_id.produced_by.clone());
					}
					recalculate_target_orders!();
				},
				else => break,
			}
		}

		Ok(Self {
			_acquisition: acquired,
			protocols_spec: protocols,
			closed_notional,
		})
	}
}

//pub struct PositionClosed {
//	_followup: PositionFollowup,
//	t_closed: DateTime<Utc>,
//}
