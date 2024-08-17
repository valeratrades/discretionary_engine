use std::str::FromStr;

use anyhow::Result;
use discretionary_engine_macros::ProtocolWrapper;
use futures_util::StreamExt;
use serde_json::Value;
use tokio::{
	sync::mpsc,
	task::JoinSet,
};
use tokio_tungstenite::connect_async;
use v_utils::{
	io::Percent,
	macros::CompactFormat,
	prelude::*,
	trades::{Ohlc, Side, Timeframe},
};

use crate::{
	exchange_apis::{order_types::*, Market, Symbol},
	positions::PositionSpec,
	protocols::{Protocol, ProtocolOrders, ProtocolType},
};

#[derive(Debug, Clone, CompactFormat, derive_new::new, Default, Copy, ProtocolWrapper)]
pub struct Sar {
	start: Percent,
	increment: Percent,
	max: Percent,
	timeframe: Timeframe,
}

#[derive(Clone, Debug, Default, derive_new::new, Copy)]
struct DataSource;
impl DataSource {
	/// HACK
	async fn listen(&self, address: &str, tx: tokio::sync::mpsc::Sender<Ohlc>) -> Result<()> {
		let (ws_stream, _) = connect_async(address).await.unwrap();
		let (_, mut read) = ws_stream.split();

		while let Some(msg) = read.next().await {
			let data = msg.unwrap().into_data();
			match serde_json::from_slice::<Value>(&data) {
				Ok(json) =>
					if let Some(open_str) = json.get("o") {
						let open: f64 = open_str.as_str().unwrap().parse().unwrap();
						let high: f64 = json["h"].as_str().unwrap().parse().unwrap();
						let low: f64 = json["l"].as_str().unwrap().parse().unwrap();
						let close: f64 = json["c"].as_str().unwrap().parse().unwrap();
						tx.send(Ohlc { open, high, low, close }).await.unwrap();
					},
				Err(e) => {
					println!("Failed to parse message as JSON: {}", e);
				}
			}
		}

		Ok(())
	}

	fn historic_klines_ohlc(&self, symbol: &str, timeframe: Timeframe, limit: u16) -> Result<Vec<Ohlc>> {
		unimplemented!()
	}
}

impl Protocol for SarWrapper {
	type Params = Sar;

	fn attach(&self, position_js: &mut JoinSet<Result<()>>, tx_orders: mpsc::Sender<ProtocolOrders>, position_spec: &PositionSpec) -> Result<()> {
		let symbol = Symbol {
			base: position_spec.asset.clone(),
			quote: "USDT".to_owned(),
			market: Market::BinanceFutures,
		};
		let tf = { &self.0.borrow().timeframe };
		let address = format!("wss://fstream.binance.com/ws/{}@kline_{tf}", symbol.to_string().to_lowercase());
		let position_spec = position_spec.clone();
		//BUG: ref trailing_stop.rs
		let params = self.0.clone();

		let (tx, mut rx) = tokio::sync::mpsc::channel::<Ohlc>(256);
		let init_klines = Vec::new(); //dbg
		let address_clone = address.clone();
		position_js.spawn(async move {
			let mut js = JoinSet::new();
			js.spawn(async move {
				//data_source_clone.listen(&address_clone, tx).await.unwrap();
				todo!()
			});

			js.spawn(async move {
				let position_side = position_spec.side;
				let mut sar = SarIndicator::init(&init_klines, &params.borrow());

				while let Some(ohlc) = rx.recv().await {
					// TODO!!!!!!: only update sar if the candle is over. Same for trade updates. (the only thing we want to be real-time is flipping of the indie, which is already captured by the placed stop_market)
					// TODO!!!!!!!!!: sub with SAR logic
					let prev_sar = sar;
					let maybe_order = sar.step(ohlc, &params.borrow(), &symbol, position_side);

					if sar.sar != prev_sar.sar {
						todo!();
						// update_orders!(sar, side);
					}
				}
			});
			js.join_all().await;
			Ok(())
		});

		unimplemented!();
	}

	fn update_params(&self, params: &Sar) -> Result<()> {
		unimplemented!()
	}

	fn get_subtype(&self) -> ProtocolType {
		ProtocolType::Momentum
	}
}

#[derive(Clone, Debug, Default, derive_new::new, Copy)]
struct SarIndicator {
	sar: f64,
	acceleration_factor: f64,
	extreme_point: f64,
}
impl SarIndicator {
	fn init(init_klines: &[Ohlc], params: &Sar) -> Self {
		let mut sar_indicator = Self {
			sar: init_klines[0].open,
			acceleration_factor: params.start.0,
			extreme_point: init_klines[0].open,
		};
		for ohlc in init_klines {
			_ = sar_indicator.step(*ohlc, params, &Symbol::default(), Side::default());
		}
		sar_indicator
	}

