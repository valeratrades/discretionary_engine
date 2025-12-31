//! Protocol module for discretionary_engine_strategy.
//!
//! This is a simplified version focused only on protocol deserialization,
//! without execution logic.

mod dummy_market;

use std::str::FromStr;

use color_eyre::eyre::{Result, bail};
pub use dummy_market::DummyMarket;

/// Protocol types supported by the strategy engine.
#[derive(Clone, Debug)]
pub enum Protocol {
	DummyMarket(DummyMarket),
}

impl FromStr for Protocol {
	type Err = color_eyre::eyre::Report;

	fn from_str(spec: &str) -> Result<Self> {
		if let Ok(dm) = DummyMarket::from_str(spec) {
			Ok(Protocol::DummyMarket(dm))
		} else {
			bail!("Could not convert string to any Protocol\nString: {spec}")
		}
	}
}

impl Protocol {
	/// Get the protocol signature (its string representation).
	pub fn signature(&self) -> String {
		match self {
			Protocol::DummyMarket(dm) => dm.to_string(),
		}
	}
}

/// Parse protocol specs from command line arguments.
pub fn interpret_protocol_specs(protocol_specs: Vec<String>) -> Result<Vec<Protocol>> {
	let protocol_specs: Vec<String> = protocol_specs.into_iter().filter(|s| !s.is_empty()).collect();
	if protocol_specs.is_empty() {
		bail!("No protocols specified");
	}
	let mut protocols = Vec::new();
	for spec in protocol_specs {
		let protocol = Protocol::from_str(&spec)?;
		protocols.push(protocol);
	}
	Ok(protocols)
}
