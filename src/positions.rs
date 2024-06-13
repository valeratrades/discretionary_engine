use crate::config::AppConfig;
use crate::exchange_apis::order_types::Fill;
use crate::exchange_apis::order_types::{ConceptualOrder, ConceptualOrderType, Order, OrderType, ProtocolOrderId};
use crate::exchange_apis::{binance, HubPayload, Symbol};
use crate::protocols::{FollowupProtocol, ProtocolOrders, ProtocolType};
use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::str::FromStr;
use std::sync::{Arc, Mutex};
use tokio::select;
use tokio::sync::mpsc;
use tracing::{info, instrument};
use uuid::Uuid;
use v_utils::trades::Side;

/// What the Position *is*_
#[derive(Debug, Clone)]
pub struct PositionSpec {
	pub id: Uuid,
	pub asset: String,
	pub side: Side,
	pub size_usdt: f64,
}
impl PositionSpec {
	pub fn new(asset: String, side: Side, size_usdt: f64) -> Self {
		Self {
			id: Uuid::new_v4(),
			asset,
			side,
			size_usdt,
		}
	}
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
#[derive(Debug, Default, derive_new::new)]
struct TargetOrders {
	stop_orders_total_notional: f64,
	normal_orders_total_notional: f64,
	market_orders_total_notional: f64,
	//total_usd: f64,
	orders: Vec<ConceptualOrder<ProtocolOrderId>>,
}
impl TargetOrders {
	// if we get an error because we did not pass the correct uuid from the last fill message, we just drop the task, as we will be forced to run with a correct value very soon.
	/// Never fails, instead the errors are sent over the channel.
	fn update_orders(&mut self, orders: Vec<ConceptualOrder<ProtocolOrderId>>) -> Vec<ConceptualOrder<ProtocolOrderId>> {
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
		self.orders.clone()
	}
	//TODO!!!!!!!!!: fill channel. Want to receive data on every fill alongside the protocol_order_id, which is required when sending the update_orders() request, defined right above this.
}

#[derive(Clone, Debug, Default, derive_new::new)]
pub struct PositionCallback {
	pub last_fill_key: Uuid,
	fills: Vec<Fill<ProtocolOrderId>>,
}

impl PositionFollowup {
	#[instrument]
	pub async fn do_followup(acquired: PositionAcquisition, protocols: Vec<FollowupProtocol>, hub_tx: mpsc::Sender<HubPayload>) -> Result<Self> {
		let mut counted_subtypes: HashMap<ProtocolType, usize> = HashMap::new();
		for protocol in &protocols {
			let subtype = protocol.get_subtype();
			*counted_subtypes.entry(subtype).or_insert(0) += 1;
		}

		let (tx_orders, mut rx_orders) = mpsc::channel::<ProtocolOrders>(256);
		for protocol in protocols.clone() {
			protocol.attach(tx_orders.clone(), &acquired.__spec)?;
		}

		let (tx_fills, mut rx_fills) = mpsc::channel::<PositionCallback>(256);

		let all_requested: Arc<Mutex<HashMap<String, ProtocolOrders>>> = Arc::new(Mutex::new(HashMap::new()));
		let all_requested_unrolled: Arc<Mutex<HashMap<String, Vec<ConceptualOrder<ProtocolOrderId>>>>> = Arc::new(Mutex::new(HashMap::new()));
		let mut closed_notional = 0.0;
		let mut target_orders = TargetOrders::default();

		let all_fills: Arc<Mutex<HashMap<String, Vec<f64>>>> = Arc::new(Mutex::new(HashMap::new()));
		let mut last_fill_key = Uuid::default();

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

				let updated_orders = target_orders.update_orders(new_target_orders);

				let hub_payload = HubPayload::new(last_fill_key, acquired.__spec.id, updated_orders, tx_fills.clone());
				match hub_tx.send(hub_payload).await {
					Ok(_) => {}
					Err(e) => {
						info!("Error sending orders: {:?}", e);
						panic!(); //dbg
					}
				}
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
				Some(position_callback) = rx_fills.recv() => {
					info!("Received position callback: {position_callback:?}",);
					last_fill_key = position_callback.last_fill_key;
					for f in &position_callback.fills {
						closed_notional += f.filled_notional;
						{
							let mut all_fills_guard = all_fills.lock().unwrap();
							let protocol_fills = all_fills_guard.entry(f.id.protocol_id.clone()).or_insert_with(|| {
								let protocol_id = f.id.protocol_id.clone();
								all_requested.lock().unwrap().get(&protocol_id).unwrap().empty_mask()
							});

							protocol_fills[f.id.ordinal] += f.filled_notional;
						}

						update_unrolled(f.id.protocol_id.clone());
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
