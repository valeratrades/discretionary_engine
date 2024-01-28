pub mod binance;
use crate::protocols::{ProtocolWrapper, Protocols};
use binance::OrderStatus;
use crate::config::Config;
use crate::positions::{Position, Positions};
use anyhow::Result;
use std::collections::HashMap;
use std::sync::atomic::Ordering;
use tokio::time::Duration;
use url::Url;
use chrono::Utc;
use v_utils::trades::Side;

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
pub async fn open_futures_position(config: Config, positions: Positions, symbol: String, side: Side, usdt_quantity: f64, protocols: Protocols) -> Result<()> {
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

#[derive(Clone, Debug)]
pub enum Order {
	BinanceOrder(binance::FuturesOrder),
}





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


