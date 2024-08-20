use std::{collections::HashMap, str::FromStr};

use anyhow::Result;
use serde::{Deserialize, Serialize};
use tokio::{select, sync::mpsc, task::JoinSet};
use tracing::{info, instrument};
use uuid::Uuid;
use v_utils::trades::Side;

use crate::{
	config::AppConfig,
	exchange_apis::{
		binance,
		order_types::{ConceptualOrder, ConceptualOrderType, Order, OrderType, ProtocolOrderId},
		HubRx, Symbol,
	},
	protocols::{FollowupProtocol, ProtocolDynamicInfo, ProtocolFill, ProtocolOrders, ProtocolType},
};

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
	protocols_spec: Option<String>, // Vec<AcquisitionProtocol>,
}
impl PositionAcquisition {
	// dbg
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
		let coin_quantity = 20.0; // dbg spec.size_usdt / current_price;

		let mut current_state = Self {
			__spec: spec.clone(),
			target_notional: coin_quantity, /* BUG: on very small order sizes, the mismatch between the size we're requesting and adjusted_qty we trim towards to satisfy exchange requirements, could be troublesome */
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

		// TODO!!!!: implement Acquisition Protocol: delayed buy-limit
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

#[derive(Debug, Clone, derive_new::new)]
pub struct PositionCallback {
	pub sender: tokio::sync::mpsc::Sender<Vec<ProtocolFill>>, // stands for "this nominal qty filled on this protocol order"
	pub position_id: Uuid,
}

impl PositionFollowup {
	#[instrument]
	pub async fn do_followup(acquired: PositionAcquisition, protocols: Vec<FollowupProtocol>, hub_tx: mpsc::Sender<HubRx>) -> Result<Self> {
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
		let mut last_fill_key = Uuid::default();

		fn recalculate_target_orders(
			counted_subtypes: &HashMap<ProtocolType, usize>,
			left_to_target_notional: f64,
			side: Side,
			dyn_info: &HashMap<String, ProtocolDynamicInfo>,
		) -> Vec<ConceptualOrder<ProtocolOrderId>> {
			let mut market_orders = Vec::new();
			let mut stop_orders = Vec::new();
			let mut limit_orders = Vec::new();
			for (protocol_spec_str, info) in dyn_info.iter() {
				let subtype = FollowupProtocol::from_str(protocol_spec_str).unwrap().get_subtype();
				let matching_subtype_n = counted_subtypes.get(&subtype).unwrap();
				let conceptual_orders = info.conceptual_orders(*matching_subtype_n, left_to_target_notional);
				conceptual_orders.into_iter().for_each(|o| match o.order_type {
					ConceptualOrderType::StopMarket(_) => stop_orders.push(o),
					ConceptualOrderType::Limit(_) => limit_orders.push(o),
					ConceptualOrderType::Market(_) => market_orders.push(o),
				});
			}

			/// NB: Market-like orders MUST be ran first
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

			let mut left_to_target_marketlike_notional = left_to_target_notional;
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
		
		async fn update_orders(hub_tx: mpsc::Sender<HubRx>, position_callback: PositionCallback, last_fill_key: Uuid, counted_subtypes: &HashMap<ProtocolType, usize>, left_to_target_notional: f64, position_side: Side, dyn_info: &HashMap<String, ProtocolDynamicInfo>) -> Result<()> {
			let new_target_orders = recalculate_target_orders(&counted_subtypes, left_to_target_notional, position_side, &dyn_info);
			match hub_tx.send(HubRx::new(last_fill_key, new_target_orders, position_callback)).await {
				Ok(_) => {}
				Err(e) => {
					info!("Error sending orders: {:?}", e);
					return Err(e.into());
				}
			};
			Ok(())
		}

		async fn process_protocol_orders_update(protocol_orders_update: ProtocolOrders, dyn_info: &mut HashMap<String, ProtocolDynamicInfo>) -> Result<()> {
			info!("{:?} sent orders: {:?}", protocol_orders_update.protocol_id, protocol_orders_update.__orders);
			if let Some(protocol_info) = dyn_info.get_mut(&protocol_orders_update.protocol_id) {
				protocol_info.update_orders(protocol_orders_update.clone());
			} else {
				dyn_info.insert(protocol_orders_update.protocol_id.clone(), ProtocolDynamicInfo::new(protocol_orders_update.clone()));
			}
			Ok(())
		}

		async fn process_fills_update(last_fill_key: &mut Uuid, fills_vec: Vec<ProtocolFill>, dyn_info: &mut HashMap<String, ProtocolDynamicInfo>, closed_notional: &mut f64) -> Result<()> {
			info!("Received fills: {:?}", fills_vec);
			for f in fills_vec {
				*last_fill_key = f.key;
				let (protocol_order_id, filled_notional) = (f.id, f.qty);
				*closed_notional += filled_notional;
				{
					let protocol_info = dyn_info.get_mut(&protocol_order_id.protocol_id).unwrap();
					protocol_info.update_fill_at(protocol_order_id.ordinal, filled_notional);
				}
			}
			Ok(())
		}

		loop {
			select! {
				Some(protocol_orders) = rx_orders.recv() => {
					process_protocol_orders_update(protocol_orders, &mut protocols_dynamic_info).await?;
					update_orders(hub_tx.clone(), position_callback.clone(), last_fill_key, &counted_subtypes, acquired.acquired_notional - closed_notional, acquired.__spec.side, &protocols_dynamic_info).await?;
				},
				Some(fills_vec) = rx_fills.recv() => {
					process_fills_update(&mut last_fill_key, fills_vec, &mut protocols_dynamic_info, &mut closed_notional).await?;
					if closed_notional >= acquired.acquired_notional {
						break;
					}
					update_orders(hub_tx.clone(), position_callback.clone(), last_fill_key, &counted_subtypes, acquired.acquired_notional - closed_notional, acquired.__spec.side, &protocols_dynamic_info).await?;
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

// pub struct PositionClosed {
// 	_followup: PositionFollowup,
// 	t_closed: DateTime<Utc>,
//}
