use std::sync::atomic::Ordering;

use crate::binance_api;
use crate::binance_api::OrderStatus;
use crate::config::Config;
use crate::positions::Position;
use anyhow::Result;
use tokio::time::Duration;
use url::Url;
use v_utils::trades::Side;

pub async fn compile_total_balance(config: Config) -> Result<f64> {
	let read_key = config.binance.read_key.clone();
	let read_secret = config.binance.read_secret.clone();

	let mut handlers = Vec::new();
	handlers.push(binance_api::get_balance(read_key.clone(), read_secret.clone(), Market::BinanceFutures));
	handlers.push(binance_api::get_balance(read_key.clone(), read_secret.clone(), Market::BinanceSpot));

	let mut total_balance = 0.0;
	for handler in handlers {
		let balance = handler.await?;
		total_balance += balance;
	}
	Ok(total_balance)
}

//? Should I make this return new total postion size?
pub async fn open_futures_position(config: Config, symbol: String, side: Side, usdt_quantity: f64) -> Result<()> {
	let full_key = config.binance.full_key.clone();
	let full_secret = config.binance.full_secret.clone();
	let position = Position::new(Market::BinanceFutures, side, symbol.clone(), usdt_quantity, chrono::Utc::now());

	let current_price_handler = binance_api::futures_price(symbol.clone());
	let quantity_percision_handler = binance_api::futures_quantity_precision(symbol.clone());
	let current_price = current_price_handler.await?;
	let quantity_precision: usize = quantity_percision_handler.await?;

	let coin_quantity = usdt_quantity / current_price;
	let factor = 10_f64.powi(quantity_precision as i32);
	let coin_quantity_adjusted = (coin_quantity * factor).round() / factor;

	let order_id = binance_api::post_futures_order(
		full_key.clone(),
		full_secret.clone(),
		binance_api::OrderType::Market,
		symbol.clone(),
		side.clone(),
		coin_quantity_adjusted,
	)
	.await?;
	loop {
		let order = binance_api::poll_futures_order(full_key.clone(), full_secret.clone(), order_id.clone(), symbol.clone()).await?;
		if order.status == OrderStatus::Filled {
			dbg!(&order);
			let order_notional = order.origQty.parse::<f64>()?;
			let order_usdt = order.avgPrice.unwrap().parse::<f64>()? * order_notional;
			//NB: currently assuming there is nothing else to the position.
			position.qty_notional.store(order_notional, Ordering::SeqCst);
			position.qty_usdt.store(order_usdt, Ordering::SeqCst);

			break;
		}
		tokio::time::sleep(Duration::from_secs(1)).await;
	}

	dbg!(&position);
	Ok(())
}

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
