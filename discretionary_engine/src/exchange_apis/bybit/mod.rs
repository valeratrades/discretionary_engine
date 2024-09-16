use serde::{Deserialize, Serialize};

/// Dummy to concept-prove design for now
#[derive(Clone, Debug, PartialEq, Eq, Hash, Default, Serialize, Deserialize, Copy)]
pub enum BybitMarket {
	#[default]
	Futures,
	Spot,
}
