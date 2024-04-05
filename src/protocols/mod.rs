mod trailing_stop;
use crate::api::order_types::{ConceptualOrder, ConceptualOrderPercents};
use crate::positions::PositionSpec;
use anyhow::Result;
use std::collections::HashMap;
use std::str::FromStr;
use std::sync::mpsc;
use tracing::error;
pub use trailing_stop::TrailingStopWrapper;
use uuid::Uuid;

/// Used when determining sizing or the changes in it, in accordance to the current distribution of rm on types of algorithms.
/// Size is by default equally distributed amongst the protocols of the same `ProtocolType`, to total 100% for each type with at least one representative.
/// Note that total size is is 100% for both the stop and normal orders (because they are on the different sides of the price).
#[derive(Debug, PartialEq, Eq, Hash)]
pub enum ProtocolType {
	Momentum,
	TP,
	SL,
}

pub trait Protocol {
	type Params;
	fn attach(&self, tx_orders: mpsc::Sender<ProtocolOrders>, position_spec: &crate::positions::PositionSpec) -> anyhow::Result<()>;
	fn update_params(&self, params: &Self::Params) -> anyhow::Result<()>;
	fn get_subtype(&self) -> ProtocolType;
}

/// possibly will implement Iterator on this, because all additional methods seem to want it.
#[derive(v_utils::macros::VecFieldsFromVecStr, Debug)]
pub struct FollowupProtocols {
	pub trailing_stop: Vec<TrailingStopWrapper>,
}
impl FollowupProtocols {
	pub fn count_subtypes(&self) -> HashMap<ProtocolType, usize> {
		let mut different_types: std::collections::HashMap<ProtocolType, usize> = std::collections::HashMap::new();
		for protocol in &self.trailing_stop {
			let subtype = protocol.get_subtype();
			*different_types.entry(subtype).or_insert(0) += 1;
		}
		// ... others
		different_types
	}

	pub fn attach_all(&self, tx_orders: mpsc::Sender<ProtocolOrders>, spec: &PositionSpec) -> anyhow::Result<()> {
		for ts in &self.trailing_stop {
			ts.attach(tx_orders.clone(), spec)?;
		}
		// ... others
		Ok(())
	}
}

#[derive(Debug, Clone)]
pub enum FollowupProtocol {
	TrailingStop(TrailingStopWrapper),
}
impl FollowupProtocol {
	pub fn from_str(spec: &str) -> Result<Self> {
		if let Ok(ts) = TrailingStopWrapper::from_str(spec) {
			Ok(FollowupProtocol::TrailingStop(ts))
		} else {
			Err(anyhow::Error::msg("Could not convert string to any FollowupProtocol"))
		}
	}
}
impl FollowupProtocol {
	pub fn attach(&self, tx_orders: mpsc::Sender<ProtocolOrders>, position_spec: &crate::positions::PositionSpec) -> anyhow::Result<()> {
		match self {
			FollowupProtocol::TrailingStop(ts) => ts.attach(tx_orders, position_spec),
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
	let mut protocols = Vec::new();
	for spec in protocol_specs {
		let protocol = FollowupProtocol::from_str(&spec)?;
		protocols.push(protocol);
	}

	Ok(protocols)
}

/// Wrapper around Orders, which allows for updating the target after a partial fill, without making a new request to the protocol.
///NB: the protocol itself must internally uphold the equality of ids attached to orders to corresponding fields of ProtocolOrders, as well as to ensure that all possible orders the protocol can ether request are initialized in every ProtocolOrders instance it outputs.
#[derive(Debug, Clone)]
pub struct ProtocolOrders {
	pub produced_by: String,
	fields: HashMap<Uuid, Option<ConceptualOrderPercents>>,
}
impl ProtocolOrders {
	pub fn new(produced_by: String, fields: HashMap<Uuid, Option<ConceptualOrderPercents>>) -> Self {
		Self { produced_by, fields }
	}

	pub fn empty_mask(&self) -> HashMap<Uuid, f64> {
		let mut mask = HashMap::new();
		for (key, _value) in self.fields {
			mask.insert(key, 0.0);
		}
		mask
	}

	pub fn apply_mask(&self, filled_mask: HashMap<Uuid, f64>, total_controlled_notional: f64) -> Vec<ConceptualOrder> {
		let mut total_offset = 0.0;
		let mut orders: Vec<ConceptualOrder> = self
			.fields
			.iter()
			.filter_map(|(uuid, order)| {
				if let Some(o) = order.clone() {
					let mut exact_order = o.to_exact(total_controlled_notional, self.produced_by.clone(), uuid.clone());
					let filled = *filled_mask.get(uuid).unwrap_or(&0.0);

					if filled > exact_order.notional() * 0.99 {
						total_offset += filled - exact_order.notional();
						return None;
					}

					exact_order.cut_size(filled);
					Some(exact_order)
				} else {
					None
				}
			})
			.collect();

		orders.sort_by(|a, b| b.notional().partial_cmp(&a.notional()).unwrap_or(std::cmp::Ordering::Equal));
		let mut l = orders.len();
		for i in (0..l).rev() {
			if orders[i].notional() < total_offset / l as f64 {
				orders.remove(i);
				total_offset -= orders[i].notional();
				l -= 1;
			} else {
				// if reached this once, all following elements will also eval to true, so the total_offset is constant now.
				orders[i].cut_size(total_offset);
			}
		}
		if orders.len() == 0 {
			error!("Missed by {total_offset}");
		}

		orders
	}
}

#[derive(Debug, Hash, Clone)]
pub struct ProtocolOrderId {
	produced_by: String,
	uuid: Uuid,
}
