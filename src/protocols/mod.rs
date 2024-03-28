mod trailing_stop;
use crate::api::order_types::OrderP;
use anyhow::Result;
use std::str::FromStr;
use std::sync::mpsc;
pub use trailing_stop::TrailingStopWrapper;

/// Used when determining sizing or the changes in it, in accordance to the current distribution of rm on types of algorithms.
pub enum ProtocolType {
	Momentum,
	TP,
	SL,
}

pub trait RevisedProtocol {
	type Params;
	fn attach(&self, tx_orders: mpsc::Sender<(Vec<OrderP>, String)>, position_spec: &crate::positions::PositionSpec) -> anyhow::Result<()>;
	fn update_params(&self, params: &Self::Params) -> anyhow::Result<()>;
	fn get_subtype(&self) -> ProtocolType;
}

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

	pub fn update_params(&self, params: &<TrailingStopWrapper as RevisedProtocol>::Params) -> anyhow::Result<()> {
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
