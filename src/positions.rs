use crate::config::AppConfig;
use crate::exchange_apis::order_types::{ConceptualOrder, ConceptualOrderType, Order, OrderType, ProtocolOrderId};
use crate::exchange_apis::{binance, HubRx, Symbol};
use crate::protocols::{FollowupProtocol, ProtocolDynamicInfo, ProtocolFill, ProtocolOrders, ProtocolType};
use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::str::FromStr;
use tokio::select;
use tokio::sync::mpsc;
use tokio::task::JoinSet;
use tracing::{info, instrument};
use uuid::Uuid;
use v_utils::trades::Side;

/// What the Position *is*_
#[derive(Clone, Debug, Default, derive_new::new)]
pub struct PositionSpec {
	pub asset: String,
	pub side: Side,
	pub size_usdt: f64,
}

#[allow(dead_code)]
#[derive(Clone, Debug, Default, derive_new::new)]
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
		let coin_quantity = 20.0; //dbg spec.size_usdt / current_price;

		let mut current_state = Self {
			__spec: spec.clone(),
			target_notional: coin_quantity, //BUG: on very small order sizes, the mismatch between the size we're requesting and adjusted_qty we trim towards to satisfy exchange requirements, could be troublesome
			acquired_notional: 0.0,
			protocols_spec: None,
		};

		let order = Order::new(Uuid::now_v7(), OrderType::Market, symbol.clone(), spec.side, coin_quantity);

		// //dbg
		let full_key = config.binance.full_key.clone();
		let full_secret = config.binance.full_secret.clone();
		let position_order_id = PositionOrderId::new(Uuid::now_v7(), "mock_acquisition".to_string(), 0);
		let mock_position_order = Order::<PositionOrderId>::new(position_order_id, OrderType::Market, symbol.clone(), spec.side, order.qty_notional);
		let _binance_order = crate::exchange_apis::binance::post_futures_order(full_key.clone(), full_secret.clone(), &mock_position_order)
			.await
			.unwrap();
		// we just assume it worked
		//

		current_state.acquired_notional = coin_quantity;

		//TODO!!!!: implement Acquisition Protocol: delayed buy-limit
		// the action core is the same as Followup's

		Ok(current_state)
	}
}

