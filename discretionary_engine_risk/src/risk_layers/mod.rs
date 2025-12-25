use v_utils::{Percent, percent::PercentU};

use crate::{Quality, RiskTier};

mod from_phone;
mod lost_last_trade;
mod stop_loss_proximity;
pub use from_phone::FromPhone;
pub use lost_last_trade::LostLastTrade;
pub use stop_loss_proximity::StopLossProximity;

pub struct RiskLayerResult {
	pub adjustment: Percent,
	pub certainty: PercentU,
}

pub enum RiskLayer {
	StopLossProximity(StopLossProximity),
	FromPhone(FromPhone),
	LostLastTrade(LostLastTrade),
}

impl RiskLayer {
	pub fn evaluate(&self) -> RiskLayerResult {
		match self {
			RiskLayer::StopLossProximity(layer) => layer.evaluate(),
			RiskLayer::FromPhone(layer) => layer.evaluate(),
			RiskLayer::LostLastTrade(layer) => layer.evaluate(),
		}
	}
}

pub fn apply_risk_layers(suggested_quality: Quality, layers: Vec<RiskLayer>) -> RiskTier {
	let mut total_adjustment = Percent(0.0);

	for layer in layers {
		let result = layer.evaluate();
		total_adjustment = Percent(*total_adjustment + *result.adjustment * *result.certainty);
	}

	let base_tier: RiskTier = suggested_quality.into();
	base_tier + total_adjustment
}
