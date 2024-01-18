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

#[derive(Clone, Debug)]
pub struct SAR {
	pub start: f32,
	pub increment: f32,
	pub max: f32,
}

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