	fn step(&mut self, ohlc: Ohlc, params: &Sar, symbol: &Symbol, side: Side) -> Option<ConceptualOrderPercents> {
		let is_uptrend = self.sar < ohlc.low;
		let sar_snapshot = self.sar;

		// Update SAR
		if is_uptrend {
			self.sar = self.sar + self.acceleration_factor * (self.extreme_point - self.sar);
			self.sar = self.sar.min(ohlc.low).min(self.extreme_point);
		} else {
			self.sar = self.sar - self.acceleration_factor * (self.sar - self.extreme_point);
			self.sar = self.sar.max(ohlc.high).max(self.extreme_point);
		}

		// Update extreme point
		if is_uptrend {
			if ohlc.high > self.extreme_point {
				self.extreme_point = ohlc.high;
				self.acceleration_factor = (self.acceleration_factor + *params.increment).min(*params.max);
			}
		} else if ohlc.low < self.extreme_point {
			self.extreme_point = ohlc.low;
			self.acceleration_factor = (self.acceleration_factor + *params.increment).min(*params.max);
		}

		// Check for trend reversal
		if (is_uptrend && ohlc.low < self.sar) || (!is_uptrend && ohlc.high > self.sar) {
			self.sar = self.extreme_point;
			self.extreme_point = if is_uptrend { ohlc.low } else { ohlc.high };
			self.acceleration_factor = *params.start;
		}

		let is_followup_side = (side == Side::Buy && is_uptrend) || (side == Side::Sell && !is_uptrend);
		if is_followup_side {
			Some(ConceptualOrderPercents {
				order_type: ConceptualOrderType::StopMarket(ConceptualStopMarket::new(self.sar)),
				symbol: symbol.clone(),
				side,
				qty_percent_of_controlled: 1.0,
			})
		} else {
			None
		}
	}
}

//? should I move this higher up? Could help compile times, and standardize the check function.
#[cfg(test)]
mod tests {
	use super::*;
	use v_utils::trades::mock_p_to_ohlc;

	#[tokio::test]
	async fn test_sar_indicator() {
		let sar_wrapper = SarWrapper::from_str("sar:s0.07:i0.02:m0.15:t1m").unwrap();

		let init_p = v_utils::distributions::laplace_random_walk(100.0, 1000, 0.2, 0.0, Some(123));
		let init_p_reversed = init_p.into_iter().rev().collect::<Vec<_>>();
		let init_ohlc = mock_p_to_ohlc(&init_p_reversed, 10);

		let test_data_p = v_utils::distributions::laplace_random_walk(100.0, 1000, 0.2, 0.0, Some(42));
		let test_data_ohlc = mock_p_to_ohlc(&test_data_p, 10);

		let mut sar_indicator = SarIndicator::init(&init_ohlc, &sar_wrapper.0.borrow());
		let mut recorded_indicator_values = Vec::new();
		let mut i: usize = 0;
		let mut orders = Vec::new();

		for (i, ohlc) in test_data_ohlc.into_iter().enumerate() {
			let maybe_order = sar_indicator.step(ohlc, &sar_wrapper.0.borrow(), &Symbol::default(), Side::default());
			recorded_indicator_values.push(sar_indicator.sar);
			orders.push((i, maybe_order.map(|o| o.unsafe_stop_market().price)));
		}

		let snapshot = v_utils::utils::snapshot_plot_orders(&recorded_indicator_values, &orders);

		insta::assert_snapshot!(snapshot, @r###"
                                                                            ▆▄▃▁            107.00
                                                                          ▂▃████▆▄▁               
                                                                       ▂▅██████████▇▃▁            
                                                                     ▃▆███████████████▇   ▂▄      
                                                                    ███████████████████▂▅███      
                                                                 ▁▅█████████████████████████      
                   ▂▅▂               ▃▆             ▇▄▂         ▅███████████████████████████      
                   ███▆▂▁       ▃▄▅ ▅███▇▃▁         ███▆▃     ▄█████████████████████████████      
       ▃▂▁        ▆██████▄  ▅▆▆▆███▇███████      ▃▆▆██████  ▂▇██████████████████████████████      
  ▁▂▃▄▅████▅▄▂   ▇█████████████████████████    ▃▇█████████ ▄████████████████████████████████      
  █████████████▆▆██████████████████████████  ▂▇█████████████████████████████████████████████      
  █████████████████████████████████████████▁▆███████████████████████████████████████████████97.46
  ──────────────────────────────────────────────────────────────────────────────────────────
                                                                       ▂▄▆██                105.66
                                                                    ▄▅██████            ▂▄▅▇      
                                                                 ▁▄▆████████           █████      
                                                              ▁▄▇███████████           █████      
                                                 ▂▄▄        ▂▅██████████████           █████      
  ▄▅▅▆▇                                       ▂▅████      ▃▅████████████████           █████      
  █████                                    ▁▄▇██████      ██████████████████           █████97.46
  "###);
	}
}
