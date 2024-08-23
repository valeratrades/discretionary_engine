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
	protocols::{Protocol, ProtocolDynamicInfo, ProtocolFill, ProtocolOrders, ProtocolType},
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
	protocols_spec: Option<String>,
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

	#[instrument]
	pub async fn do_acquisition(spec: PositionSpec, protocols: Vec<Protocol>, hub_tx: mpsc::Sender<HubRx>) -> Result<Self> {
		let mut js = JoinSet::new();
		let (mut rx_orders, counted_subtypes) = init_protocols(&mut js, &protocols, &spec.asset, spec.side);

		// HACK
		let current_price = binance::futures_price(&spec.asset).await?;
		let target_coin_quantity = spec.size_usdt / current_price;

		let position_id = Uuid::now_v7();
		let (tx_fills, mut rx_fills) = mpsc::channel::<Vec<ProtocolFill>>(256);
		let position_callback = PositionCallback::new(tx_fills, position_id);

		let mut protocols_dynamic_info: HashMap<String, ProtocolDynamicInfo> = HashMap::new();
		let mut executed_notional = 0.0;
		let mut last_fill_key = Uuid::default();

		loop {
			select! {
				Some(protocol_orders) = rx_orders.recv() => {
					process_protocol_orders_update(protocol_orders, &mut protocols_dynamic_info).await?;
					update_orders(hub_tx.clone(), position_callback.clone(), last_fill_key, &counted_subtypes, target_coin_quantity - executed_notional, spec.side, &protocols_dynamic_info).await?;
				},
				Some(fills_vec) = rx_fills.recv() => {
					process_fills_update(&mut last_fill_key, fills_vec, &mut protocols_dynamic_info, &mut executed_notional).await?;
					if executed_notional >= target_coin_quantity {
						break;
					}
					update_orders(hub_tx.clone(), position_callback.clone(), last_fill_key, &counted_subtypes, target_coin_quantity - executed_notional, spec.side, &protocols_dynamic_info).await?;
				},
				Some(_) = js.join_next() => { unreachable!("All protocols are endless, this is here only for structured concurrency, as all tasks should be actively awaited.")},
				else => unreachable!("hub outlives positions"),
			}
		}

		tracing::debug!("Followup completed:\nFilled: {:?}\nTarget: {:?}", executed_notional, target_coin_quantity);
		Ok(Self {
			__spec: spec,
			target_notional: target_coin_quantity,
			acquired_notional: executed_notional,
			protocols_spec: None,
		})
	}
}

#[derive(Clone, Debug, Default, derive_new::new)]
pub struct PositionFollowup {
	_acquisition: PositionAcquisition,
	protocols_spec: Vec<Protocol>,
	closed_notional: f64,
}

#[derive(Debug, Clone, derive_new::new)]
pub struct PositionCallback {
	pub sender: mpsc::Sender<Vec<ProtocolFill>>,
	pub position_id: Uuid,
}

impl PositionFollowup {
	#[instrument]
	pub async fn do_followup(acquired: PositionAcquisition, protocols: Vec<Protocol>, hub_tx: mpsc::Sender<HubRx>) -> Result<Self> {
		let mut js = JoinSet::new();
		let (mut rx_orders, counted_subtypes) = init_protocols(&mut js, &protocols, &acquired.__spec.asset, !acquired.__spec.side);

		let position_id = Uuid::now_v7();
		let (tx_fills, mut rx_fills) = mpsc::channel::<Vec<ProtocolFill>>(256);
		let position_callback = PositionCallback::new(tx_fills, position_id);

		let mut protocols_dynamic_info: HashMap<String, ProtocolDynamicInfo> = HashMap::new();
		let mut executed_notional = 0.0;
		let mut last_fill_key = Uuid::default();

		loop {
			select! {
				Some(protocol_orders) = rx_orders.recv() => {
					process_protocol_orders_update(protocol_orders, &mut protocols_dynamic_info).await?;
					update_orders(hub_tx.clone(), position_callback.clone(), last_fill_key, &counted_subtypes, acquired.acquired_notional - executed_notional, acquired.__spec.side, &protocols_dynamic_info).await?;
				},
				Some(fills_vec) = rx_fills.recv() => {
					process_fills_update(&mut last_fill_key, fills_vec, &mut protocols_dynamic_info, &mut executed_notional).await?;
					if executed_notional >= acquired.acquired_notional {
						break;
					}
					update_orders(hub_tx.clone(), position_callback.clone(), last_fill_key, &counted_subtypes, acquired.acquired_notional - executed_notional, acquired.__spec.side, &protocols_dynamic_info).await?;
				},
				Some(_) = js.join_next() => { unreachable!("All protocols are endless, this is here only for structured concurrency, as all tasks should be actively awaited.")},
				else => unreachable!("hub outlives positions"),
			}
		}

		tracing::debug!("Followup completed:\nFilled: {:?}\nTarget: {:?}", executed_notional, acquired.target_notional);
		Ok(Self {
			_acquisition: acquired,
			protocols_spec: protocols,
			closed_notional: executed_notional,
		})
	}
}

fn init_protocols(parent_js: &mut JoinSet<Result<()>>, protocols: &[Protocol], asset: &str, protocols_side: Side) -> (mpsc::Receiver<ProtocolOrders>, HashMap<ProtocolType, usize>) {
	let (tx_orders, rx_orders) = mpsc::channel::<ProtocolOrders>(256);
	for protocol in protocols {
		protocol.attach(parent_js, tx_orders.clone(), asset.to_owned(), protocols_side).unwrap();
	}

	let mut counted_subtypes: HashMap<ProtocolType, usize> = HashMap::new();
	for protocol in protocols {
		let subtype = protocol.get_subtype();
		*counted_subtypes.entry(subtype).or_insert(0) += 1;
	}

	(rx_orders, counted_subtypes)
}

async fn update_orders(
	hub_tx: mpsc::Sender<HubRx>,
	position_callback: PositionCallback,
	last_fill_key: Uuid,
	counted_subtypes: &HashMap<ProtocolType, usize>,
	left_to_target_notional: f64,
	position_side: Side,
	dyn_info: &HashMap<String, ProtocolDynamicInfo>,
) -> Result<()> {
	let new_target_orders = recalculate_target_orders(counted_subtypes, left_to_target_notional, position_side, dyn_info);
	match hub_tx.send(HubRx::new(last_fill_key, new_target_orders, position_callback)).await {
		Ok(_) => {}
		Err(e) => {
			info!("Error sending orders: {:?}", e);
			return Err(e.into());
		}
	};
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
		let subtype = Protocol::from_str(protocol_spec_str).unwrap().get_subtype();
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

async fn process_protocol_orders_update(protocol_orders_update: ProtocolOrders, dyn_info: &mut HashMap<String, ProtocolDynamicInfo>) -> Result<()> {
	info!("{:?} sent orders: {:?}", protocol_orders_update.protocol_id, protocol_orders_update.__orders);
	if let Some(protocol_info) = dyn_info.get_mut(&protocol_orders_update.protocol_id) {
		protocol_info.update_orders(protocol_orders_update.clone());
	} else {
		dyn_info.insert(protocol_orders_update.protocol_id.clone(), ProtocolDynamicInfo::new(protocol_orders_update.clone()));
	}
	Ok(())
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
