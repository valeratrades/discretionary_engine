use crate::api::binance::{self, futures_price};
use crate::api::round_to_required_precision;
use crate::api::KlinesSpec;
use crate::api::OrderSpec;
use crate::positions::{Position, PositionCore, PositionFollowup};
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
use v_utils::macros::{CompactFormat, FromVecStr};
use v_utils::trades::{Side, Timeframe, Timestamp};

// everybody will have owned orders on them too

#[derive(Clone, Debug, FromVecString)]
pub struct Protocols {
	pub trailing_stop: Option<TrailingStop>,
	pub sar: Option<SAR>,
	pub tpsl: Option<TPSL>,
	/// close position when another asset crosses certain price
	pub leading_crosses: Option<LeadingCrosses>,
}
impl Protocols {
	//TODO!!!: \
	pub async fn attach(&self, owner: &Position) -> Result<()> {
		//if let Some(trailing_stop) = &self.trailing_stop {
		//	trailing_stop.attach(owner).await?;
		//}
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

#[derive(Debug, CompactFormat)]
pub struct TrailingStop {
	pub percent: f64,
}

#[derive(Debug, CompactFormat)]
pub struct SAR {
	pub start: f64,
	pub increment: f64,
	pub max: f64,
	pub timeframe: Timeframe,
}

#[derive(Debug, CompactFormat)]
pub struct TakeProfitStopLoss {
	pub tp: f64,
	pub sl: f64,
}

#[derive(Debug, CompactFormat)]
pub struct LeadingCrosses {
	pub symbol: String,
	pub price: f64,
}

/// Writes directly to the unprotected fields of CacheBlob, using unsafe
pub trait ProtocolAttach {
	async fn attach<T>(&self, cache_blob: &CacheBlob, position_core: &PositionCore) -> Result<()>;
}

impl ProtocolAttach for TrailingStop {
	async fn attach(&self, cache_blob: &CacheBlob<TrailingStopCache>, position_core: &PositionCore) -> Result<()> {
		async fn websocket_listen(symbol: String, side: Side, percent: f64, cache_blob: &CacheBlob<TrailingStopCache>) {
			let address = format!("wss://fstream.binance.com/ws/{}@markPrice", symbol.to_lowercase());
			let url = url::Url::parse(&address).unwrap();
			let (ws_stream, _) = connect_async(url).await.expect("Failed to connect");
			let (_, read) = ws_stream.split();

			read.for_each(|message| {
				let cache_blob = cache_blob.clone();
				async move {
					let data = message.unwrap().into_data();
					match serde_json::from_slice::<Value>(&data) {
						Ok(json) => {
							if let Some(price_str) = json.get("p") {
								let price: f64 = price_str.as_str().unwrap().parse().unwrap();
								dbg!(&price);
								if price < internal.bottom {
									internal.bottom = price;
									match side {
										Side::Buy => {}
										Side::Sell => {
											let target_price = price + price * percent;
											let orders_raw_pointer = &mut cache_blob.orders as *mut Vec<InternalOrder>;
											*orders = vec![InternalOrder {
												side: Side::Buy,
												price: target_price,
											}];
										}
									}
								}
								if price > internal.top {
									internal.top = price;
									match side {
										Side::Buy => {
											let target_price = price - price * percent;
											let orders_raw_pointer = &mut cache_blob.orders as *mut Vec<InternalOrder>;
											*orders = vec![InternalOrder {
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
		let symbol = position_core.symbol.clone();
		let side = position_core.side;

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
				let handle = websocket_listen(symbol.clone(), side, percent, trailing_cache.clone() /*trailing_orders.clone()*/);

				handle.await;
				eprintln!("Restarting Binance websocket for the trailing stop in 30 seconds...");
				tokio::time::sleep(tokio::time::Duration::from_secs(30)).await;
			}
		});
		Ok(())
	}
}

/// With current implementation, structs that do not store Cache do not have their associated field. All protocols receive Arc<Mutex<ProtocolCache>> in its entirety.
#[derive(Debug, Default)]
pub struct FollowupCache {
	pub trailing_stop: Option<CacheBlob<TrailingStopCache>>,
	pub sar: Option<CacheBlob<SARCache>>,
	pub tpsl: Option<CacheBlob<TpSlCache>>,
	pub leading_crosses: Option<CacheBlob<LeadingCrossesCache>>,
}
#[derive(Debug)]
pub struct CacheBlob<T> {
	// written internally; read from outside
	pub orders: Vec<InternalOrder>,
	// written internally
	pub internal: T,
}
impl<T> CacheBlob<T> {
	pub fn read_orders(&self) -> Vec<InternalOrder> {
		self.orders.clone()
	}

	/// passes the ref to self of CacheBlob down to the owned protocol, which in turn will start writing directly to the fields of CacheBlob, using unsafe
	pub fn attach(&self) -> Result<()> {
		self.internal.attach(self)
	}
}

/// Stores both highest and lowest prices in case the direction is switched for some reason. Note: it's not meant to though.
#[derive(Debug)]
pub struct TrailingStopCache {
	pub top: f64,
	pub bottom: f64,
}
impl Cache for TrailingStopCache {
	async fn init() -> Result<Self> {
		let price = futures_price(symbol.clone()).await?;
		Ok(Self { top: price, bottom: price })
	}
}
#[derive(Debug)]
pub struct LeadingCrossesCache {
	pub init_price: f64,
}
#[derive(Debug)]
pub struct SARCache {}
#[derive(Debug)]
pub struct TpSlCache {}

#[derive(Debug, Clone)]
pub struct InternalOrder {
	pub side: Side,
	pub price: f64,
}

pub trait Cache {
	async fn init() -> Result<Self> {}
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
