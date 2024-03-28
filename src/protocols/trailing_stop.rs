use crate::api::{
	binance::{self},
	order_types::*,
	Market, Symbol,
};
use crate::positions::PositionSpec;
use crate::protocols::RevisedProtocol;
use anyhow::Result;
use futures_util::StreamExt;
use serde_json::Value;
use std::cell::RefCell;
use std::sync::mpsc;
use tokio_tungstenite::connect_async;
use v_utils::macros::CompactFormat;
use v_utils::trades::Side;

#[derive(Debug, Clone, CompactFormat)]
pub struct TrailingStop {
	params: RefCell<TrailingStopParams>,
}

impl RevisedProtocol for TrailingStop {
	type Params = TrailingStopParams;

	/// Requested orders are being sent over the mspc with uuid of the protocol on each batch, as we want to replace the previous requested batch if any.
	fn attach(&self, tx_orders: mpsc::Sender<(Vec<OrderP>, String)>, position_spec: &PositionSpec) -> Result<()> {
		tokio::spawn(async move {
			let symbol = Symbol {
				base: position_spec.asset.clone(),
				quote: "USDT".to_owned(),
				market: Market::BinanceFutures,
			};
			let price = binance::futures_price(&symbol.base).await?;
			let mut top: f64 = price;
			let mut bottom: f64 = price;
			let side = position_spec.side.clone();
			let address = format!("wss://fstream.binance.com/ws/{}@aggTrade", symbol.to_string().to_lowercase());
			let uid = self.params.borrow().to_string();

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
								match side {
									Side::Buy => {}
									Side::Sell => {
										let target_price = price + price * self.percent;
										let _ = tx_orders.send((
											vec![OrderP::StopMarket(StopMarketP {
												symbol: symbol.clone(),
												side: Side::Buy,
												price: target_price,
												percent_size: 1.0,
											})],
											uid.clone(),
										));
									}
								}
							}
							if price > top {
								top = price;
								match side {
									Side::Buy => {
										let target_price = price - price * self.percent;
										let _ = tx_orders.send((
											vec![OrderP::StopMarket(StopMarketP {
												symbol: symbol.clone(),
												side: Side::Sell,
												price: target_price,
												percent_size: 1.0,
											})],
											uid.clone(),
										));
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

	fn update_params(&self, params: &Self::Params) -> Result<()> {
		todo!()
	}
}

#[derive(Debug, Clone, CompactFormat)]
struct TrailingStopParams {
	percent: f64,
}
