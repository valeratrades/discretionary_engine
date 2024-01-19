use crate::positions::Position;
use anyhow::{Error, Result};
use std::collections::HashMap;
use std::fmt;
use std::str::FromStr;
use v_utils::trades::Timeframe;

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
impl fmt::Display for Protocol {
	fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
		match self {
			Protocol::TrailingStop(ts) => ts.fmt(f),
			Protocol::SAR(sar) => sar.fmt(f),
		}
	}
}

// my_format! macro {{{
macro_rules! my_format {
($name:ident, [ $(($field:ident, $field_type:ty)),* ]) => {
#[derive(Clone, Debug)]
pub struct $name {
$(
$field: $field_type,
)*
}
///NB: Note that FromStr takes string withot $name, while Display prints it with $name
/// Not sure if that's a good idea, but no clue how to fix.
impl FromStr for $name {
	type Err = anyhow::Error;

	fn from_str(s: &str) -> Result<Self> {
		let parts = s.split('-').collect::<Vec<_>>();
		let mut fields = Vec::new();
		$(
		fields.push(stringify!($field));
		)*
		assert_eq!(parts.len(), fields.len(), "Incorrect number of parameters provided");

		let mut provided_params: HashMap<char, &str> = HashMap::new();
		for param in s.split('-') {
			if let Some(first_char) = param.chars().next() {
				let value = &param[1..];
				provided_params.insert(first_char, value);
			}
		}

		Ok($name {
		$(
		$field: {
			let first_char = stringify!($field).chars().next().unwrap();
			let value = provided_params.get(&first_char).unwrap().parse::<$field_type>()?;
			value
		},
		)*
		})
	}
}

impl fmt::Display for $name {
	fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
		let struct_name = stringify!($name).to_lowercase();
		write!(f, "{}", struct_name)?;

		$(
			write!(f, "-{}{}", stringify!($field).chars().next().unwrap(), self.$field)?;
		)*

		Ok(())
	}
}
};}
//,}}}

my_format!(SAR, [(start, f64), (increment, f64), (max, f64), (timeframe, Timeframe)]);

my_format!(TrailingStop, [(percent, f64)]);

my_format!(TpSl, [(tp, f64), (sl, f64)]);

//TODO!!!: Slap a protocol slot on Position
impl TrailingStop {
	pub fn follow(&self, position: &Position) {
		todo!()
	}
}
