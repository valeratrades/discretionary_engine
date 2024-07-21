use crate::exchange_apis::{order_types::*, Market, Symbol};
use crate::positions::PositionSpec;
use crate::protocols::{Protocol, ProtocolOrders, ProtocolType};
use anyhow::Result;
use futures_util::StreamExt;
use serde_json::Value;
use std::str::FromStr;
use std::sync::{Arc, Mutex};
use tokio::sync::mpsc;
use tokio_tungstenite::connect_async;
use v_utils::io::Percent;
use v_utils::macros::CompactFormat;
use v_utils::trades::{Timeframe, Ohlc, Side};
use v_utils::trades::mock_p_to_ohlc;

#[derive(Clone)]
pub struct SarWrapper {
	params: Arc<Mutex<Sar>>,
	data_source: DataSource,
}
impl FromStr for SarWrapper {
	type Err = anyhow::Error;

	fn from_str(spec: &str) -> Result<Self> {
		let ts = Sar::from_str(spec)?;

		Ok(Self {
			params: Arc::new(Mutex::new(ts)),
			data_source: DataSource::Default(DefaultDataSource),
		})
	}
}
impl std::fmt::Debug for SarWrapper {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		f.debug_struct("SarWrapper")
			.field("params", &self.params)
			.field("data_source", &"<FnMut>")
			.finish()
	}
}

///HACK
#[derive(Debug, Clone, Copy)]
pub enum DataSource {
	Default(DefaultDataSource),
	Test(TestDataSource),
}
impl DataSource {
	async fn listen(&self, address: &str, tx: tokio::sync::mpsc::Sender<Ohlc>) -> Result<()> {
		match self {
			DataSource::Default(ds) => ds.listen(address, tx).await,
			DataSource::Test(ds) => ds.listen(tx).await,
		}
	}
	fn historic_klines_ohlc(&self, symbol: &str, timeframe: Timeframe, limit: u16) -> Result<Vec<Ohlc>> {
		match self {
			DataSource::Default(ds) => ds.historic_klines_ohlc(symbol, timeframe, limit),
			DataSource::Test(ds) => Ok(ds.historic_klines_ohlc()),
		}
	}
}

#[derive(Clone, Debug, Default, derive_new::new, Copy)]
struct DefaultDataSource;

#[derive(Clone, Debug, Default, derive_new::new, Copy)]
struct TestDataSource;

