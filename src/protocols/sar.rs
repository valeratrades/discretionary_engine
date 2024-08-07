//use crate::exchange_apis::{order_types::*, Market, Symbol};
//use crate::positions::PositionSpec;
//use crate::protocols::{Protocol, ProtocolOrders, ProtocolType};
//use anyhow::Result;
//use futures_util::StreamExt;
//use serde_json::Value;
//use std::str::FromStr;
//use std::sync::{Arc, Mutex};
//use tokio::sync::{mpsc, watch};
//use tokio_tungstenite::connect_async;
//use v_utils::io::Percent;
//use v_utils::macros::CompactFormat;
//use v_utils::trades::mock_p_to_ohlc;
//use v_utils::trades::{Ohlc, Side, Timeframe};
//
//#[derive(Clone)]
//pub struct SarWrapper {
//	params: Arc<Mutex<Sar>>,
//	data_source: DataSource,
//}
//impl FromStr for SarWrapper {
//	type Err = anyhow::Error;
//
//	fn from_str(spec: &str) -> Result<Self> {
//		let ts = Sar::from_str(spec)?;
//
//		Ok(Self {
//			params: Arc::new(Mutex::new(ts)),
//			data_source: DataSource::Default(DefaultDataSource),
//		})
//	}
//}
//impl std::fmt::Debug for SarWrapper {
//	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
//		f.debug_struct("SarWrapper")
//			.field("params", &self.params)
//			.field("data_source", &"<FnMut>")
//			.finish()
//	}
//}
//
/////HACK
//#[derive(Debug, Clone, Copy)]
//pub enum DataSource {
//	Default(DefaultDataSource),
//	Test(TestDataSource),
//}
//impl DataSource {
//	async fn listen(&self, address: &str, tx: tokio::sync::mpsc::Sender<Ohlc>) -> Result<()> {
//		match self {
//			DataSource::Default(ds) => ds.listen(address, tx).await,
//			DataSource::Test(ds) => ds.listen(tx).await,
//		}
//	}
//
//	fn historic_klines_ohlc(&self, symbol: &str, timeframe: Timeframe, limit: u16) -> Result<Vec<Ohlc>> {
//		match self {
//			DataSource::Default(ds) => ds.historic_klines_ohlc(symbol, timeframe, limit),
//			DataSource::Test(ds) => Ok(ds.historic_klines_ohlc()),
//		}
//	}
//}
//
//#[derive(Clone, Debug, Default, derive_new::new, Copy)]
//struct DefaultDataSource;
//
//#[derive(Clone, Debug, Default, derive_new::new, Copy)]
//struct TestDataSource;
//
//impl DefaultDataSource {
//	///HACK
//	async fn listen(&self, address: &str, tx: tokio::sync::mpsc::Sender<Ohlc>) -> Result<()> {
//		let (ws_stream, _) = connect_async(address).await.unwrap();
//		let (_, mut read) = ws_stream.split();
//
//		while let Some(msg) = read.next().await {
//			let data = msg.unwrap().into_data();
//			match serde_json::from_slice::<Value>(&data) {
//				Ok(json) => {
//					if let Some(open_str) = json.get("o") {
//						let open: f64 = open_str.as_str().unwrap().parse().unwrap();
//						let high: f64 = json["h"].as_str().unwrap().parse().unwrap();
//						let low: f64 = json["l"].as_str().unwrap().parse().unwrap();
//						let close: f64 = json["c"].as_str().unwrap().parse().unwrap();
//						tx.send(Ohlc { open, high, low, close }).await.unwrap();
//					}
//				}
//				Err(e) => {
//					println!("Failed to parse message as JSON: {}", e);
//				}
//			}
//		}
//
//		Ok(())
//	}
//
//	fn historic_klines_ohlc(&self, symbol: &str, timeframe: Timeframe, limit: u16) -> Result<Vec<Ohlc>> {
//		unimplemented!()
//	}
//}
//
//impl TestDataSource {
//	async fn listen(&self, tx: tokio::sync::mpsc::Sender<Ohlc>) -> Result<()> {
//		let test_data_p = crate::utils::laplace_random_walk(100.0, 1000, 0.2, 0.0, Some(42));
//		let test_data_ohlc = mock_p_to_ohlc(&test_data_p, 10);
//		for ohlc in test_data_ohlc {
//			tx.send(ohlc).await.unwrap();
//		}
//
//		Ok(())
//	}
//
//	fn historic_klines_ohlc(&self) -> Vec<Ohlc> {
//		#[rustfmt::skip]
//		let example_historic_p = crate::utils::laplace_random_walk(100.0, 1000, 0.2, 0.0, Some(123));
//		let example_historic_p = example_historic_p.into_iter().rev().collect::<Vec<_>>();
//		let example_historic_ohlc = mock_p_to_ohlc(&example_historic_p, 10);
//		example_historic_ohlc
//	}
//}
//
//impl Protocol for SarWrapper {
//	type Params = Sar;
//
//	/// Requested orders are being sent over the mspc with uuid of the protocol on each batch, as we want to replace the previous requested batch if any.
//	fn attach(&self, tx_orders: mpsc::Sender<ProtocolOrders>, position_spec: &PositionSpec) -> Result<watch::Sender<()>> {
//		let symbol = Symbol {
//			base: position_spec.asset.clone(),
//			quote: "USDT".to_owned(),
//			market: Market::BinanceFutures,
//		};
//		let tf = { self.params.lock().unwrap().timeframe };
//		let address = format!("wss://fstream.binance.com/ws/{}@kline_{tf}", symbol.to_string().to_lowercase());
//
//		let params = self.params.clone();
//		let position_spec = position_spec.clone();
//
//		let order_mask: Vec<Option<ConceptualOrderPercents>> = vec![None; 1];
//		//TODO!: rewrite
//		macro_rules! update_orders {
//			($target_price:expr, $side:expr) => {{
//				let protocol_spec = params.lock().unwrap().to_string();
//				let mut orders = order_mask.clone();
//
//				let sm = ConceptualStopMarket::new(1.0, $target_price);
//				let order = Some(ConceptualOrderPercents::new(
//					ConceptualOrderType::StopMarket(sm),
//					symbol.clone(),
//					$side,
//					1.0,
//				));
//				orders[0] = order;
//
//				let protocol_orders = ProtocolOrders::new(protocol_spec, orders);
//				tx_orders.send(protocol_orders).await.unwrap();
//			}};
//		}
//
//		let (tx, mut rx) = tokio::sync::mpsc::channel::<Ohlc>(256);
//		let address_clone = address.clone();
//		let data_source_clone = self.data_source;
//		tokio::spawn(async move {
//			data_source_clone.listen(&address_clone, tx).await.unwrap();
//		});
//
//		tokio::spawn(async move {
//			let position_side = position_spec.side;
//			let mut sar = SarIndicator::init(&data_source_clone, params.clone(), &symbol);
//
//			while let Some(ohlc) = rx.recv().await {
//				//TODO!!!!!!: only update sar if the candle is over. Same for trade updates. (the only thing we want to be real-time is flipping of the indie, which is already captured by the placed stop_market)
//				//TODO!!!!!!!!!: sub with SAR logic
//				let prev_sar = sar;
//				sar.step(ohlc);
//
//				if sar.sar != prev_sar.sar {
//					todo!();
//					//update_orders!(sar, side);
//				}
//			}
//		});
//
//		unimplemented!();
//	}
//
//	fn update_params(&self, params: &Sar) -> Result<()> {
//		unimplemented!()
//	}
//
//	fn get_subtype(&self) -> ProtocolType {
//		ProtocolType::Momentum
//	}
//}
//
//#[derive(Clone, Debug, Default, derive_new::new, Copy)]
//struct SarIndicator {
//	sar: f64,
//	acceleration_factor: f64,
//	extreme_point: f64,
//	/// (start, increment, max)
//	params: (f64, f64, f64),
//}
//impl SarIndicator {
//	fn init(data_source: &DataSource, protocol_params: Arc<Mutex<Sar>>, symbol: &Symbol) -> Self {
//		let tf = { protocol_params.lock().unwrap().timeframe };
//		let historic_klines_ohlc = data_source.historic_klines_ohlc(&symbol.to_string(), tf, 100).unwrap();
//
//		let mut extreme_point = historic_klines_ohlc[0].open;
//		let mut sar = historic_klines_ohlc[0].open;
//		let (start, increment, max) = {
//			let params_lock = protocol_params.lock().unwrap();
//			(params_lock.start.0, params_lock.increment.0, params_lock.maximum.0)
//		};
//		let mut acceleration_factor = start;
//
//		let mut sar_indicator = Self {
//			sar,
//			acceleration_factor,
//			extreme_point,
//			params: (start, increment, max),
//		};
//		for ohlc in historic_klines_ohlc {
//			sar_indicator.step(ohlc);
//		}
//		sar_indicator
//	}
//
//	fn step(&mut self, ohlc: Ohlc) {
//		let (start, increment, max) = self.params;
//		let is_uptrend = self.sar < ohlc.low;
//
//		// Update SAR
//		if is_uptrend {
//			self.sar = self.sar + self.acceleration_factor * (self.extreme_point - self.sar);
//			self.sar = self.sar.min(ohlc.low).min(self.extreme_point);
//		} else {
//			self.sar = self.sar - self.acceleration_factor * (self.sar - self.extreme_point);
//			self.sar = self.sar.max(ohlc.high).max(self.extreme_point);
//		}
//
//		// Update extreme point
//		if is_uptrend {
//			if ohlc.high > self.extreme_point {
//				self.extreme_point = ohlc.high;
//				self.acceleration_factor = (self.acceleration_factor + increment).min(max);
//			}
//		} else if ohlc.low < self.extreme_point {
//			self.extreme_point = ohlc.low;
//			self.acceleration_factor = (self.acceleration_factor + increment).min(max);
//		}
//
//		// Check for trend reversal
//		if (is_uptrend && ohlc.low < self.sar) || (!is_uptrend && ohlc.high > self.sar) {
//			self.sar = self.extreme_point;
//			self.extreme_point = if is_uptrend { ohlc.low } else { ohlc.high };
//			self.acceleration_factor = start;
//		}
//	}
//}
//
//impl SarWrapper {
//	pub fn set_data_source(mut self, new_data_source: DataSource) -> Self {
//		self.data_source = new_data_source;
//		self
//	}
//}
//
//#[derive(Debug, Clone, CompactFormat, derive_new::new, Copy)]
//pub struct Sar {
//	start: Percent,
//	increment: Percent,
//	maximum: Percent,
//	timeframe: Timeframe,
//}
//
////? should I move this higher up? Could compile times, and standardize the check function.
//#[cfg(test)]
//mod tests {
//	use super::*;
//
//	#[tokio::test]
//	async fn test_sar_indicator() {
//		let ts = SarWrapper::from_str("sar:s0.07:i0.02:m0.15:t1m")
//			.unwrap()
//			.set_data_source(DataSource::Test(TestDataSource));
//
//		let mut sar = SarIndicator::init(&ts.data_source, ts.params.clone(), &Symbol::new("BTC", "USDT", Market::BinanceFutures));
//
//		let datasource_clone = ts.data_source.clone();
//		let (tx, mut rx) = tokio::sync::mpsc::channel(32);
//		tokio::spawn(async move {
//			datasource_clone.listen("", tx).await.unwrap();
//		});
//
//		let mut received_data = Vec::new();
//		while let Some(data) = rx.recv().await {
//			received_data.push(data);
//		}
//
//		let mut recorded_indicator_values = Vec::new();
//		for ohlc in received_data {
//			sar.step(ohlc);
//			recorded_indicator_values.push(sar.sar);
//		}
//		let snapshot = v_utils::utils::snapshot_plot_p(&recorded_indicator_values, 90, 12);
//
//		insta::assert_snapshot!(snapshot, @r###"
//                                                                            ▆▄▃▁
//                                                                          ▂▃████▆▄▁
//                                                                       ▂▅██████████▇▃▁
//                                                                     ▃▆███████████████▇   ▂▄
//                                                                    ███████████████████▂▅███
//                                                                 ▁▅█████████████████████████
//                   ▂▅▂               ▃▆             ▇▄▂         ▅███████████████████████████
//                   ███▆▂▁       ▃▄▅ ▅███▇▃▁         ███▆▃     ▄█████████████████████████████
//       ▃▂▁        ▆██████▄  ▅▆▆▆███▇███████      ▃▆▆██████  ▂▇██████████████████████████████
//  ▁▂▃▄▅████▅▄▂   ▇█████████████████████████    ▃▇█████████ ▄████████████████████████████████
//  █████████████▆▆██████████████████████████  ▂▇█████████████████████████████████████████████
//  █████████████████████████████████████████▁▆███████████████████████████████████████████████
//  "###);
//	}
//
//	//? Could I move part of this operation inside the check function, following https://matklad.github.io/2021/05/31/how-to-test.html ?
//	#[tokio::test]
//	async fn test_sar_orders() {
//		let ts = SarWrapper::from_str("sar:s0.07:i0.02:m0.15:t1m")
//			.unwrap()
//			.set_data_source(DataSource::Test(TestDataSource));
//
//		let position_spec = PositionSpec::new("BTC".to_owned(), Side::Buy, 1.0);
//
//		let (tx, mut rx) = tokio::sync::mpsc::channel(32);
//		tokio::spawn(async move {
//			ts.attach(tx, &position_spec).unwrap();
//		});
//
//		let mut received_data = Vec::new();
//		while let Some(data) = rx.recv().await {
//			received_data.push(data);
//		}
//
//		let received_data_inner_orders = received_data.iter().map(|x| x.__orders.clone()).collect::<Vec<_>>();
//
//		insta::assert_json_snapshot!(
//			received_data_inner_orders,
//			@"[]",
//		);
//	}
//}
