use crate::exchange_apis::{
	binance::{self},
	order_types::*,
	Market, Symbol,
};
use crate::positions::PositionSpec;
use crate::protocols::{Protocol, ProtocolOrders, ProtocolType};
use anyhow::Result;
use futures_util::StreamExt;
use serde_json::Value;
use std::sync::{Arc, Mutex};
use std::{collections::HashMap, str::FromStr};
use tokio::sync::mpsc;
use tokio_tungstenite::connect_async;
use uuid::Uuid;
use v_utils::macros::CompactFormat;
use v_utils::trades::Side;

#[derive(Debug, Clone)]
pub struct TrailingStopWrapper {
	params: Arc<Mutex<TrailingStop>>,
}
impl FromStr for TrailingStopWrapper {
	type Err = anyhow::Error;

	fn from_str(spec: &str) -> Result<Self> {
		let ts = TrailingStop::from_str(&spec)?;
		Ok(Self {
			params: Arc::new(Mutex::new(ts)),
		})
	}
}

impl Protocol for TrailingStopWrapper {
	type Params = TrailingStop;

	/// Requested orders are being sent over the mspc with uuid of the protocol on each batch, as we want to replace the previous requested batch if any.
	fn attach(&self, tx_orders: mpsc::Sender<ProtocolOrders>, position_spec: &PositionSpec) -> Result<()> {
		let symbol = Symbol {
			base: position_spec.asset.clone(),
			quote: "USDT".to_owned(),
			market: Market::BinanceFutures,
		};
		let address = format!("wss://fstream.binance.com/ws/{}@aggTrade", symbol.to_string().to_lowercase());

		let params = self.params.clone();
		let position_spec = position_spec.clone();

		// a thing that uniquely marks all the semantic orders of the grid the protocol may want to place.
		let mut order_mask: HashMap<Uuid, Option<ConceptualOrderPercents>> = HashMap::new();
		let stop_market_uuid = Uuid::new_v4();
		order_mask.insert(stop_market_uuid.clone(), None);

		macro_rules! send_orders {
			($target_price:expr, $side:expr) => {{
				let protocol_spec = params.lock().unwrap().to_string();
				let mut orders = order_mask.clone();

				let sm = ConceptualStopMarket::new(1.0, $target_price);
				orders.insert(
					stop_market_uuid,
					Some(ConceptualOrderPercents::new(
						ConceptualOrderType::StopMarket(sm),
						symbol.clone(),
						$side,
						1.0,
					)),
				);

				let protocol_orders = ProtocolOrders::new(protocol_spec, orders);
				tx_orders.send(protocol_orders).await.unwrap();
			}};
		}

		tokio::spawn(async move {
			let price = binance::futures_price(&symbol.base).await.unwrap();
			let mut top: f64 = price;
			let mut bottom: f64 = price;
			let position_side = position_spec.side.clone();

			let url = url::Url::parse(&address).unwrap();
			let (ws_stream, _) = connect_async(url).await.expect("Failed to connect");
			let (_, mut read) = ws_stream.split();

			while let Some(msg) = read.next().await {
				let data = msg.unwrap().into_data();
				match serde_json::from_slice::<Value>(&data) {
					Ok(json) => {
						if let Some(price_str) = json.get("p") {
							let price: f64 = price_str.as_str().unwrap().parse().unwrap();
							if price < bottom {
								bottom = price;
								match position_side {
									Side::Buy => {}
									Side::Sell => {
										let target_price = price + price * params.lock().unwrap().percent.abs();
										send_orders!(target_price, Side::Buy);
									}
								}
							}
							if price > top {
								top = price;
								match position_side {
									Side::Buy => {
										let target_price = price - price * params.lock().unwrap().percent.abs();
										send_orders!(target_price, Side::Sell);
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
		});

		Ok(())
	}

	fn update_params(&self, params: &TrailingStop) -> Result<()> {
		unimplemented!()
	}

	fn get_subtype(&self) -> ProtocolType {
		ProtocolType::Momentum
	}
}

impl TrailingStop {
	fn data_source(&self) -> String {
		unimplemented!()
	}
}

#[derive(Debug, Clone, CompactFormat)]
pub struct TrailingStop {
	percent: f64,
}
