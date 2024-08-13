#![allow(unused_variables)] // getting false positives on the test code for some reason
use discretionary_engine_macros::ProtocolWrapper;
use std::str::FromStr;

#[derive(ProtocolWrapper, Debug, Default, Clone, PartialEq)]
pub struct TrailingStop {
	pub percent: f64,
}
impl FromStr for TrailingStop {
	type Err = anyhow::Error;

	fn from_str(spec: &str) -> anyhow::Result<Self> {
		Ok(Self { percent: 42.0 })
	}
}

fn main() {
	{
		let ts_str = "ts:p-0.5";
		let _ts_wrapper = TrailingStopWrapper::from_str(ts_str).unwrap();
		let _ts = std::cell::RefCell::new(TrailingStop { percent: 42.0 });
		assert_eq!(_ts_wrapper.0, _ts);
	}
}
