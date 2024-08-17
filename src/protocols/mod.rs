mod sar;
mod trailing_stop;
use std::{collections::HashSet, str::FromStr};

use anyhow::Result;
use tokio::{sync::mpsc, task::JoinSet};
pub use trailing_stop::TrailingStopWrapper;
use uuid::Uuid;

use crate::exchange_apis::order_types::{ConceptualOrder, ConceptualOrderPercents, ProtocolOrderId};

/// Used when determining sizing or the changes in it, in accordance to the current distribution of rm on types of algorithms.
/// Size is by default equally distributed amongst the protocols of the same `ProtocolType`, to total 100% for each type with at least one representative.
/// Note that total size is is 100% for both the stop and normal orders (because they are on the different sides of the price).
#[derive(Debug, PartialEq, Eq, Hash, Clone, Copy, derive_new::new)]
pub enum ProtocolType {
	Momentum,
	TP,
	SL,
}

pub trait Protocol {
	type Params;
	/// Requested orders are being sent over the mspc with uuid of the protocol on each batch, as we want to replace the previous requested batch if any.
	fn attach(&self, set: &mut JoinSet<Result<()>>, tx_orders: mpsc::Sender<ProtocolOrders>, position_spec: &crate::positions::PositionSpec) -> Result<()>;
	fn update_params(&self, params: &Self::Params) -> anyhow::Result<()>;
	fn get_subtype(&self) -> ProtocolType;
}

// TODO!!: revisit
/// possibly will implement Iterator on this, because all additional methods seem to want it.
//#[derive(v_utils::macros::VecFieldsFromVecStr, Debug, Clone, Default, serde::Serialize, serde::Deserialize, derive_new::new)]
// pub struct FollowupProtocols {
// 	pub trailing_stop: Vec<TrailingStopWrapper>,
//}
// impl FollowupProtocols {
// 	pub fn count_subtypes(&self) -> HashMap<ProtocolType, usize> {
// 		let mut different_types: std::collections::HashMap<ProtocolType, usize> = std::collections::HashMap::new();
// 		for protocol in &self.trailing_stop {
// 			let subtype = protocol.get_subtype();
// 			*different_types.entry(subtype).or_insert(0) += 1;
// 		}
// 		// ... others
// 		different_types
// 	}
//
// 	pub fn attach_all(&self, tx_orders: mpsc::Sender<ProtocolOrders>, spec: &PositionSpec) -> anyhow::Result<()> {
// 		for ts in &self.trailing_stop {
// 			ts.attach(tx_orders.clone(), spec)?;
// 		}
// 		// ... others
// 		Ok(())
// 	}
//}

#[derive(Debug, Clone)]
pub enum FollowupProtocol {
	TrailingStop(TrailingStopWrapper),
}
impl FromStr for FollowupProtocol {
	type Err = anyhow::Error;

	fn from_str(spec: &str) -> Result<Self> {
		if let Ok(ts) = TrailingStopWrapper::from_str(spec) {
			Ok(FollowupProtocol::TrailingStop(ts))
		} else {
			Err(anyhow::Error::msg("Could not convert string to any FollowupProtocol"))
		}
	}
}
impl FollowupProtocol {
	pub fn attach(&self, position_set: &mut JoinSet<Result<()>>, tx_orders: mpsc::Sender<ProtocolOrders>, position_spec: &crate::positions::PositionSpec) -> anyhow::Result<()> {
		match self {
			FollowupProtocol::TrailingStop(ts) => ts.attach(position_set, tx_orders, position_spec),
		}
	}

	pub fn update_params(&self, params: &<TrailingStopWrapper as Protocol>::Params) -> anyhow::Result<()> {
		match self {
			FollowupProtocol::TrailingStop(ts) => ts.update_params(params),
		}
	}

	pub fn get_subtype(&self) -> ProtocolType {
		match self {
			FollowupProtocol::TrailingStop(ts) => ts.get_subtype(),
		}
	}
}

pub fn interpret_followup_specs(protocol_specs: Vec<String>) -> Result<Vec<FollowupProtocol>> {
	assert_eq!(protocol_specs.len(), protocol_specs.iter().collect::<HashSet<&String>>().len()); // protocol specs are later used as their IDs
	let mut protocols = Vec::new();
	for spec in protocol_specs {
		let protocol = FollowupProtocol::from_str(&spec)?;
		protocols.push(protocol);
	}

	Ok(protocols)
}

