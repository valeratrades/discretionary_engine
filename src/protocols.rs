use crate::api::binance;
use crate::api::Order;
use crate::positions::Position;
use anyhow::{Error, Result};
use arrow2::array::{Float64Array, Int64Array};
use serde::de::{self, SeqAccess, Visitor};
use serde::{Deserialize, Deserializer};
use std::collections::HashMap;
use std::fmt;
use std::str::FromStr;
use std::sync::{Arc, Mutex};
use v_utils::data::compact_format::COMPACT_FORMAT_DELIMITER;
use v_utils::init_compact_format;
use v_utils::trades::{Timeframe, Timestamp};

// everybody will have owned orders on them too

// de impl on this will split upon a delimiter, then have several ways to define the name, which is the first part and translated directly; while the rest is parsed.
#[derive(Clone, Debug)]
pub struct Protocols {
	pub trailing_stop: Option<ProtocolWrapper<TrailingStop>>,
	pub sar: Option<ProtocolWrapper<SAR>>,
	pub tpsl: Option<ProtocolWrapper<TpSl>>,
	/// close position when another asset crosses certain price
	pub leading_crosses: Option<ProtocolWrapper<LeadingCrosses>>,
}
// want to just go through the supplied Vec<String>, and try until it fits.
//impl FromStr for Protocol {
//	type Err = anyhow::Error;
//
//	fn from_str(s: &str) -> Result<Self> {
//		let mut parts = s.splitn(2, COMPACT_FORMAT_DELIMITER);
//		let name = parts.next().ok_or_else(|| Error::msg("No protocol name"))?;
//		let params = parts.next().ok_or_else(|| Error::msg("Missing parameter specifications"))?;
//		let protocol: Protocol = match name.to_lowercase().as_str() {
//			"trailing" | "trailing_stop" | "ts" => Protocol::TrailingStop(TrailingStop::from_str(params)?),
//			"sar" => Protocol::SAR(SAR::from_str(params)?),
//			"tpsl" | "take_stop" | "sltp" | "take_profit_stop_loss" => Protocol::TpSl(TpSl::from_str(params)?),
//			"leading_crosses" | "lc" => Protocol::LeadingCrosses(LeadingCrosses::from_str(params)?),
//			_ => return Err(Error::msg("Unknown protocol")),
//		};
//		Ok(protocol)
//	}
//}
//impl fmt::Display for Protocol {
//	fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
//		match self {
//			Protocol::TrailingStop(ts) => ts.fmt(f),
//			Protocol::SAR(sar) => sar.fmt(f),
//			Protocol::TpSl(tpsl) => tpsl.fmt(f),
//			Protocol::LeadingCrosses(lc) => lc.fmt(f),
//		}
//	}
//}
//fn deserialize_from_vec<'de, D, T>(deserializer: D) -> Result<Option<T>, D::Error>
//where
//	D: Deserializer<'de>,
//	T: FromStr,
//	T::Err: std::fmt::Display,
//{
//	let vec: Vec<String> = Vec::deserialize(deserializer)?;
//	vec.iter()
//		.find_map(|s| s.parse().ok())
//		.ok_or_else(|| serde::de::Error::custom("Deserialization failed for all elements"))
//}
//
//impl<'de> Deserialize<'de> for Protocols {
//	fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
//	where
//		D: Deserializer<'de>,
//	{
//		#[derive(Deserialize)]
//		struct Helper {
//			#[serde(deserialize_with = "deserialize_from_vec")]
//			trailing_stop: Option<TrailingStop>,
//			#[serde(deserialize_with = "deserialize_from_vec")]
//			sar: Option<SAR>,
//			#[serde(deserialize_with = "deserialize_from_vec")]
//			tpsl: Option<TpSl>,
//			#[serde(deserialize_with = "deserialize_from_vec")]
//			leading_crosses: Option<LeadingCrosses>,
//		}
//
//		let helper = Helper::deserialize(deserializer)?;
//		Ok(Protocols {
//			trailing_stop: helper.trailing_stop.clone().map(|protocol| ProtocolWrapper::new(protocol)),
//			sar: helper.sar.clone().map(|protocol| ProtocolWrapper::new(protocol)),
//			tpsl: helper.tpsl.clone().map(|protocol| ProtocolWrapper::new(protocol)),
//			leading_crosses: helper.leading_crosses.clone().map(|protocol| ProtocolWrapper::new(protocol)),
//		})
//	}
//}

init_compact_format!(SAR, [(start, f64), (increment, f64), (max, f64), (timeframe, Timeframe)]);
init_compact_format!(TrailingStop, [(percent, f64)]);
init_compact_format!(TpSl, [(tp, f64), (sl, f64)]);
init_compact_format!(LeadingCrosses, [(symbol, String), (price, f64)]);

//impl TrailingStop {
//	pub fn attach(&self, position: Arc<Mutex<Position>>) {
//		//- klines loop
//
//		//- orders based on klines
//
//		todo!()
//	}
//}

#[derive(Clone, Debug)]
pub struct ProtocolWrapper<T> {
	pub protocol: T,
	/// Vec of IDs, as we are never checking what's here from the inside. Currently assuming IDs are all numeric, but that should not be relied upon, as it may be a subject to change.
	pub orders: Arc<Mutex<Vec<usize>>>,
	pub klines: Arc<Mutex<HashMap<Timestamp, Klines>>>,
	pub requesting_orders: Arc<Mutex<Vec<Order>>>,
}
impl<T> ProtocolWrapper<T> {
	pub fn new(protocol: T) -> Self {
		Self {
			protocol,
			orders: Arc::new(Mutex::new(Vec::new())),
			klines: Arc::new(Mutex::new(HashMap::new())),
			requesting_orders: Arc::new(Mutex::new(Vec::new())),
		}
	}

	// I guess will be called only when the child has timeframe, as we're fucking unable to access it in a generic.
	pub async fn init(&self, owner: Arc<Mutex<Position>>, timeframe: Timeframe) -> Result<()> {
		let klines_arc = self.klines.clone();
		let symbol = owner.lock().unwrap().symbol.clone();
		tokio::spawn(async move {
			let klines = binance::get_futures_klines(symbol, timeframe, 1000 as usize).await.unwrap();
			klines_arc.lock().unwrap().insert(timeframe::to_str(), klines).unwrap();
		});
		Ok(())
	}
}

/// For components that are not included into standard definition of a kline, (and thus are behind `Option`), requesting of these fields should be adressed to the producer.
//TODO!!: move to v_utils after it's functional enough \
#[derive(Clone, Debug, Default)]
pub struct Klines {
	pub t_open: Int64Array,
	pub open: Float64Array,
	pub high: Float64Array,
	pub low: Float64Array,
	pub close: Float64Array,
	pub volume: Option<Float64Array>,
	//... other optional
}
