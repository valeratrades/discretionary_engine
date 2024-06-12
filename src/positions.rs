use crate::config::AppConfig;
use crate::exchange_apis::order_types::{ConceptualOrder, ConceptualOrderType, Order, OrderType, ProtocolOrderId};
use crate::exchange_apis::{binance, Symbol};
use crate::protocols::{FollowupProtocol, ProtocolOrders, ProtocolType};
use anyhow::Result;
use derive_new::new;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::str::FromStr;
use std::sync::{Arc, Mutex};
use tokio::select;
use tracing::{info, instrument};
use uuid::Uuid;
use v_utils::trades::Side;

/// What the Position *is*_
#[derive(Debug, Clone, new)]
pub struct PositionSpec {
	pub asset: String,
	pub side: Side,
	pub size_usdt: f64,
}

#[allow(dead_code)]
#[derive(Debug)]
pub struct PositionAcquisition {
	__spec: PositionSpec,
	target_notional: f64,
	acquired_notional: f64,
	protocols_spec: Option<String>, //Vec<AcquisitionProtocol>,
}
impl PositionAcquisition {
	//dbg
	#[allow(clippy::unused_async)]
	pub async fn dbg_new(spec: PositionSpec) -> Result<Self> {
		Ok(Self {
			__spec: spec,
			target_notional: 20.0,
			acquired_notional: 20.0,
			protocols_spec: None,
		})
	}