#[derive(Clone, Debug, Default, derive_new::new)]
pub struct ProtocolFill {
	pub key: Uuid,
	pub id: ProtocolOrderId,
	pub qty: f64,
}

#[derive(Clone, Debug, Default)]
pub struct ProtocolDynamicInfo {
	fills: Vec<f64>,
	protocol_orders: ProtocolOrders,
}
impl ProtocolDynamicInfo {
	pub fn new(protocol_orders: ProtocolOrders) -> Self {
		let fills = protocol_orders.empty_mask();
		Self { fills, protocol_orders }
	}

	pub fn update_fills(&mut self, fills: Vec<f64>) {
		self.fills = fills;
	}

	pub fn update_fill_at(&mut self, i: usize, fill: f64) {
		self.fills[i] += fill;
	}

	pub fn update_orders(&mut self, orders: ProtocolOrders) {
		self.protocol_orders = orders;
	}

	pub fn conceptual_orders(&self, parent_matching_subtype_n: usize, parent_notional: f64) -> Vec<ConceptualOrder<ProtocolOrderId>> {
		let size_multiplier = 1.0 / parent_matching_subtype_n as f64;
		let total_controlled_size = parent_notional * size_multiplier;

		self.protocol_orders.apply_mask(&self.fills, total_controlled_size)
	}
}

/// Wrapper around Orders, which allows for updating the target after a partial fill, without making a new request to the protocol.
/// NB: the protocol itself must internally uphold the equality of ids attached to orders to corresponding fields of ProtocolOrders, as well as to ensure that all possible orders the protocol can ether request are initialized in every ProtocolOrders instance it outputs.
#[derive(Debug, Clone, derive_new::new, Default)]
pub struct ProtocolOrders {
	pub protocol_id: String,
	pub __orders: Vec<Option<ConceptualOrderPercents>>, // pub for testing purposes
}
impl ProtocolOrders {
	pub fn empty_mask(&self) -> Vec<f64> {
		vec![0.; self.__orders.len()]
	}

	pub fn apply_mask(&self, filled_mask: &[f64], total_controlled_notional: f64) -> Vec<ConceptualOrder<ProtocolOrderId>> {
		let mut total_offset = 0.0;

		// subtract filled
		let mut orders: Vec<ConceptualOrder<ProtocolOrderId>> = self
			.__orders
			.iter()
			.enumerate()
			.filter_map(|(i, order)| {
				if let Some(o) = order.clone() {
					let mut exact_order = o.to_exact(total_controlled_notional, ProtocolOrderId::new(self.protocol_id.clone(), i));
					let filled = *filled_mask.get(i).unwrap_or(&0.0);

					if filled > exact_order.qty_notional * 0.99 {
						total_offset += filled - exact_order.qty_notional;
						return None;
					}

					exact_order.qty_notional -= filled;
					Some(exact_order)
				} else {
					None
				}
			})
			.collect();

		// redistribute the total size
		orders.sort_by(|a, b| b.qty_notional.partial_cmp(&a.qty_notional).unwrap_or(std::cmp::Ordering::Equal));
		let mut l = orders.len();
		let individual_offset = total_offset / l as f64;
		for i in (0..l).rev() {
			if orders[i].qty_notional < individual_offset {
				orders.remove(i);
				total_offset -= orders[i].qty_notional;
				l -= 1;
			} else {
				// if reached this once, all following elements will also eval to true, so the total_offset is constant now.
				orders[i].qty_notional -= individual_offset;
			}
		}
		if orders.len() == 0 && total_offset != 0.0 {
			tracing::warn!("Missed by {total_offset}");
		}

		orders
	}
}

#[cfg(test)]
mod tests {
	use v_utils::trades::Side;

	use super::*;
	use crate::exchange_apis::{
		order_types::{ConceptualMarket, ConceptualOrderType},
		Market, Symbol,
	};

	#[test]
	fn test_apply_mask() {
		let orders = ProtocolOrders::new(
			"test".to_string(),
			vec![Some(ConceptualOrderPercents::new(
				ConceptualOrderType::Market(ConceptualMarket::new(0.0)),
				Symbol::new("BTC".to_string(), "USDT".to_string(), Market::BinanceFutures),
				Side::Buy,
				0.5,
			))],
		);

		let filled_mask = vec![0.1];
		let total_controlled_notional = 1.0;
		let got = orders.apply_mask(&filled_mask, total_controlled_notional);
		assert_eq!(got.len(), 1);
		assert_eq!(got[0].qty_notional, 0.4);
	}
}