impl DefaultDataSource {
	///HACK
	async fn listen(&self, address: &str, tx: tokio::sync::mpsc::Sender<Ohlc>) -> Result<()> {
		let (ws_stream, _) = connect_async(address).await.unwrap();
		let (_, mut read) = ws_stream.split();

		while let Some(msg) = read.next().await {
			let data = msg.unwrap().into_data();
			match serde_json::from_slice::<Value>(&data) {
				Ok(json) => {
					if let Some(open_str) = json.get("o") {
						let open: f64 = open_str.as_str().unwrap().parse().unwrap();
						let high: f64 = json["h"].as_str().unwrap().parse().unwrap();
						let low: f64 = json["l"].as_str().unwrap().parse().unwrap();
						let close: f64 = json["c"].as_str().unwrap().parse().unwrap();
						tx.send(Ohlc { open, high, low, close }).await.unwrap();
					}
				}
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

impl TestDataSource {
	async fn listen(&self, tx: tokio::sync::mpsc::Sender<Ohlc>) -> Result<()> {
		//TODO!: gen 10 candles the same way as the history
		#[rustfmt::skip]
		let test_data_p = vec![100.0, 99.98034123405445, 100.0347290174959, 100.48839759605941, 99.62133401559197, 101.38574519793993, 101.6684245335374, 101.65966766136323, 101.70648749936485, 102.6232010411682, 102.97313350013474, 101.55631004207399, 100.25871594663444, 100.52272804857675, 100.58314893786022, 100.64283254607244, 100.73433531354823, 100.69221517631237, 100.09351720527273, 100.67293466664673, 100.64235444424168, 100.37334762199043, 101.05505250560705, 101.96492364175322, 102.2552341902445, 102.4643771874453, 103.00400856658018, 103.0770079705397, 103.02995640665938, 102.38206280914957, 101.44333626880916, 101.01280839314724, 100.9499248204719, 101.78576790776899, 102.10434545937888, 102.41886658150547, 101.8961177804279, 101.91029272363858, 104.75134118777744, 104.6278560056506, 104.58452393952936, 104.21408906771778, 103.83574406047777, 103.88493636600897, 103.59095001733286, 102.99965993528096, 103.08175530600438, 102.23148201587901, 102.38348765012664, 102.68463685169142, 102.78148763710935, 102.48123981286992, 102.87908213769386, 101.54193253304851, 102.05643181896018, 103.26123912359945, 103.69839088086984, 103.83468348905919, 104.04304962479134, 104.95516117788536, 104.92389865980158, 105.35315115800985, 104.7544940516362, 105.36401129198312, 105.37857194360474, 106.45390037633943, 105.00661272503059, 105.82631191045223, 106.28603604450699, 106.66008635913374, 105.11486352514159, 105.34500651042048, 105.23385387405953, 104.85123641027657, 105.39713078569835, 105.55530324795174, 105.79159364234994, 105.92782737092307, 108.05899313915141, 107.89735278459993, 108.43341001175129, 108.32542181864629, 108.33872576814629, 108.33443914321589, 108.55780426988207, 108.4253892576315, 107.50736654802179, 107.62402763087272, 107.51398114643504, 107.47638374795653, 107.55541974293325, 107.94972268681686, 108.00694173462705, 108.7869334128387, 107.90069882793894, 107.5365360328119, 106.69100048255488, 106.63267206807168, 107.03367790159332, 106.33479734000295, 106.585157352886];
		let test_data_ohlc = mock_p_to_ohlc(&test_data_p, 4);
		for ohlc in test_data_ohlc {
			tx.send(ohlc).await.unwrap();
		}

		Ok(())
	}

	fn historic_klines_ohlc(&self) -> Vec<Ohlc> {
		//TODO!: shove into ohlc (probably gen 10x, then squeeze on 10-by-10 basis (reduce var for each step))
		#[rustfmt::skip]
		let example_historic_p = vec![104.29141020478691, 106.23121625787837, 105.75048573140208, 105.2911472757244, 105.26324218407512, 104.98280771004836, 104.97961348542003, 105.41307641928171, 106.25256127580126, 106.06629311708362, 105.97539626207127, 106.27009444000736, 105.91499253346099, 105.97129070024222, 105.5088666906226, 105.78790773728736, 105.67073875715707, 105.71622524906917, 104.76623503992752, 104.09540015806775, 104.0037221865679, 104.06491359240626, 103.78368766653739, 103.75522849764758, 103.69036031900345, 103.72356746390756, 104.06827763816528, 104.0347951530378, 104.25819510645442, 105.75405114549801, 105.97362964119652, 105.61852599258826, 104.36364615386984, 104.4991639308393, 104.42949488242556, 104.42504233817463, 104.76092309542352, 104.90608464832697, 105.0742039301147, 105.03555719427408, 104.77617684464893, 105.98783002292284, 106.87699335024006, 106.24484731719853, 104.97381491835762, 104.44383983474492, 104.20285638189209, 104.28722718963704, 104.35934558819474, 104.65922938706265, 104.65073638375839, 104.34287081936398, 104.3037729688407, 103.48779326064093, 103.01898431055784, 103.01431771890114, 102.12283037969767, 102.65220891609908, 104.0962204705779, 107.22183953493864, 107.34003991297988, 107.70116292878137, 107.51969173340188, 107.22679354788556, 107.47483453311067, 106.81568492733258, 106.3118483770158, 106.29759835329595, 105.9438153393612, 105.22460502843589, 105.09055147372698, 104.11012455228018, 104.74946167643407, 105.0404190365796, 104.9685840384766, 104.9041612345468, 105.07788103638063, 105.2285390773552, 105.29694972979458, 104.88356738043646, 104.42301903565114, 104.56821139080125, 104.6647564046602, 104.27592491336831, 104.39737578208953, 103.81396894051164, 104.18863162102353, 104.60837629767862, 106.17398882276363, 106.27926140349801, 106.42919463649463, 106.60493583168845, 105.98414919638157, 103.6831869049231, 103.75345423652524, 102.22135426776546, 102.13628943823835, 101.83401289687525, 100.9267924570407, 100.30584853452622, 100.0];
		let example_historic_ohlc = mock_p_to_ohlc(&example_historic_p, 4);
		example_historic_ohlc
	}
}

impl Protocol for SarWrapper {
	type Params = Sar;

	/// Requested orders are being sent over the mspc with uuid of the protocol on each batch, as we want to replace the previous requested batch if any.
	fn attach(&self, tx_orders: mpsc::Sender<ProtocolOrders>, position_spec: &PositionSpec) -> Result<()> {
		let symbol = Symbol {
			base: position_spec.asset.clone(),
			quote: "USDT".to_owned(),
			market: Market::BinanceFutures,
		};
		let tf = { 
			self.params.lock().unwrap().timeframe
		};
		let address = format!("wss://fstream.binance.com/ws/{}@kline_{tf}", symbol.to_string().to_lowercase());

		let params = self.params.clone();
		let position_spec = position_spec.clone();

		let order_mask: Vec<Option<ConceptualOrderPercents>> = vec![None; 1];
		//TODO!: rewrite
		macro_rules! update_orders {
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

		let (tx, mut rx) = tokio::sync::mpsc::channel::<Ohlc>(256);
		let address_clone = address.clone();
		let data_source_clone = self.data_source;
		tokio::spawn(async move {
			data_source_clone.listen(&address_clone, tx).await.unwrap();
		});

		tokio::spawn(async move {
			let position_side = position_spec.side;
			let mut sar = SarIndicator::init(&data_source_clone, params.clone(), &symbol);

			while let Some(ohlc) = rx.recv().await {
				//TODO!!!!!!: only update sar if the candle is over. Same for trade updates. (the only thing we want to be real-time is flipping of the indie, which is already captured by the placed stop_market)
				//TODO!!!!!!!!!: sub with SAR logic
				let prev_sar = sar;
				sar.step(ohlc);

				if sar.sar != prev_sar.sar {
					todo!();
					//update_orders!(sar, side);
				}
			}
		});

		Ok(())
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
	params: (f64, f64, f64),
}
impl SarIndicator {
	//TODO!!!!!!!!!: \
	fn init(data_source: &DataSource, protocol_params: Arc<Mutex<Sar>>, symbol: &Symbol) -> Self {
		//TODO!!!!!!: Request 100 bars back on the chosen tf, to init sar correctly
		// This should actually be passed to the function, otherwise testing is impossible
		let mut extreme_point = 44.0; //dbg
		let mut sar = 44.0; //dbg (normally should init at `price`, then sim for 100 bars to be up-to-date here)
		let tf = { 
			protocol_params.lock().unwrap().timeframe
		};
		let historic_klines_ohlc = data_source.historic_klines_ohlc(&symbol.to_string(), tf, 100).unwrap();
		//- init the correct sar value by running on 100 points of history



		let (start, increment, max) = {
			let params_lock = protocol_params.lock().unwrap();
			(params_lock.start.0, params_lock.increment.0, params_lock.maximum.0)
		};
		let mut acceleration_factor = start;

		todo!()
	}
	fn step(&mut self, ohlc: Ohlc) {
		let (start, increment, max) = self.params;
		let is_uptrend = self.sar < ohlc.low;

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
				self.acceleration_factor = (self.acceleration_factor + increment).min(max);
			}
		} else if ohlc.low < self.extreme_point {
			self.extreme_point = ohlc.low;
			self.acceleration_factor = (self.acceleration_factor + increment).min(max);
		}

		// Check for trend reversal
		if (is_uptrend && ohlc.low < self.sar) || (!is_uptrend && ohlc.high > self.sar) {
			self.sar = self.extreme_point;
			self.extreme_point = if is_uptrend { ohlc.low } else { ohlc.high };
			self.acceleration_factor = start;
		}
	}
}

impl SarWrapper {
	pub fn set_data_source(mut self, new_data_source: DataSource) -> Self {
		self.data_source = new_data_source;
		self
	}
}

#[derive(Debug, Clone, CompactFormat, derive_new::new, Copy)]
pub struct Sar {
	start: Percent,
	increment: Percent,
	maximum: Percent,
	timeframe: Timeframe,
}

//? should I move this higher up? Could compile times, and standardize the check function.
#[cfg(test)]
mod tests {
	use super::*;

	#[tokio::test]
	async fn test_sar_indicator() {
		let ts = SarWrapper::from_str("sar:s0.07:i0.02:m0.15:t1m")
			.unwrap()
			.set_data_source(DataSource::Test(TestDataSource));
		
		let mut sar = SarIndicator::init(&ts.data_source, ts.params.clone(), &Symbol::new("BTC", "USDT", Market::BinanceFutures));

		let datasource_clone = ts.data_source.clone();
		let (tx, mut rx) = tokio::sync::mpsc::channel(32);
		tokio::spawn(async move {
			datasource_clone.listen("", tx).await.unwrap();
		});

		let mut received_data = Vec::new();
		while let Some(data) = rx.recv().await {
			received_data.push(data);
		}

		let mut recorded_indicator_values = Vec::new();
		for ohlc in received_data {
			sar.step(ohlc);
			recorded_indicator_values.push(sar.sar);
		}

		insta::assert_debug_snapshot!(recorded_indicator_values, @r###"isaendtaerd"###);
	}

	//? Could I move part of this operation inside the check function, following https://matklad.github.io/2021/05/31/how-to-test.html ?
	#[tokio::test]
	async fn test_sar_orders() {
		let ts = SarWrapper::from_str("sar:s0.07:i0.02:m0.15:t1m")
			.unwrap()
			.set_data_source(DataSource::Test(TestDataSource));

		let position_spec = PositionSpec::new("BTC".to_owned(), Side::Buy, 1.0);

		let (tx, mut rx) = tokio::sync::mpsc::channel(32);
		tokio::spawn(async move {
			ts.attach(tx, &position_spec).unwrap();
		});

		let mut received_data = Vec::new();
		while let Some(data) = rx.recv().await {
			received_data.push(data);
		}

		let received_data_inner_orders = received_data.iter().map(|x| x.__orders.clone()).collect::<Vec<_>>();

		insta::assert_json_snapshot!(
			received_data_inner_orders,
			@"[]",
		);
	}
}