#[derive(Clone, Debug, Default, derive_new::new)]
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
	hub_tx: tokio::sync::mpsc::Sender<HubRx>,
}
impl TargetOrders {
	pub fn new(hub_tx: tokio::sync::mpsc::Sender<HubRx>) -> Self {
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
	async fn update_orders(&mut self, last_filled_key: Uuid, orders: Vec<ConceptualOrder<ProtocolOrderId>>, position_callback: PositionCallback) {
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
		match self.hub_tx.send(HubRx::new(last_filled_key, self.orders.clone(), position_callback)).await {
			Ok(_) => {}
			Err(e) => {
				info!("Error sending orders: {:?}", e);
				panic!(); //dbg
			}
		};
	}
}

#[derive(Debug, Clone, derive_new::new)]
pub struct PositionCallback {
	pub sender: tokio::sync::mpsc::Sender<Vec<ProtocolFill>>, // stands for "this nominal qty filled on this protocol order"
	pub position_id: Uuid,
}

impl PositionFollowup {
	#[instrument]
	pub async fn do_followup(acquired: PositionAcquisition, protocols: Vec<FollowupProtocol>, hub_tx: tokio::sync::mpsc::Sender<HubRx>) -> Result<Self> {
		let mut counted_subtypes: HashMap<ProtocolType, usize> = HashMap::new();
		for protocol in &protocols {
			let subtype = protocol.get_subtype();
			*counted_subtypes.entry(subtype).or_insert(0) += 1;
		}

		let (tx_orders, mut rx_orders) = mpsc::channel::<ProtocolOrders>(32);
		let mut set = JoinSet::new();
		for protocol in protocols.clone() {
			protocol.attach(&mut set, tx_orders.clone(), &acquired.__spec)?;
		}

		let position_id = Uuid::now_v7();
		let (tx_fills, mut rx_fills) = tokio::sync::mpsc::channel::<Vec<ProtocolFill>>(32);
		let position_callback = PositionCallback::new(tx_fills, position_id);

		let mut protocols_dynamic_info: HashMap<String, ProtocolDynamicInfo> = HashMap::new();

		let mut closed_notional = 0.0;
		let mut target_orders = TargetOrders::new(hub_tx);
		let mut last_fill_key = Uuid::default();

		fn recalculate_target_orders(
			counted_subtypes: &HashMap<ProtocolType, usize>,
			left_to_acquire_notional: f64,
			side: Side,
			dyn_info: &HashMap<String, ProtocolDynamicInfo>,
		) -> Vec<ConceptualOrder<ProtocolOrderId>> {
			let mut market_orders = Vec::new();
			let mut stop_orders = Vec::new();
			let mut limit_orders = Vec::new();
			for (protocol_spec_str, info) in dyn_info.iter() {
				let subtype = FollowupProtocol::from_str(protocol_spec_str).unwrap().get_subtype();
				let matching_subtype_n = counted_subtypes.get(&subtype).unwrap();
				let conceptual_orders = info.conceptual_orders(*matching_subtype_n, left_to_acquire_notional);
				conceptual_orders.into_iter().for_each(|o| match o.order_type {
					ConceptualOrderType::StopMarket(_) => stop_orders.push(o),
					ConceptualOrderType::Limit(_) => limit_orders.push(o),
					ConceptualOrderType::Market(_) => market_orders.push(o),
				});
			}

			///NB: Market-like orders MUST be ran first
			fn update_order_selection(extendable: &mut Vec<ConceptualOrder<ProtocolOrderId>>, incoming: &[ConceptualOrder<ProtocolOrderId>], left_to_target: &mut f64) {
				for order in incoming {
					let notional = order.qty_notional;
					let mut order = order.clone();
					if notional > *left_to_target {
						order.qty_notional = *left_to_target;
					}
					extendable.push(order.clone());
					*left_to_target -= notional;
				}
			}

			let mut new_target_orders: Vec<ConceptualOrder<ProtocolOrderId>> = Vec::new();

			let mut left_to_target_marketlike_notional = left_to_acquire_notional;
			update_order_selection(&mut new_target_orders, &market_orders, &mut left_to_target_marketlike_notional);

			match side {
				Side::Buy => {
					stop_orders.sort_by(|a, b| b.price().unwrap().partial_cmp(&a.price().unwrap()).unwrap());
					limit_orders.sort_by(|a, b| a.price().unwrap().partial_cmp(&b.price().unwrap()).unwrap());
				}
				Side::Sell => {
					stop_orders.sort_by(|a, b| a.price().unwrap().partial_cmp(&b.price().unwrap()).unwrap());
					limit_orders.sort_by(|a, b| b.price().unwrap().partial_cmp(&a.price().unwrap()).unwrap());
				}
			}
			let mut left_to_target_stop_notional = left_to_target_marketlike_notional;
			update_order_selection(&mut new_target_orders, &stop_orders, &mut left_to_target_stop_notional);
			let mut left_to_target_limit_notional = left_to_target_marketlike_notional;
			update_order_selection(&mut new_target_orders, &limit_orders, &mut left_to_target_limit_notional);

			new_target_orders
		}

		loop {
			select! {
				Some(protocol_orders) = rx_orders.recv() => {
					info!("{:?} sent orders: {:?}", protocol_orders.protocol_id, protocol_orders.__orders); //dbg
					if let Some(protocol_info) = protocols_dynamic_info.get_mut(&protocol_orders.protocol_id) {
						protocol_info.update_orders(protocol_orders.clone());
					} else {
						protocols_dynamic_info.insert(protocol_orders.protocol_id.clone(), ProtocolDynamicInfo::new(protocol_orders.clone()));
					}
					let new_target_orders = recalculate_target_orders(&counted_subtypes, acquired.target_notional - closed_notional, acquired.__spec.side, &protocols_dynamic_info);
					target_orders .update_orders(last_fill_key, new_target_orders, position_callback.clone()) .await;
				},
				Some(fills_vec) = rx_fills.recv() => {
					info!("Received fills: {:?}", fills_vec);
					for f in fills_vec {
						last_fill_key = f.key;
						let (protocol_order_id, filled_notional) = (f.id, f.qty);
						closed_notional += filled_notional;
						{
							let protocol_info = protocols_dynamic_info.get_mut(&protocol_order_id.protocol_id).unwrap();
							protocol_info.update_fill_at(protocol_order_id.ordinal, filled_notional);
						}
					}
					if closed_notional >= acquired.target_notional {
						break;
					}
					let new_target_orders = recalculate_target_orders(&counted_subtypes, acquired.target_notional - closed_notional, acquired.__spec.side, &protocols_dynamic_info);
					target_orders .update_orders(last_fill_key, new_target_orders, position_callback.clone()) .await;
				},
				Some(_) = set.join_next() => { unreachable!("All protocols are endless, this is here only for structured concurrency, as all tasks should be actively awaited.")},
				else => unreachable!("hub outlives positions"),
			}
		}

		tracing::debug!("Followup completed:\nFilled: {:?}\nTarget: {:?}", closed_notional, acquired.target_notional);
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
