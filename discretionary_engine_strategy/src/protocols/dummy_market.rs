//! DummyMarket protocol - sends a single market order.

use std::{fmt, str::FromStr};

use color_eyre::eyre::{Result, bail};

/// A protocol that simply sends one market order.
///
/// This is the simplest possible protocol, used for testing and debugging.
#[derive(Clone, Debug, Default)]
pub struct DummyMarket;

impl DummyMarket {
	/// Protocol prefix used for parsing.
	pub const PREFIX: &'static str = "dm";
}

impl fmt::Display for DummyMarket {
	fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
		write!(f, "{}", Self::PREFIX)
	}
}

impl FromStr for DummyMarket {
	type Err = color_eyre::eyre::Report;

	fn from_str(spec: &str) -> Result<Self> {
		// Accept "dm" or "dm:" with no additional params
		let trimmed = spec.trim();
		if trimmed == Self::PREFIX || trimmed.starts_with(&format!("{}:", Self::PREFIX)) {
			// For now, DummyMarket has no parameters
			let after_prefix = trimmed.strip_prefix(Self::PREFIX).unwrap_or("");
			let after_colon = after_prefix.strip_prefix(':').unwrap_or(after_prefix);
			if after_colon.is_empty() {
				return Ok(DummyMarket);
			}
			bail!("DummyMarket does not accept parameters, got: {after_colon}");
		}
		bail!("Expected protocol spec starting with '{}', got: {spec}", Self::PREFIX)
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn parse_dm() {
		let dm = DummyMarket::from_str("dm").unwrap();
		assert_eq!(dm.to_string(), "dm");
	}

	#[test]
	fn parse_dm_with_colon() {
		let dm = DummyMarket::from_str("dm:").unwrap();
		assert_eq!(dm.to_string(), "dm");
	}

	#[test]
	fn parse_dm_with_params_fails() {
		let result = DummyMarket::from_str("dm:p0.5");
		assert!(result.is_err());
	}

	#[test]
	fn parse_wrong_prefix_fails() {
		let result = DummyMarket::from_str("ts:p0.5");
		assert!(result.is_err());
	}
}
