use crate::api::binance::{self, futures_price};
use crate::api::order_types::*;
use crate::api::round_to_required_precision;
use crate::api::OrderSpec;
use crate::positions::{Position, PositionFollowup, PositionSpec};
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

pub enum ProtocolType {
	Momentum,
	TP,
	SL,
}

pub struct Protocol<T>
where
	T: FollowupProtocol + Clone + Send + Sync + FromStr,
	T::Err: std::error::Error + Send + Sync + 'static,
{
	pub spec: T,
	pub orders: Vec<OrderType>,
	pub cache: T::Cache,
}

impl<T> Protocol<T>
where
	T: FollowupProtocol + Clone + Send + Sync + FromStr,
	T::Err: std::error::Error + Send + Sync + 'static,
{
	fn build(s: &str, spec: &PositionSpec) -> anyhow::Result<Self> {
		let t = T::from_str(s)?;

		Ok(Self {
			spec: t.clone(),
			orders: Vec::new(),
			cache: T::Cache::build(t, spec),
		})
	}
}
/// Writes directly to the unprotected fields of CacheBlob, using unsafe
pub trait FollowupProtocol: FromStr + Clone {
	type Cache: ProtocolCache;
	async fn attach<T>(&self, orders: &mut Vec<OrderType>, cache: &mut Self::Cache) -> Result<()>;
	fn subtype(&self) -> ProtocolType;
}

pub trait ProtocolCache {
	fn build<T>(spec: T, position_spec: &PositionSpec) -> Self;
}

//=============================================================================
// Individual implementations
//=============================================================================

// Trailing Stop {{{
#[derive(Debug)]
pub struct TrailingStop {
	pub percent: f64,
}
impl FollowupProtocol for TrailingStop {
	type Cache = TrailingStopCache;

	async fn attach<T>(&self, orders: &mut Vec<OrderType>, cache: &mut Cache) -> Result<()> {
		let address = format!("wss://fstream.binance.com/ws/{}@markPrice", &cache.symbol);
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
							if price < cache.bottom {
								cache.bottom = price;
								match side {
									Side::Buy => {}
									Side::Sell => {
										let target_price = price + price * self.percent;
										orders.clear();
										orders.push(StopMarketWhere {
											symbol: cache.symbol,
											side: Side::Buy,
											price: target_price,
										});
									}
								}
							}
							if price > cache.top {
								cache.top = price;
								match side {
									Side::Buy => {
										let target_price = price - price * self.percent;
										orders.clear();
										orders.push(StopMarketWhere {
											symbol: cache.symbol,
											side: Side::Sell,
											price: target_price,
										});
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

	fn subtype(&self) -> ProtocolType {
		ProtocolType::Momentum
	}
}

/// Stores both highest and lowest prices in case the direction is switched for some reason. Note: it's not meant to though.
#[derive(Debug)]
pub struct TrailingStopCache {
	pub symbol: Symbol,
	pub top: f64,
	pub bottom: f64,
}
impl ProtocolCache for TrailingStopCache {
	fn build<T>(spec: T, position_core: PositionSpec) -> Self {
		let binance_symbol = Symbol {
			base: position_core.asset.clone(),
			quote: "USDT".to_owned(),
			market: Market::BinanceFutures,
		};
		let price = binance::futures_price(&binance_symbol.base).await?;
		Self {
			symbol: binance_symbol,
			top: price,
			bottom: price,
		}
	}
} //}}}
  //
  //// LeadingCrosses {{{
  //#[derive(Debug)]
  //pub struct LeadingCrossesCache {
  //	pub symbol: Symbol,
  //	pub init_price: f64,
  //}
  //#[derive(Debug)]
  //pub struct LeadingCrosses {
  //	pub symbol: Symbol,
  //	pub price: f64,
  //}
  //impl ProtocolCache for LeadingCrossesCache {
  //	fn build<LeadingCrosses>(spec: LeadingCrosses, position_core: PositionSpec) -> Self {
  //		let target_asset = spec.symbol.asset.clone();
  //		let price = binance::futures_price(target_asset).await?;
  //		Self {
  //			symbol: target_asset,
  //			init_price: price,
  //		}
  //	}
  //} //}}}
  //
  //// SAR {{{
  //#[derive(Debug)]
  //pub struct SAR {
  //	pub start: f64,
  //	pub increment: f64,
  //	pub max: f64,
  //	pub timeframe: Timeframe,
  //}
  //
  //#[derive(Debug)]
  //pub struct SARCache {}
  //impl ProtocolCache for SARCache {
  //	fn build<SAR>(spec: SAR, position_core: PositionSpec) -> Self {
  //		SARCache {}
  //	}
  //}
  ////}}}
  //
  //// TPSL {{{
  //#[derive(Debug)]
  //pub struct TPSL {
  //	pub market: Market,
  //	pub tp: f64,
  //	pub sl: f64,
  //}
  //
  //#[derive(Debug)]
  //pub struct TPSLCache {
  //	symbol: Symbol,
  //}
  //impl ProtocolCache for TpSlCache {
  //	fn build<TPSL>(spec: T, position_core: PositionSpec) -> Self {
  //		let binance_symbol = Symbol {
  //			base: position_core.asset.clone(),
  //			quote: "USDT".to_owned(),
  //			market: T.market.clone(),
  //		};
  //		TpSlCache { symbol: binance_symbol }
  //	}
  //}
  ////,}}}
