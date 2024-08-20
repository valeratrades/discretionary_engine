use anyhow::Result;
use discretionary_engine_macros::ProtocolWrapper;
use futures_util::StreamExt;
use serde_json::Value;
use tokio::sync::mpsc;
use tokio_tungstenite::connect_async;
use v_utils::{io::Percent, macros::CompactFormat, prelude::*, trades::Side};

use crate::{
	exchange_apis::{order_types::*, Market, Symbol},
	positions::PositionSpec,
	protocols::{ProtocolOrders, ProtocolTrait, ProtocolType},
};

#[derive(Debug, Clone, CompactFormat, derive_new::new, Default, Copy, ProtocolWrapper)]
pub struct ApproachingLimit {
	percent: Percent,
}

impl ProtocolTrait for ApproachingLimitWrapper {
	type Params = ApproachingLimit;

	fn attach(&self, position_js: &mut JoinSet<Result<()>>, tx_orders: mpsc::Sender<ProtocolOrders>, asset: String, protocol_side: Side) -> Result<()> {
		let symbol = Symbol {
			base: asset,
			quote: "USDT".to_owned(),
			market: Market::BinanceFutures,
		};
		let address = format!("wss://fstream.binance.com/ws/{}@aggTrade", symbol.to_string().to_lowercase());

		// BUG: update_params will do nothing, as we're cloning them before starting the tasks.
		let params = self.0.clone();
		let (tx, mut rx) = tokio::sync::mpsc::channel::<f64>(256);
		position_js.spawn(async move {
			let mut js = JoinSet::new();
			js.spawn(async move {
				let (ws_stream, _) = connect_async(address).await.unwrap();
				let (_, mut read) = ws_stream.split();

				while let Some(msg) = read.next().await {
					let data = msg.unwrap().into_data();
					match serde_json::from_slice::<Value>(&data) {
						Ok(json) =>
							if let Some(price_str) = json.get("p") {
								let price: f64 = price_str.as_str().unwrap().parse().unwrap();
								tx.send(price).await.unwrap();
							},
						Err(e) => {
							println!("Failed to parse message as JSON: {}", e);
						}
					}
				}
			});

			js.spawn(async move {
				let mut ts_indicator = ApproachingLimitIndicator::new();
				while let Some(price) = rx.recv().await {
					let maybe_order = ts_indicator.step(price, params.read().unwrap().percent, protocol_side, &symbol);
					if let Some(order) = maybe_order {
						let protocol_spec = params.read().unwrap().to_string();
						let protocol_orders = ProtocolOrders::new(protocol_spec, vec![Some(order)]);
						tx_orders.send(protocol_orders).await.unwrap();
					}
				}
			});
			js.join_all().await;
			Ok(())
		});
		Ok(())
	}

	fn update_params(&self, new_params: ApproachingLimit) -> Result<()> {
		*self.0.write().unwrap() = new_params;
		Ok(())
	}

	fn get_subtype(&self) -> ProtocolType {
		ProtocolType::Momentum
	}
}

#[derive(Clone, Debug, Default, Copy)]
struct ApproachingLimitIndicator {
	top: f64,
	bottom: f64,
}
impl ApproachingLimitIndicator {
	fn new() -> Self {
		Self { top: 0.0, bottom: 0.0 }
	}

	fn step(&mut self, price: f64, percent: Percent, side: Side, symbol: &Symbol) -> Option<ConceptualOrderPercents> {
		unimplemented!();
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

#[cfg(test)]
mod tests {
	use super::*;

	#[tokio::test]
	async fn internals() {
		let mut ts = ApproachingLimitIndicator::new();
		let mut orders = Vec::new();
		let prices = v_utils::distributions::laplace_random_walk(100.0, 1000, 0.1, 0.0, Some(42));
		for (i, price) in prices.iter().enumerate() {
			if let Some(order) = ts.step(*price, Percent(0.02), Side::Buy, &Symbol::new("BTC", "USDT", Market::BinanceFutures)) {
				let ConceptualOrderPercents { order_type, .. } = order;
				if let ConceptualOrderType::StopMarket(sm) = order_type {
					orders.push((i, Some(sm.price)));
				} else {
					panic!("Expected StopMarket order type");
				}
			}
		}
		let plot = v_utils::utils::snapshot_plot_orders(&prices, &orders);
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
