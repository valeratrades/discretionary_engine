pub mod binance;
use crate::config::Config;
use crate::positions::{Position, Positions};
use crate::protocols::Klines;
use crate::protocols::Protocols;
use anyhow::Result;
use binance::OrderStatus;
use chrono::Utc;
use std::collections::HashMap;
use std::sync::atomic::Ordering;
use tokio::time::Duration;
use url::Url;
use v_utils::trades::{Side, Timeframe};

pub async fn compile_total_balance(config: Config) -> Result<f64> {
	let read_key = config.binance.read_key.clone();
	let read_secret = config.binance.read_secret.clone();

	let mut handlers = Vec::new();
	handlers.push(binance::get_balance(read_key.clone(), read_secret.clone(), Market::BinanceFutures));
	handlers.push(binance::get_balance(read_key.clone(), read_secret.clone(), Market::BinanceSpot));

	let mut total_balance = 0.0;
	for handler in handlers {
		let balance = handler.await?;
		total_balance += balance;
	}
	Ok(total_balance)
}

//? Should I make this return new total postion size?
pub async fn open_futures_position(
	config: Config,
	positions: Positions,
	symbol: String,
	side: Side,
	usdt_quantity: f64,
	protocols: Protocols,
) -> Result<()> {
	let full_key = config.binance.full_key.clone();
	let full_secret = config.binance.full_secret.clone();
	let position = Position::new(Market::BinanceFutures, side, symbol.clone(), usdt_quantity, protocols, Utc::now());

	let current_price_handler = binance::futures_price(symbol.clone());
	let quantity_percision_handler = binance::futures_quantity_precision(symbol.clone());
	let current_price = current_price_handler.await?;
	let quantity_precision: usize = quantity_percision_handler.await?;

	let coin_quantity = usdt_quantity / current_price;
	let factor = 10_f64.powi(quantity_precision as i32);
	let coin_quantity_adjusted = (coin_quantity * factor).round() / factor;

	let order_id = binance::post_futures_order(
		full_key.clone(),
		full_secret.clone(),
		binance::OrderType::Market,
		symbol.clone(),
		side.clone(),
		coin_quantity_adjusted,
	)
	.await?;
	//info!(target: "/tmp/discretionary_engine.lock", "placed order: {:?}", order_id);
	loop {
		let order = binance::poll_futures_order(full_key.clone(), full_secret.clone(), order_id.clone(), symbol.clone()).await?;
		if order.status == OrderStatus::Filled {
			let order_notional = order.origQty.parse::<f64>()?;
			let order_usdt = order.avgPrice.unwrap().parse::<f64>()? * order_notional;
			//NB: currently assuming there is nothing else to the position.
			position.qty_notional.store(order_notional, Ordering::SeqCst);
			position.qty_usdt.store(order_usdt, Ordering::SeqCst);

			//info!(target: "/tmp/discretionary_engine.lock", "Order filled; new position: {:?}", &position);
			positions.positions.lock().unwrap().push(position);
			break;
		}
		tokio::time::sleep(Duration::from_secs(1)).await;
	}
	positions.sync(config.clone()).await?;
	Ok(())
}

//TODO!: \
pub async fn get_positions(config: &Config) -> Result<HashMap<String, f64>> {
	binance::get_futures_positions(config.binance.full_key.clone(), config.binance.full_secret.clone()).await
}

/// Does the routing to the most liquid exchange automatically, don't have to specify. (not yet implemented).
pub async fn klines(klines_spec: KlinesSpec) -> Result<Klines> {
	binance::get_futures_klines(klines_spec.symbol, klines_spec.timeframe, klines_spec.limit).await
}
pub struct KlinesSpec {
	symbol: String,
	timeframe: Timeframe,
	limit: usize,
}
impl KlinesSpec {
	pub fn new(symbol: String, timeframe: Timeframe, limit: usize) -> Self {
		Self { symbol, timeframe, limit }
	}
}

//? Should I start passing down the entire Exchange objects?
#[allow(dead_code)]
#[derive(Clone, Debug)]
pub enum Market {
	BinanceFutures,
	BinanceSpot,
	BinanceMargin,
}
impl Market {
	pub fn get_base_url(&self) -> Url {
		match self {
			Market::BinanceFutures => Url::parse("https://fapi.binance.com/").unwrap(),
			Market::BinanceSpot => Url::parse("https://api.binance.com/").unwrap(),
			Market::BinanceMargin => Url::parse("https://api.binance.com/").unwrap(),
		}
	}
}

/// Order spec and human-interpretable unique name of the structure requesting it. Ex: `"trailing_stop"`
#[derive(Clone, Debug)]
pub enum OrderSpec {
	BinanceOrder((String, binance::FuturesOrder)),
}

// top level: [Protocol, Execution, RiskManager]
// second level: [TrailingStop, SAR, ...]

//pub struct Exchanges {
//	pub binance_futures: Exchange,
//}
//
//pub struct Exchange {
//	//pub general_info: GeneralInfo, // gonna be gradually expanded, based on the needs. The abstraction shall be generalized across exchanges.
//	//pub account_info: AccountInfo,
//	pub coins: HashMap<String, Coin>,
//}
//
//pub struct Coin {
//	pub price: f64, // is copied over from klines, for easier access
//	//pub kliens: Vec<Kline>,
//}
