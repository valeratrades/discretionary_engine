use crate::api::{
	binance::{self},
	order_types::*,
	Market, Symbol,
};
use crate::positions::PositionSpec;
use crate::protocols::{FollowupProtocol, ProtocolCache, ProtocolType};
use anyhow::Result;
use futures_util::StreamExt;
use serde_json::Value;
use std::{
	any::Any,
	sync::{Arc, Mutex},
};
use tokio_tungstenite::connect_async;
use v_utils::macros::CompactFormat;
use v_utils::trades::Side;

#[derive(Debug, Clone, CompactFormat)]
pub struct TrailingStop {
	pub percent: f64,
}
impl FollowupProtocol for TrailingStop {
	type Cache = TrailingStopCache;
	type Item = TrailingStop;

	async fn attach(&self, orders: Arc<Mutex<Vec<OrderP>>>, cache: Arc<Mutex<Self::Cache>>) -> Result<()> {
		let address = format!(
			"wss://fstream.binance.com/ws/{}@aggTrade",
			&cache.lock().unwrap().symbol.to_string().to_lowercase()
		);
		let url = url::Url::parse(&address).unwrap();
		let (ws_stream, _) = connect_async(url).await.expect("Failed to connect");
		let (_, mut read) = ws_stream.split();

		while let Some(msg) = read.next().await {
			let data = msg.unwrap().into_data();
			match serde_json::from_slice::<Value>(&data) {
				Ok(json) => {
					let mut cache_lock = cache.lock().unwrap();

					if let Some(price_str) = json.get("p") {
						let price: f64 = price_str.as_str().unwrap().parse().unwrap();
						if price < cache_lock.bottom {
							cache_lock.bottom = price;
							match cache_lock.side {
								Side::Buy => {}
								Side::Sell => {
									let target_price = price + price * self.percent;
									let mut orders_lock = orders.lock().unwrap();
									orders_lock.clear();
									orders_lock.push(OrderP::StopMarket(StopMarketP {
										symbol: cache_lock.symbol.clone(),
										side: Side::Buy,
										price: target_price,
										percent_size: 1.0,
									}));
								}
							}
						}
						if price > cache_lock.top {
							cache_lock.top = price;
							match cache_lock.side {
								Side::Buy => {
									let target_price = price - price * self.percent;
									let mut orders_lock = orders.lock().unwrap();
									orders_lock.clear();
									orders_lock.push(OrderP::StopMarket(StopMarketP {
										symbol: cache_lock.symbol.clone(),
										side: Side::Sell,
										price: target_price,
										percent_size: 1.0,
									}));
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
		Ok(())
	}

	fn as_any(&self) -> &dyn Any {
		self
	}

	fn subtype(&self) -> ProtocolType {
		ProtocolType::Momentum
	}

	fn get_item(&self) -> Self::Item {
		self.clone()
	}
}

/// Stores both highest and lowest prices in case the direction is switched for some reason. Note: it's not meant to though.
#[derive(Debug)]
pub struct TrailingStopCache {
	pub symbol: Symbol,
	pub top: f64,
	pub bottom: f64,
	pub side: Side,
}

struct CacheInfoCarrier {
	symbol: Symbol,
	top: f64,
	bottom: f64,
	side: Side,
}

impl ProtocolCache for TrailingStopCache {
	async fn build(position_spec: &PositionSpec) -> Result<Self> {
		let binance_symbol = Symbol {
			base: position_spec.asset.clone(),
			quote: "USDT".to_owned(),
			market: Market::BinanceFutures,
		};
		let price = binance::futures_price(&binance_symbol.base).await?;
		Ok(Self {
			symbol: binance_symbol,
			top: price,
			bottom: price,
			side: position_spec.side.clone(),
		})
	}
}
