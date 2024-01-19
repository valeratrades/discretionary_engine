use crate::positions::Position;
use anyhow::{Error, Result};
use std::collections::HashMap;
use std::fmt;
use std::str::FromStr;
use v_utils::data::compact_format::COMPACT_FORMAT_DELIMITER;
use v_utils::init_compact_format;
use v_utils::trades::Timeframe;

// de impl on this will split upon a delimiter, then have several ways to define the name, which is the first part and translated directly; while the rest is parsed.
//TODO!: move away from a vec of protocols, and embrace specification of their functions.
#[derive(Clone, Debug)]
pub enum Protocol {
	TrailingStop(TrailingStop),
	SAR(SAR),
	TpSl(TpSl),
}
impl FromStr for Protocol {
	type Err = anyhow::Error;

	fn from_str(s: &str) -> Result<Self> {
		let mut parts = s.splitn(2, COMPACT_FORMAT_DELIMITER);
		let name = parts.next().ok_or_else(|| Error::msg("No protocol name"))?;
		let params = parts.next().ok_or_else(|| Error::msg("Missing parameter specifications"))?;
		let protocol: Protocol = match name.to_lowercase().as_str() {
			"trailing" | "trailing_stop" | "ts" => Protocol::TrailingStop(TrailingStop::from_str(params)?),
			"sar" => Protocol::SAR(SAR::from_str(params)?),
			"tpsl" | "take_stop" | "take_profit_stop_loss" => Protocol::TpSl(TpSl::from_str(params)?),
			_ => return Err(Error::msg("Unknown protocol")),
		};
		Ok(protocol)
	}
}
impl fmt::Display for Protocol {
	fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
		match self {
			Protocol::TrailingStop(ts) => ts.fmt(f),
			Protocol::SAR(sar) => sar.fmt(f),
			Protocol::TpSl(tpsl) => tpsl.fmt(f),
		}
	}
}

init_compact_format!(SAR, [(start, f64), (increment, f64), (max, f64), (timeframe, Timeframe)]);

init_compact_format!(TrailingStop, [(percent, f64)]);

init_compact_format!(TpSl, [(tp, f64), (sl, f64)]);

//TODO!!!: Slap a protocol slot on Position
impl TrailingStop {
	pub fn follow(&self, position: &Position) {
		todo!()
	}
}
