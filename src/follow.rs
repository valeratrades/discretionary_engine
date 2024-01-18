use crate::positions::Position;
use anyhow::{Error, Result};
use std::collections::HashMap;
use std::str::FromStr;

// de impl on this will split upon a delimiter, then have several ways to define the name, which is the first part and translated directly; while the rest is parsed.
#[derive(Clone, Debug)]
pub enum Protocol {
	TrailingStop(TrailingStop),
	SAR(SAR),
}
impl FromStr for Protocol {
	type Err = anyhow::Error;

	fn from_str(s: &str) -> Result<Self> {
		let mut parts = s.splitn(2, '-');
		let name = parts.next().ok_or_else(|| Error::msg("No protocol name"))?;
		let params = parts.next().ok_or_else(|| Error::msg("Missing parameter specifications"))?;
		let protocol: Protocol = match name.to_lowercase().as_str() {
			"trailing" | "trailing_stop" | "ts" => Protocol::TrailingStop(TrailingStop::from_str(params)?),
			"sar" => Protocol::SAR(SAR::from_str(params)?),
			_ => return Err(Error::msg("Unknown protocol")),
		};
		Ok(protocol)
	}
}

#[derive(Clone, Debug)]
pub struct TrailingStop {
	pub percent: f32,
}
impl FromStr for TrailingStop {
	type Err = anyhow::Error;

	fn from_str(s: &str) -> Result<Self> {
		let params: Vec<&str> = s.split('-').collect();
		assert_eq!(params.len(), 1);

		let (first_char, rest) = params[0].split_at(1);
		match first_char {
			"p" => {
				let percent = rest.parse::<f32>()?;
				Ok(TrailingStop { percent })
			}
			_ => Err(Error::msg("Unknown trailing stop parameter")),
		}
	}
}

// I would want to have one centralized place for storing references of all the known positions.
// But then I also want to prevent having multiples of follow strategies for one position.
// So who the fuck should own it??

// what if position has one slot for a protocol on it? Then have a centralized container structure for all positions, taking ownership of them, and then starting threads for each, which would do the following, if any protocol is specified.

impl TrailingStop {
	pub fn follow(&self, position: &Position) {
		todo!()
	}
}

#[derive(Clone, Debug)]
pub struct SAR {
	//TODO!!: add tf: Timeframe,
	pub start: f32,
	pub increment: f32,
	pub max: f32,
}

// could make with clap's subcommands, but then would need to implement serialization into string anyways, just a cli-command-like string in that case.
// So the custom format it is.
// would need to make a derive macro, that would encode every param by its first letter.
// for reference: clap's #[arg(short)] and serde docs here https://serde.rs/
impl FromStr for SAR {
	type Err = anyhow::Error;

	fn from_str(s: &str) -> Result<Self> {
		let mut params: HashMap<&str, f32> = HashMap::new();

		for param in s.split('-') {
			let (name, value) = param.split_at(1);
			if let Ok(val) = value.parse::<f32>() {
				params.insert(name, val);
			} else {
				return Err(Error::msg("Invalid parameter value"));
			}
		}

		if let (Some(&start), Some(&increment), Some(&max)) = (params.get("s"), params.get("i"), params.get("m")) {
			Ok(SAR { start, increment, max })
		} else {
			Err(Error::msg("Missing SAR parameter(s)"))
		}
	}
}
