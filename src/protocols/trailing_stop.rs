use crate::exchange_apis::{order_types::*, Market, Symbol};
use crate::positions::PositionSpec;
use crate::protocols::{Protocol, ProtocolOrders, ProtocolType};
use anyhow::Result;
use futures_util::StreamExt;
use serde_json::Value;
use std::str::FromStr;
use std::sync::{Arc, Mutex};
use tokio::sync::{mpsc, watch};
use tokio_tungstenite::connect_async;
use v_utils::io::Percent;
use v_utils::macros::CompactFormat;
use tokio::task::{JoinHandle, JoinSet};
use v_utils::trades::Side;

#[derive(Clone)]
pub struct TrailingStopWrapper {
	params: Arc<Mutex<TrailingStop>>,
	data_source: DataSource,
}
impl FromStr for TrailingStopWrapper {
	type Err = anyhow::Error;

	fn from_str(spec: &str) -> Result<Self> {
		let ts = TrailingStop::from_str(spec)?;

		Ok(Self {
			params: Arc::new(Mutex::new(ts)),
			data_source: DataSource::Default,
		})
	}
}
impl std::fmt::Debug for TrailingStopWrapper {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		f.debug_struct("TrailingStopWrapper")
			.field("params", &self.params)
			.field("data_source", &"<FnMut>")
			.finish()
	}
}

#[derive(Debug, Clone, Copy)]
pub enum DataSource {
	Default,
	Test,
}
impl DataSource {
	async fn listen(&self, address: &str, tx: tokio::sync::mpsc::Sender<f64>) -> Result<()> {
		match self {
			DataSource::Default => {
				let (ws_stream, _) = connect_async(address).await.unwrap();
				let (_, mut read) = ws_stream.split();

				while let Some(msg) = read.next().await {
					let data = msg.unwrap().into_data();
					match serde_json::from_slice::<Value>(&data) {
						Ok(json) => {
							if let Some(price_str) = json.get("p") {
								let price: f64 = price_str.as_str().unwrap().parse().unwrap();
								tx.send(price).await.unwrap();
							}
						}
						Err(e) => {
							println!("Failed to parse message as JSON: {}", e);
						}
					}
				}
			}
			DataSource::Test => {
				let test_data = vec![100.0, 100.5, 102.5, 100.0, 101.0, 97.0, 102.6];
				for price in test_data {
					tx.send(price).await.unwrap();
				}
			}
		}

		Ok(())
	}
}

impl Protocol for TrailingStopWrapper {
	type Params = TrailingStop;

