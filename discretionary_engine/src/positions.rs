use std::{collections::HashMap, sync::Arc};

use color_eyre::eyre::Result;
use serde::{Deserialize, Serialize};
use tokio::{select, sync::mpsc, task::JoinSet};
use tracing::{debug, field::Empty, info, instrument, Span};
use uuid::Uuid;
use v_utils::trades::Side;

use crate::{
	exchange_apis::{
		binance,
		exchanges::Exchanges,
		hub::PositionToHub,
		order_types::{ConceptualOrder, ConceptualOrderPercents, ConceptualOrderType, ProtocolOrderId},
	},
	protocols::{Protocol, ProtocolDynamicInfo, ProtocolFills, ProtocolOrders, ProtocolType, RecalculateOrdersPerOrderInfo},
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
		let current_price = binance::futures_price(&spec.asset).await?;
		let target_coin_quantity = spec.size_usdt / current_price;
		Ok(Self {
			__spec: spec,
			target_notional: target_coin_quantity,
			acquired_notional: target_coin_quantity,
			protocols_spec: None,
		})
	}

	#[instrument(skip(hub_tx, exchanges))]
	pub async fn do_acquisition(spec: PositionSpec, protocols: Vec<Protocol>, hub_tx: mpsc::Sender<PositionToHub>, exchanges: Arc<Exchanges>) -> Result<Self> {
		let mut js = JoinSet::new();
		let (mut rx_orders, mut position_protocols_dynamic_info) = init_protocols(&mut js, &protocols, &spec.asset, spec.side);

		// HACK
		let current_price = binance::futures_price(&spec.asset).await?;
		let target_coin_quantity = spec.size_usdt / current_price;

		let position_id = Uuid::now_v7();
		let (tx_fills, mut rx_fills) = mpsc::channel::<ProtocolFills>(256);
		let position_callback = HubToPosition::new(tx_fills, position_id);

		let mut executed_notional = 0.0;
		let mut last_fill_key = Uuid::default();

		let min_qty_any_ordertype = Exchanges::min_qty_any_ordertype(exchanges.clone(), &spec.asset);

		loop {
			select! {
				Some(protocol_orders) = rx_orders.recv() => {
					process_protocol_orders_update(protocol_orders, &mut position_protocols_dynamic_info).await?;
					let new_target_orders = recalculate_protocol_orders(&spec.asset, min_qty_any_ordertype, target_coin_quantity - executed_notional, spec.side, &position_protocols_dynamic_info, exchanges.clone());
					send_orders_to_hub(hub_tx.clone(), position_callback.clone(), last_fill_key, new_target_orders).await?;
				},
				Some(protocol_fills) = rx_fills.recv() => {
					last_fill_key = protocol_fills.key;
					process_fills_update(protocol_fills, &mut position_protocols_dynamic_info, &mut executed_notional).await?;
					debug!(executed_notional);
					if executed_notional > target_coin_quantity - min_qty_any_ordertype {
						break;
					}
					let new_target_orders = recalculate_protocol_orders(&spec.asset,min_qty_any_ordertype,  target_coin_quantity - executed_notional, spec.side, &position_protocols_dynamic_info, exchanges.clone());
					send_orders_to_hub(hub_tx.clone(), position_callback.clone(), last_fill_key, new_target_orders).await?;
				},
				Some(_) = js.join_next() => { unreachable!("All protocols are endless, this is here only for structured concurrency, as all tasks should be actively awaited.")},
				else => unreachable!("hub outlives positions"),
			}
		}

		info!("Acquisition completed:\nFilled: {:?}\nTarget: {:?}", executed_notional, target_coin_quantity);
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
pub struct HubToPosition {
	pub sender: mpsc::Sender<ProtocolFills>,
	pub position_id: Uuid,
}

impl PositionFollowup {
	#[instrument(skip(hub_tx, exchanges_arc))]
	pub async fn do_followup(acquired: PositionAcquisition, protocols: Vec<Protocol>, hub_tx: mpsc::Sender<PositionToHub>, exchanges_arc: Arc<Exchanges>) -> Result<Self> {
		let mut js = JoinSet::new();
		let (mut rx_orders, mut position_protocols_dynamic_info) = init_protocols(&mut js, &protocols, &acquired.__spec.asset, !acquired.__spec.side);

		let position_id = Uuid::now_v7();
		let (tx_fills, mut rx_fills) = mpsc::channel::<ProtocolFills>(256);
		let position_callback = HubToPosition::new(tx_fills, position_id);

		let mut executed_notional = 0.0;
		let mut last_fill_key = Uuid::default();

		let min_qty_any_ordertype = Exchanges::min_qty_any_ordertype(exchanges_arc.clone(), &acquired.__spec.asset);

		loop {
			select! {
				Some(protocol_orders) = rx_orders.recv() => {
					process_protocol_orders_update(protocol_orders, &mut position_protocols_dynamic_info).await?;
					let new_target_orders = recalculate_protocol_orders(&acquired.__spec.asset, min_qty_any_ordertype, acquired.acquired_notional - executed_notional, acquired.__spec.side, &position_protocols_dynamic_info, exchanges_arc.clone());
					send_orders_to_hub(hub_tx.clone(), position_callback.clone(), last_fill_key, new_target_orders).await?;
				},
				Some(protocol_fills) = rx_fills.recv() => {
					last_fill_key = protocol_fills.key;
					process_fills_update(protocol_fills, &mut position_protocols_dynamic_info, &mut executed_notional).await?;
					debug!(executed_notional);
					if executed_notional > acquired.acquired_notional - min_qty_any_ordertype {
						break;
					}
					let new_target_orders = recalculate_protocol_orders(&acquired.__spec.asset, min_qty_any_ordertype, acquired.acquired_notional - executed_notional, acquired.__spec.side, &position_protocols_dynamic_info, exchanges_arc.clone());
					send_orders_to_hub(hub_tx.clone(), position_callback.clone(), last_fill_key, new_target_orders).await?;
				},
				Some(_) = js.join_next() => { unreachable!("All protocols are endless, this is here only for structured concurrency, as all tasks should be actively awaited.")},
				else => unreachable!("hub outlives positions"),
			}
		}

		info!("Followup completed:\nFilled: {:?}\nTarget: {:?}", executed_notional, acquired.target_notional);
		Ok(Self {
			_acquisition: acquired,
			protocols_spec: protocols,
			closed_notional: executed_notional,
		})
	}
}

#[instrument(skip(parent_js))]
fn init_protocols(parent_js: &mut JoinSet<Result<()>>, protocols: &[Protocol], asset: &str, protocols_side: Side) -> (mpsc::Receiver<ProtocolOrders>, PositionProtocolsDynamicInfo) {
	let (tx_orders, rx_orders) = mpsc::channel::<ProtocolOrders>(256);
	for protocol in protocols {
		protocol.attach(parent_js, tx_orders.clone(), asset.to_owned(), protocols_side).unwrap();
	}

	let mut protocol_type_mapped_order: HashMap<ProtocolType, HashMap<String, Option<ProtocolDynamicInfo>>> = HashMap::new();
	for protocol in protocols {
		let subtype = protocol.get_type();
		let map_entry = protocol_type_mapped_order.entry(subtype).or_default();
		map_entry.insert(protocol.signature(), None);
	}

	(rx_orders, PositionProtocolsDynamicInfo(protocol_type_mapped_order))
}

#[instrument(skip(hub_tx))]
async fn send_orders_to_hub(
	hub_tx: mpsc::Sender<PositionToHub>,
	position_callback: HubToPosition,
	last_fill_key: Uuid,
	new_target_orders: Vec<ConceptualOrder<ProtocolOrderId>>,
) -> Result<()> {
	match hub_tx.send(PositionToHub::new(last_fill_key, new_target_orders, position_callback)).await {
		Ok(_) => {}
		Err(e) => {
			debug!("Error sending orders: {:?}", e);
			return Err(e.into());
		}
	};
	Ok(())
}

//? is it worth it to change the insides of a function for better logging?
#[instrument(skip(dyn_info), fields(accessed_info_fields = Empty))]
async fn process_fills_update(protocol_fills: ProtocolFills, dyn_info: &mut PositionProtocolsDynamicInfo, closed_notional: &mut f64) -> Result<()> {
	let mut accessed_info_fields = Vec::new();

	for f in protocol_fills.fills {
		let (protocol_order_id, filled_notional) = (f.id, f.qty);
		*closed_notional += filled_notional;

		for (_protocol_type, on_type_infos) in dyn_info.iter_mut() {
			if let Some(found_protocol_info) = on_type_infos.get_mut(&protocol_order_id.protocol_signature) {
				found_protocol_info
					.as_mut()
					.expect("Can't receive fill if it hasn't posted orders. Thus guaranteed to be `Some` here.")
					.update_fill_at(protocol_order_id.ordinal, filled_notional);

				accessed_info_fields.push(found_protocol_info.clone());
				Span::current().record("accessed_info_fields", format!("{:?}", accessed_info_fields));
			}
		}
	}
	debug!("Position processed fills");
	Ok(())
}

#[derive(Clone, Debug, Default)]
struct PositionProtocolsDynamicInfo(pub HashMap<ProtocolType, HashMap<String, Option<ProtocolDynamicInfo>>>);
impl PositionProtocolsDynamicInfo {
	pub fn iter(&self) -> impl Iterator<Item = (&ProtocolType, &HashMap<String, Option<ProtocolDynamicInfo>>)> {
		self.0.iter()
	}
}
impl std::ops::Deref for PositionProtocolsDynamicInfo {
	type Target = HashMap<ProtocolType, HashMap<String, Option<ProtocolDynamicInfo>>>;

	fn deref(&self) -> &Self::Target {
		&self.0
	}
}
impl std::ops::DerefMut for PositionProtocolsDynamicInfo {
	fn deref_mut(&mut self) -> &mut Self::Target {
		&mut self.0
	}
}

/// Reapply fills knowledge to orders supplied by [Protocol]s.
///
/// If `Position` has [Protocol]s of different subtypes, we don't care to have them mix, - from the orders produced here (in full size for each `Protocol` subtype) position will choose the closest ones, ignoring the rest.
#[instrument(skip(exchanges_arc))]
fn recalculate_protocol_orders(
	parent_position_asset: &str,
	min_qty_any_ordertype: f64,
	left_to_target_notional: f64,
	side: Side,
	dyn_info: &PositionProtocolsDynamicInfo,
	exchanges_arc: Arc<Exchanges>,
) -> Vec<ConceptualOrder<ProtocolOrderId>> {
	let mut market_orders = Vec::new();
	let mut stop_orders = Vec::new();
	let mut limit_orders = Vec::new();

	//PERF: (n^3), but it's fine, as n is small.
	for (_protocol_type, protocols_map) in dyn_info.iter() {
		let mut in_play_protocols_map = protocols_map.clone();
		let mut accumulated_leftovers = 0.0;

		#[allow(clippy::never_loop)] // clippy being dumb
		loop {
			for (i, (signature, info)) in in_play_protocols_map.clone().iter().enumerate() {
				// clone to avoid using unsafe on `remove`
				let info = match info.as_ref() {
					Some(info) => info,
					None => continue, // Protocol is _yet to_ send orders. We assume it's always intentional.
				};
				let orders = &info.protocol_orders.__orders;
				let size_multiplier = 1.0 / protocols_map.len() as f64; //NB: not in_play_protocols_map.len()
				let protocol_controlled_notional = (left_to_target_notional + accumulated_leftovers) * size_multiplier;

				let qties_payload: Vec<ConceptualOrderPercents> = orders.iter().flatten().cloned().collect();
				let asset_min_trade_qties = Exchanges::compile_min_trade_qties(exchanges_arc.clone(), parent_position_asset, &qties_payload);

				let per_order_infos: Vec<RecalculateOrdersPerOrderInfo> = info
					.fills
					.iter()
					.enumerate()
					.map(|(i, filled)| {
						let min_possible_qty = asset_min_trade_qties[i];
						RecalculateOrdersPerOrderInfo::new(*filled, min_possible_qty)
					})
					.collect();

				let recalculated_allocation = info
					.protocol_orders
					.recalculate_protocol_orders_allocation(&per_order_infos, protocol_controlled_notional, min_qty_any_ordertype);

				match recalculated_allocation.leftovers {
					Some(offset) => {
						match i {
							x if x == in_play_protocols_map.len() - 1 => {
								debug!("Discarding leftovers for {:?}", _protocol_type);
							}
							_ => {
								#[allow(unused_assignments)] // clippy being dumb
								accumulated_leftovers += offset;
								in_play_protocols_map.remove(signature);
								break;
							}
						}
					}
					None => {
						recalculated_allocation.orders.into_iter().for_each(|o| match o.order_type {
							ConceptualOrderType::StopMarket(_) => stop_orders.push(o),
							ConceptualOrderType::Limit(_) => limit_orders.push(o),
							ConceptualOrderType::Market(_) => market_orders.push(o),
						});
					}
				}
			}
			break;
		}
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

#[instrument(skip(protocol_orders_update))]
async fn process_protocol_orders_update(protocol_orders_update: ProtocolOrders, dyn_info: &mut PositionProtocolsDynamicInfo) -> Result<()> {
	debug!(
		"Position received protocol {:?} sending orders: {:?}",
		protocol_orders_update.protocol_id, protocol_orders_update.__orders
	);
	for (_protocol_type, on_type_infos) in dyn_info.iter_mut() {
		if let Some(maybe_protocol_info) = on_type_infos.get_mut(&protocol_orders_update.protocol_id) {
			if let Some(protocol_info) = maybe_protocol_info {
				protocol_info.update_orders(protocol_orders_update.clone());
			} else {
				maybe_protocol_info.replace(ProtocolDynamicInfo::new(protocol_orders_update.clone()));
			}
		}
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
		Self::new(position_id, poid.protocol_signature, poid.ordinal)
	}
}

// pub struct PositionClosed {
// 	_followup: PositionFollowup,
// 	t_closed: DateTime<Utc>,
//}