	pub async fn do_acquisition(spec: PositionSpec, config: &AppConfig) -> Result<Self> {
		let coin = spec.asset.clone();
		let symbol = Symbol::from_str(format!("{coin}-USDT-BinanceFutures").as_str())?;

		let current_price = binance::futures_price(&coin).await?;
		let coin_quantity = spec.size_usdt / current_price;

		let mut current_state = Self {
			__spec: spec.clone(),
			target_notional: coin_quantity, //BUG: on very small order sizes, the mismatch between the size we're requesting and adjusted_qty we trim towards to satisfy exchange requirements, could be troublesome
			acquired_notional: 0.0,
			protocols_spec: None,
		};

		let order = Order::new(Uuid::new_v4(), OrderType::Market, symbol.clone(), spec.side, coin_quantity);

		let qty = order.qty_notional;
		todo!();
		//crate::exchange_apis::binance::dirty_hardcoded_exec(order, config).await?;
		current_state.acquired_notional += qty;

		//let order_id = binance::post_futures_order(full_key.clone(), full_secret.clone(), order).await?;
		////info!(target: "/tmp/discretionary_engine.lock", "placed order: {:?}", order_id);
		//loop {
		//	let r = binance::poll_futures_order(full_key.clone(), full_secret.clone(), order_id, symbol.to_string()).await?;
		//	if r.status == binance::OrderStatus::Filled {
		//		let order_notional = r.origQty.parse::<f64>()?;
		//		current_state.acquired_notional += order_notional;
		//		break;
		//	}
		//}

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
#[derive(Debug)]
struct TargetOrders {
	stop_orders_total_notional: f64,
	normal_orders_total_notional: f64,
	market_orders_total_notional: f64,
	//total_usd: f64,
	orders: Vec<ConceptualOrder<ProtocolOrderId>>,
	hub_tx: tokio::sync::mpsc::Sender<(Vec<ConceptualOrder<ProtocolOrderId>>, PositionCallback)>,
}
impl TargetOrders {
	pub fn new(hub_tx: tokio::sync::mpsc::Sender<(Vec<ConceptualOrder<ProtocolOrderId>>, PositionCallback)>) -> Self {
		Self {
			stop_orders_total_notional: 0.0,
			normal_orders_total_notional: 0.0,
			market_orders_total_notional: 0.0,
			orders: Vec::new(),
			hub_tx,
		}
	}
}
impl TargetOrders {
	// if we get an error because we did not pass the correct uuid from the last fill message, we just drop the task, as we will be forced to run with a correct value very soon.
	/// Never fails, instead the errors are sent over the channel.
	async fn update_orders(&mut self, orders: Vec<ConceptualOrder<ProtocolOrderId>>, position_callback: PositionCallback) {
		{
			let mut new_orders = Vec::new();
			for order in orders.into_iter() {
				match order.order_type {
					ConceptualOrderType::StopMarket(_) => self.stop_orders_total_notional += order.qty_notional,
					ConceptualOrderType::Limit(_) => self.normal_orders_total_notional += order.qty_notional,
					ConceptualOrderType::Market(_) => self.market_orders_total_notional += order.qty_notional,
				}
				new_orders.push(order);
			}
			self.orders = new_orders;
		}
		match self.hub_tx.send((self.orders.clone(), position_callback)).await {
			Ok(_) => {}
			Err(e) => {
				info!("Error sending orders: {:?}", e);
				panic!(); //dbg
			}
		};
	}
	//TODO!!!!!!!!!: fill channel. Want to receive data on every fill alongside the protocol_order_id, which is required when sending the update_orders() request, defined right above this.
}

#[derive(Debug, Clone, new)]
pub struct PositionCallback {
	pub sender: tokio::sync::mpsc::Sender<Vec<(ProtocolOrderId, f64)>>, // stands for "this nominal qty filled on this protocol order"
	pub position_uuid: Uuid,
}

impl PositionFollowup {
	#[instrument]
	pub async fn do_followup(
		acquired: PositionAcquisition,
		protocols: Vec<FollowupProtocol>,
		hub_tx: tokio::sync::mpsc::Sender<(Vec<ConceptualOrder<ProtocolOrderId>>, PositionCallback)>,
	) -> Result<Self> {
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
		let all_requested_unrolled: Arc<Mutex<HashMap<String, Vec<ConceptualOrder<ProtocolOrderId>>>>> = Arc::new(Mutex::new(HashMap::new()));
		let mut closed_notional = 0.0;
		let mut target_orders = TargetOrders::new(hub_tx);

		let all_fills: Arc<Mutex<HashMap<String, Vec<f64>>>> = Arc::new(Mutex::new(HashMap::new()));

		let update_unrolled = |update_on_protocol: String| {
			let protocol = FollowupProtocol::from_str(&update_on_protocol).unwrap();
			let subtype = protocol.get_subtype();
			let size_multiplier = 1.0 / *counted_subtypes.get(&subtype).unwrap() as f64;
			let total_controlled_size = acquired.acquired_notional * size_multiplier;

			let target_protocol_orders = &all_requested.lock().unwrap()[&update_on_protocol];
			let mask: Vec<f64> = {
				let all_fills_guard = all_fills.lock().unwrap();
				match all_fills_guard.get(&update_on_protocol) {
					Some(mask) => mask.to_vec(),
					None => target_protocol_orders.empty_mask(),
				}
			};
			let order_batch = target_protocol_orders.apply_mask(&mask, total_controlled_size);
			all_requested_unrolled.lock().unwrap().insert(update_on_protocol, order_batch);
		};

		macro_rules! recalculate_target_orders {
			() => {{
				let mut market_orders = Vec::new();
				let mut stop_orders = Vec::new();
				let mut limit_orders = Vec::new();
				for (_key, value) in all_requested_unrolled.lock().unwrap().clone() {
					value.into_iter().for_each(|o| match o.order_type {
						ConceptualOrderType::StopMarket(_) => stop_orders.push(o),
						ConceptualOrderType::Limit(_) => limit_orders.push(o),
						ConceptualOrderType::Market(_) => market_orders.push(o),
					});
				}

				let mut left_to_target_full_notional = acquired.acquired_notional - closed_notional;
				let (mut left_to_target_spot_notional, mut left_to_target_normal_notional) = (left_to_target_full_notional, left_to_target_full_notional);
				let mut new_target_orders: Vec<ConceptualOrder<ProtocolOrderId>> = Vec::new();

				// orders should be all of the same conceptual type (no idea how to enforce it)
				let mut update_target_orders = |orders: Vec<ConceptualOrder<ProtocolOrderId>>| {
					for order in orders {
						let notional = order.qty_notional;
						let compare_against = match order.order_type {
							ConceptualOrderType::StopMarket(_) => left_to_target_spot_notional,
							ConceptualOrderType::Limit(_) => left_to_target_normal_notional,
							ConceptualOrderType::Market(_) => left_to_target_full_notional,
						};
						let mut order = order.clone();
						if notional > compare_against {
							order.qty_notional = compare_against;
						}
						new_target_orders.push(order.clone());
						match order.order_type {
							ConceptualOrderType::StopMarket(_) => left_to_target_spot_notional -= notional,
							ConceptualOrderType::Limit(_) => left_to_target_normal_notional -= notional,
							ConceptualOrderType::Market(_) => {
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

		//TODO!!: move the handling of inner values inside a tokio task (protocol orders and fills update data inside an enum, and send it to the task)

		//TODO!: figure out abort when all closed.
		loop {
			select! {
				Some(protocol_orders) = rx_orders.recv() => {
					info!("{:?} sent orders: {:?}", protocol_orders.protocol_id, protocol_orders.__orders); //dbg
					all_requested.lock().unwrap().insert(protocol_orders.protocol_id.clone(), protocol_orders.clone());
					update_unrolled(protocol_orders.protocol_id.clone());
					recalculate_target_orders!();
				},
				Some(fills_vec) = rx_fills.recv() => {
					info!("Received fills: {:?}", fills_vec);
					for f in fills_vec {
						let (protocol_order_id, filled_notional) = f;
						closed_notional += filled_notional;
						{
							let mut all_fills_guard = all_fills.lock().unwrap();
							let protocol_fills = all_fills_guard.entry(protocol_order_id.protocol_id.clone()).or_insert_with(|| {
								let protocol_id = protocol_order_id.protocol_id.clone();
								all_requested.lock().unwrap().get(&protocol_id).unwrap().empty_mask()
							});

							protocol_fills[protocol_order_id.ordinal] += filled_notional;
						}

						update_unrolled(protocol_order_id.protocol_id.clone());
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

#[derive(Clone, Debug, Default, derive_new::new, PartialEq, Hash, Serialize, Deserialize)]
pub struct PositionOrderId {
	pub position_id: Uuid,
	pub protocol_id: String,
	pub ordinal: usize,
}
impl PositionOrderId {
	pub fn new_from_protocol_id(position_id: Uuid, poid: ProtocolOrderId) -> Self {
		Self::new(position_id, poid.protocol_id, poid.ordinal)
	}
}

//pub struct PositionClosed {
//	_followup: PositionFollowup,
//	t_closed: DateTime<Utc>,
//}