	/// Requested orders are being sent over the mspc with uuid of the protocol on each batch, as we want to replace the previous requested batch if any.
	fn attach(&self, position_set: &mut JoinSet<Result<()>>, tx_orders: mpsc::Sender<ProtocolOrders>, position_spec: &PositionSpec) -> Result<()> {
		let symbol = Symbol {
			base: position_spec.asset.clone(),
			quote: "USDT".to_owned(),
			market: Market::BinanceFutures,
		};
		let address = format!("wss://fstream.binance.com/ws/{}@aggTrade", symbol.to_string().to_lowercase());

		let params = self.params.clone();
		let position_spec = position_spec.clone();

		let order_mask: Vec<Option<ConceptualOrderPercents>> = vec![None; 1];
		macro_rules! send_orders {
			($target_price:expr, $side:expr) => {{
				let protocol_spec = params.lock().unwrap().to_string();
				let mut orders = order_mask.clone();

				let sm = ConceptualStopMarket::new(1.0, $target_price);
				let order = Some(ConceptualOrderPercents::new(
					ConceptualOrderType::StopMarket(sm),
					symbol.clone(),
					$side,
					1.0,
				));
				orders[0] = order;

				let protocol_orders = ProtocolOrders::new(protocol_spec, orders);
				tx_orders.send(protocol_orders).await.unwrap();
			}};
		}

		let (tx, mut rx) = tokio::sync::mpsc::channel::<f64>(256);
		let address_clone = address.clone();
		let data_source_clone = self.data_source;

		position_set.spawn(async move {
			//BUG: s drops immediately, kills all tasks
			let mut s = JoinSet::new();
			s.spawn(async move {
				dbg!(&"first");
				data_source_clone.listen(&address_clone, tx).await.unwrap()
			});

			s.spawn(async move {
				dbg!(&"second");
				let position_side = position_spec.side;
				let mut top = 0.0;
				let mut bottom = 0.0;

				while let Some(price) = rx.recv().await {
					dbg!(&price);
					if price < bottom || bottom == 0.0 {
						bottom = price;
						match position_side {
							Side::Buy => {}
							Side::Sell => {
								let target_price = price * heuristic(params.lock().unwrap().percent.0, Side::Sell);
								send_orders!(target_price, Side::Buy);
							}
						}
					}
					if price > top || top == 0.0 {
						top = price;
						match position_side {
							Side::Buy => {
								let target_price = price * heuristic(params.lock().unwrap().percent.0, Side::Buy);
								send_orders!(target_price, Side::Sell);
							}
							Side::Sell => {}
						}
					}
				}
			});
			//HACK: must be a way to write more concisely
			while (s.join_next().await).is_some() {};
			Ok(())
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

impl TrailingStopWrapper {
	pub fn set_data_source(mut self, new_data_source: DataSource) -> Self {
		self.data_source = new_data_source;
		self
	}
}

fn heuristic(percent: f64, side: Side) -> f64 {
	let base = match side {
		Side::Buy => 1.0 - percent.abs(),
		Side::Sell => 1.0 + percent.abs(),
	};
	1.0 + base.ln()
}

#[derive(Debug, Clone, CompactFormat, derive_new::new, Default)]
pub struct TrailingStop {
	percent: Percent,
}

//? should I move this higher up? Could compile times, and standardize the check function.
#[cfg(test)]
mod tests {
	use super::*;

	//? Could I move part of this operation inside the check function, following https://matklad.github.io/2021/05/31/how-to-test.html ?
	#[tokio::test]
	async fn test_trailing_stop() {
		let percent = 0.02;
		let ts = TrailingStopWrapper::from_str(&format!("ts:p{percent}"))
			.unwrap()
			.set_data_source(DataSource::Test);

		let position_spec = PositionSpec::new("BTC".to_owned(), Side::Buy, 1.0);

		let (tx, mut rx) = tokio::sync::mpsc::channel(32);
		let mut set = JoinSet::new();
		ts.attach(&mut set, tx, &position_spec).unwrap();

		let mut received_data = Vec::new();
		while let Some(data) = rx.recv().await {
			received_data.push(data);
		}

		let received_data_inner_orders = received_data.iter().map(|x| x.__orders.clone()).collect::<Vec<_>>();

		insta::assert_json_snapshot!(
			received_data_inner_orders,
			@r###"
  [
    [
      {
        "order_type": {
          "StopMarket": {
            "maximum_slippage_percent": 1.0,
            "price": 97.97972926824805
          }
        },
        "symbol": {
          "base": "BTC",
          "quote": "USDT",
          "market": "BinanceFutures"
        },
        "side": "Sell",
        "qty_percent_of_controlled": 1.0
      }
    ],
    [
      {
        "order_type": {
          "StopMarket": {
            "maximum_slippage_percent": 1.0,
            "price": 98.4696279145893
          }
        },
        "symbol": {
          "base": "BTC",
          "quote": "USDT",
          "market": "BinanceFutures"
        },
        "side": "Sell",
        "qty_percent_of_controlled": 1.0
      }
    ],
    [
      {
        "order_type": {
          "StopMarket": {
            "maximum_slippage_percent": 1.0,
            "price": 100.42922249995425
          }
        },
        "symbol": {
          "base": "BTC",
          "quote": "USDT",
          "market": "BinanceFutures"
        },
        "side": "Sell",
        "qty_percent_of_controlled": 1.0
      }
    ],
    [
      {
        "order_type": {
          "StopMarket": {
            "maximum_slippage_percent": 1.0,
            "price": 100.5272022292225
          }
        },
        "symbol": {
          "base": "BTC",
          "quote": "USDT",
          "market": "BinanceFutures"
        },
        "side": "Sell",
        "qty_percent_of_controlled": 1.0
      }
    ]
  ]
  "###,
		);
	}
}
