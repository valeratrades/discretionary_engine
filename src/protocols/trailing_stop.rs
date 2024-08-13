use crate::exchange_apis::{order_types::*, Market, Symbol};
use crate::positions::PositionSpec;
use crate::protocols::{Protocol, ProtocolOrders, ProtocolType};
use anyhow::Result;
use futures_util::StreamExt;
use serde_json::Value;
use std::cell::RefCell;
use std::str::FromStr;
use std::sync::{Arc, Mutex};
use tokio::sync::mpsc;
use tokio_tungstenite::connect_async;
use v_utils::io::Percent;
use v_utils::macros::CompactFormat;
use v_utils::prelude::*;
use v_utils::trades::Side;

#[derive(Clone)]
pub struct TrailingStopWrapper {
	params: RefCell<TrailingStop>,
}
impl FromStr for TrailingStopWrapper {
	type Err = anyhow::Error;

	fn from_str(spec: &str) -> Result<Self> {
		let ts = TrailingStop::from_str(spec)?;

		Ok(Self { params: RefCell::new(ts) })
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

		let (tx, mut rx) = tokio::sync::mpsc::channel::<f64>(256);
		position_set.spawn(async move {
			let mut s = JoinSet::new();
			s.spawn(async move {
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
			});

			s.spawn(async move {
				let mut ts_indicator = TrailingStopIndicator::new(params.borrow().percent, position_spec.side);

				while let Some(price) = rx.recv().await {
					let maybe_order = ts_indicator.step(price);
					if let Some(order) = maybe_order {
						let protocol_spec = params.borrow().to_string();
						let protocol_orders = ProtocolOrders::new(protocol_spec, vec![Some(order)]);
						tx_orders.send(protocol_orders).await.unwrap();
					}
				}
			});
			s.join_all().await;
			Ok(())
		});
		Ok(())
	}

	fn update_params(&self, new_params: &TrailingStop) -> Result<()> {
		*self.params.borrow_mut() = new_params.clone();
		Ok(())
	}

	fn get_subtype(&self) -> ProtocolType {
		ProtocolType::Momentum
	}
}

#[derive(Clone, Debug, Default, Copy)]
struct TrailingStopIndicator {
	percent: Percent,
	top: f64,
	bottom: f64,
	/// Side of the position. So the orders that the protocol places are in the opposite direction.
	side: Side,
}
impl TrailingStopIndicator {
	fn new(percent: Percent, side: Side) -> Self {
		Self {
			percent,
			top: 0.0,
			bottom: 0.0,
			side,
		}
	}

	fn step(&mut self, price: f64) -> Option<ConceptualOrderPercents> {
		if price < self.bottom || self.bottom == 0.0 {
			self.bottom = price;
			if self.side == Side::Sell {
				let target_price = price * Self::heuristic(self.percent.0, Side::Sell);
				let sm = ConceptualStopMarket::new(1.0, target_price);
				let order = Some(ConceptualOrderPercents::new(
					ConceptualOrderType::StopMarket(sm),
					Symbol::new("BTC", "USDT", Market::BinanceFutures),
					Side::Buy,
					1.0,
				));
				return order;
			}
		}
		if price > self.top || self.top == 0.0 {
			self.top = price;
			if self.side == Side::Buy {
				let target_price = price * Self::heuristic(self.percent.0, Side::Buy);
				let sm = ConceptualStopMarket::new(1.0, target_price);
				let order = Some(ConceptualOrderPercents::new(
					ConceptualOrderType::StopMarket(sm),
					Symbol::new("BTC", "USDT", Market::BinanceFutures),
					Side::Sell,
					1.0,
				));
				return order;
			}
		}
		None
	}

	fn heuristic(percent: f64, side: Side) -> f64 {
		let base = match side {
			Side::Buy => 1.0 - percent.abs(),
			Side::Sell => 1.0 + percent.abs(),
		};
		1.0 + base.ln()
	}
}

#[derive(Debug, Clone, CompactFormat, derive_new::new, Default)]
pub struct TrailingStop {
	percent: Percent,
}

#[cfg(test)]
mod tests {
	use super::*;

	#[tokio::test]
	async fn internals() {
		let mut ts = TrailingStopIndicator::new(Percent(0.02), Side::Buy);
		let mut orders = Vec::new();
		let prices = v_utils::distributions::laplace_random_walk(100.0, 1000, 0.1, 0.0, Some(42));
		for (i, price) in prices.iter().enumerate() {
			if let Some(order) = ts.step(*price) {
				let ConceptualOrderPercents { order_type, .. } = order;
				if let ConceptualOrderType::StopMarket(sm) = order_type {
					orders.push((i, Some(sm.price)));
				} else {
					panic!("Expected StopMarket order type");
				}
			}
		}
		let plot = v_utils::utils::snapshot_plot_orders(&prices, &orders);
		//let plot = v_utils::utils::SnapshotP::build(prices).draw();

		insta::assert_snapshot!(plot, @r###"
                                                                      ▂▃▄▃                  103.50
                                                                   ▃  █████▆▁▆▇▄                  
                                                                  ▅█▅▆██████████▃       ▃▆▄▄      
                                                                ▄▄███████████████▅▅▆▂  ▂████      
                                                              ▅▅█████████████████████▅▇█████      
                                                             ███████████████████████████████      
                     ▂                ▂        ▅▄▁▄         ▁███████████████████████████████      
                   ▆██▃▁         ▂▁  ▅█▇▄   ▁ █████▁ ▅    ▃▅████████████████████████████████      
  ▂▃  ▃           ▄█████▇     ▆▆▇██▇▆████▆▅▆█▇██████▇█▇ ▂▁██████████████████████████████████      
  ██▃▅█▇▆ ▃       ███████▇ ▇█▅█████████████████████████▆████████████████████████████████████      
  █████████▇▃ ▁  ▇████████▄█████████████████████████████████████████████████████████████████      
  ███████████▇█▇▇███████████████████████████████████████████████████████████████████████████98.73
  ──────────────────────────────────────────────────────────────────────────────────────────
                                                                      ▃▆▆███████████████████101.41
                                                                  ▁▆▆▆██████████████████████      
                                                               ▁▃▅██████████████████████████      
                                                              ▆█████████████████████████████      
                                                 ▁▁▁▁▁▁▁▁▁▁▁▁███████████████████████████████      
                   ▂▅▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇█████████████████████████████████████████████████████      
  ▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▂█████████████████████████████████████████████████████████████████████████97.98
  "###);
	}
}
