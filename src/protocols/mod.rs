mod trailing_stop;
use anyhow::Result;
use std::sync::mpsc;
use std::str::FromStr;
use crate::api::order_types::OrderP;
pub use trailing_stop::TrailingStop;

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
	TrailingStop(TrailingStop),
}
impl FollowupProtocol {
	pub fn from_str(spec: &str) -> Result<Self> {
		if let Ok(ts) = TrailingStop::from_str(spec) {
			Ok(FollowupProtocol::TrailingStop(ts))
		} else {
			Err(anyhow::Error::msg("Could not convert string to any FollowupProtocol"))
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

