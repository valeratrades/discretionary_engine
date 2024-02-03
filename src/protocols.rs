use crate::api::binance::{self, futures_price};
use crate::api::round_to_required_precision;
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
use std::sync::atomic::Ordering;
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
impl Protocols {
	pub async fn attach(&self, owner: &Position) -> Result<()> {
		if let Some(trailing_stop) = &self.trailing_stop {
			trailing_stop.attach(owner).await?;
		}
		//if let Some(sar) = &self.sar {
		//	sar.attach(owner).await?;
		//}
		//if let Some(tpsl) = &self.tpsl {
		//	tpsl.attach(owner).await?;
		//}
		//if let Some(leading_crosses) = &self.leading_crosses {
		//	leading_crosses.attach(owner).await?;
		//}
		Ok(())
	}
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

pub trait ProtocolAttach {
	async fn attach(&self, owner: &Position) -> Result<()>;
}

impl ProtocolAttach for TrailingStop {
	async fn attach(&self, owner: &Position) -> Result<()> {
		async fn websocket_listen(
			symbol: String,
			side: Side,
			percent: f64,
			cache: Arc<Mutex<TrailingStopCache>>,
			orders_sink: Arc<Mutex<Vec<InnerOrder>>>,
		) {
			let address = format!("wss://fstream.binance.com/ws/{}@markPrice", symbol.to_lowercase());
			let url = url::Url::parse(&address).unwrap();
			let (ws_stream, _) = connect_async(url).await.expect("Failed to connect");
			let (_, read) = ws_stream.split();

			read.for_each(|message| {
				let cache = cache.clone();
				let orders_sink = orders_sink.clone();
				async move {
					let data = message.unwrap().into_data();
					match serde_json::from_slice::<Value>(&data) {
						Ok(json) => {
							if let Some(price_str) = json.get("p") {
								let price: f64 = price_str.as_str().unwrap().parse().unwrap();
								dbg!(&price);
								let mut trailing_stop_guard = cache.lock().unwrap();
								if price < trailing_stop_guard.bottom {
									trailing_stop_guard.bottom = price;
									match side {
										Side::Buy => {}
										Side::Sell => {
											let target_price = price + price * percent;
											let mut orders_guard = orders_sink.lock().unwrap();
											*orders_guard = vec![InnerOrder {
												side: Side::Buy,
												price: target_price,
											}];
										}
									}
								}
								if price > trailing_stop_guard.top {
									trailing_stop_guard.top = price;
									match side {
										Side::Buy => {
											let target_price = price - price * percent;
											let mut orders_guard = orders_sink.lock().unwrap();
											*orders_guard = vec![InnerOrder {
												side: Side::Buy,
												price: target_price,
											}];
										}
										Side::Sell => {}
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
		let symbol = owner.symbol.clone();
		let side = owner.side;

		let mut cache_guard = owner.cache.lock().unwrap();
		let trailing_internal = &mut cache_guard.trailing_stop.internal;
		if trailing_internal.is_none() {
			let price = futures_price(symbol.clone()).await?;
			*trailing_internal = Some(Arc::new(Mutex::new(TrailingStopCache { top: price, bottom: price })));
		}
		let trailing_cache = trailing_internal.as_ref().map(|arc| Arc::clone(arc)).unwrap();

		let trailing_orders = cache_guard.trailing_stop.orders.clone();
		drop(cache_guard);

		tokio::spawn(async move {
			let symbol = symbol.clone();
			loop {
				let handle = websocket_listen(symbol.clone(), side, percent, trailing_cache.clone(), trailing_orders.clone());

				handle.await;
				eprintln!("Restarting Binance websocket for the trailing stop in 30 seconds...");
				tokio::time::sleep(tokio::time::Duration::from_secs(30)).await;
			}
		});
		Ok(())
	}
}

/// With current implementation, structs that do not store Cache do not have their associated field. All protocols receive Arc<Mutex<ProtocolCache>> in its entirety.
#[derive(Debug)]
pub struct Cache {
	pub trailing_stop: CacheBlob<TrailingStopCache>,
	pub sar: CacheBlob<SARCache>,
	pub tpsl: CacheBlob<TpSlCache>,
	pub leading_crosses: CacheBlob<LeadingCrossesCache>,
}
impl Cache {
	pub fn new() -> Self {
		Self {
			trailing_stop: CacheBlob::new(),
			sar: CacheBlob::new(),
			tpsl: CacheBlob::new(),
			leading_crosses: CacheBlob::new(),
		}
	}
}
#[derive(Debug)]
pub struct CacheBlob<T> {
	pub orders: Arc<Mutex<Vec<InnerOrder>>>,
	pub internal: Option<Arc<Mutex<T>>>,
}
impl<T> CacheBlob<T> {
	pub fn new() -> Self {
		Self {
			orders: Arc::new(Mutex::new(Vec::new())),
			internal: None,
		}
	}
}

/// Stores both highest and lowest prices in case the direction is switched for some reason. Note: not meant to.
#[derive(Debug)]
pub struct TrailingStopCache {
	pub top: f64,
	pub bottom: f64,
}
#[derive(Debug)]
pub struct LeadingCrossesCache {
	pub init_price: f64,
}
#[derive(Debug)]
pub struct SARCache {}
#[derive(Debug)]
pub struct TpSlCache {}

#[derive(Debug)]
pub struct InnerOrder {
	pub side: Side,
	pub price: f64,
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
