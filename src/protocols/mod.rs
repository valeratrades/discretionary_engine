mod trailing_stop;
use crate::api::order_types::OrderP;
use crate::positions::PositionSpec;
use anyhow::Result;
use std::collections::HashMap;
use std::str::FromStr;
use std::sync::mpsc;
pub use trailing_stop::TrailingStopWrapper;

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
	fn attach(&self, tx_orders: mpsc::Sender<(Vec<OrderP>, String)>, position_spec: &crate::positions::PositionSpec) -> anyhow::Result<()>;
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

	pub fn attach_all(&self, tx_orders: mpsc::Sender<(Vec<OrderP>, String)>, spec: &PositionSpec) -> anyhow::Result<()> {
		for ts in &self.trailing_stop {
			ts.attach(tx_orders.clone(), spec)?;
		}
		// ... others
		Ok(())
	}
}

#[derive(Debug)]
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
	pub fn attach(&self, tx_orders: mpsc::Sender<(Vec<OrderP>, String)>, position_spec: &crate::positions::PositionSpec) -> anyhow::Result<()> {
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
