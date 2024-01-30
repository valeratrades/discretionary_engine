use crate::api::binance;
use crate::api::KlinesSpec;
use crate::api::OrderSpec;
use crate::positions::Position;
use anyhow::{Error, Result};
use arrow2::array::{Float64Array, Int64Array};
use futures_util::StreamExt;
use serde::de::{self, SeqAccess, Visitor};
use serde::{Deserialize, Deserializer};
use serde_json::Value;
use std::collections::HashMap;
use std::fmt;
use std::str::FromStr;
use std::sync::{Arc, Mutex};
use tokio_tungstenite::connect_async;
use v_utils::data::compact_format::COMPACT_FORMAT_DELIMITER;
use v_utils::init_compact_format;
use v_utils::trades::{Side, Timeframe, Timestamp};

// everybody will have owned orders on them too

// de impl on this will split upon a delimiter, then have several ways to define the name, which is the first part and translated directly; while the rest is parsed.
#[derive(Clone, Debug)]
pub struct Protocols {
	pub trailing_stop: Option<TrailingStop>,
	pub sar: Option<SAR>,
	pub tpsl: Option<TpSl>,
	/// close position when another asset crosses certain price
	pub leading_crosses: Option<LeadingCrosses>,
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

// this will be done as part of the macro
pub enum Protocol {
	TrailingStop(TrailingStop),
	SAR(SAR),
	TpSl(TpSl),
	LeadingCrosses(LeadingCrosses),
}
// this not
impl Protocol {
	pub fn required_klines_spec(&self, symbol: String) -> Option<KlinesSpec> {
		match self {
			//? How do you even express this? I need to know the highest/lowest point over the entire period that the position was open.
			// This has to dinamically choose a timeframe such as the moment the position was opened is on a different candle from current.
			// Does this mean we cannot have our own loops, and have to just request data to be supplied on call?
			Protocol::TrailingStop(_) => Some(KlinesSpec::new(symbol, Timeframe::from_str("1m").unwrap(), 1)),
			Protocol::SAR(sar) => Some(KlinesSpec::new(symbol, sar.timeframe.clone(), 500)),
			Protocol::TpSl(_) => None,
			Protocol::LeadingCrosses(lc) => Some(KlinesSpec::new(lc.symbol.clone(), Timeframe::from_str("1m").unwrap(), 1)),
		}
	}
}

pub trait ProtocolAttach {
	async fn attach(&self, owner: Arc<Mutex<Position>>, cache: Arc<ProtocolCache>) -> Result<()>;
}

impl ProtocolAttach for TrailingStop {
	async fn attach(&self, owner: Arc<Mutex<Position>>, cache: Arc<ProtocolCache>) -> Result<()> {
		async fn websocket_listen(symbol: String, side: Side, percent: f64, cache: Arc<ProtocolCache>) {
			let address = format!("wss://fstream.binance.com/ws/{}@markPrice", symbol.to_lowercase());
			let url = url::Url::parse(&address).unwrap();
			let (ws_stream, _) = connect_async(url).await.expect("Failed to connect");
			let (_, read) = ws_stream.split();

			read.for_each(|message| {
				let cache = cache.clone();
				async move {
					let data = message.unwrap().into_data();
					match serde_json::from_slice::<Value>(&data) {
						Ok(json) => {
							if let Some(price_str) = json.get("p") {
								let price: f64 = price_str.as_str().unwrap().parse().unwrap();
								dbg!(&price);
								let mut trailing_stop_local_lock = cache.trailing_stop.local.lock().unwrap();
								if price < trailing_stop_local_lock.bottom {
									trailing_stop_local_lock.bottom = price;
									match side {
										Side::Buy => {}
										Side::Sell => {
											// remove old order request, place new at new p+percent
											todo!()
										}
									}
								}
								if price > trailing_stop_local_lock.top {
									trailing_stop_local_lock.top = price;
									match side {
										Side::Buy => {}
										Side::Sell => {
											// remove old order request, place new at new p+percent
											todo!()
										}
									}
								}
							}
						}
						Err(e) => {
							println!("Failed to parse message as JSON: {}", e);
						}
					}
				}
			})
			.await;
		}
		let percent = self.percent;
		let symbol: String;
		let side: Side;
		{
			let lock = owner.lock().unwrap();
			symbol = lock.symbol.clone();
			side = lock.side.clone();
		}
		tokio::spawn(async move {
			let symbol = symbol.clone();
			loop {
				let handle = websocket_listen(symbol.clone(), side, percent, cache.clone());

				handle.await;
				eprintln!("Restarting Binance websocket for the trailing stop in 30 seconds...");
				tokio::time::sleep(tokio::time::Duration::from_secs(30)).await;
			}
		});
		Ok(())
	}
}

//	// I guess will be called only when the child has timeframe, as we're fucking unable to access it in a generic.
//	pub async fn attach(&self, owner: Arc<Mutex<Position>>, cache: Arc<Mutex<ProtocolCache>>, timeframe: Timeframe) -> Result<()> {
//		let klines_arc = self.klines.clone();
//		let symbol = owner.lock().unwrap().symbol.clone();
//		tokio::spawn(async move {
//			let klines = binance::get_futures_klines(symbol, timeframe, 1000 as usize).await.unwrap();
//			klines_arc.lock().unwrap().insert(timeframe::to_str(), klines).unwrap();
//		});
//		Ok(())
//	}
//}

/// With current implementation, structs that do not store Cache do not have their associated field. All protocols receive Arc<Mutex<ProtocolCache>> in its entirety.
pub struct ProtocolCache {
	pub trailing_stop: CacheBlob<TrailingStopCache>,
	pub sar: CacheBlob<SARCache>,
	pub tpsl: CacheBlob<TpSlCache>,
	pub leading_crosses: CacheBlob<LeadingCrossesCache>,
}
pub struct CacheBlob<T> {
	pub orders: Arc<Mutex<OrderSpec>>,
	pub local: Arc<Mutex<T>>,
}

/// Stores both highest and lowest prices in case the direction is switched for some reason. Note: not meant to.
pub struct TrailingStopCache {
	pub timeframe: Timeframe,
	pub top: f64,
	pub bottom: f64,
}
pub struct LeadingCrossesCache {
	pub init_price: f64,
}
pub struct SARCache {}
pub struct TpSlCache {}

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
