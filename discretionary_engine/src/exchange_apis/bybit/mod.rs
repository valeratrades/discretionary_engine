use serde::{Deserialize, Serialize};

/// Dummy to concept-prove design for now
#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, Hash, PartialEq, Serialize)]
pub enum BybitMarket {
	#[default]
	Futures,
	Spot,
}
