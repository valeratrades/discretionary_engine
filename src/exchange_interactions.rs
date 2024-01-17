use crate::binance_api;
use crate::config::Config;
use anyhow::Result;
use v_utils::trades::Side;

pub async fn compile_total_balance(config: Config) -> Result<f32> {
	let read_key = config.binance.read_key.clone();
	let read_secret = config.binance.read_secret.clone();

	let mut handlers = Vec::new();
	handlers.push(binance_api::get_balance(read_key.clone(), read_secret.clone(), binance_api::Market::Futures));
	handlers.push(binance_api::get_balance(read_key.clone(), read_secret.clone(), binance_api::Market::Spot));

	let mut total_balance = 0.0;
	for handler in handlers {
		let balance = handler.await?;
		total_balance += balance;
	}
	Ok(total_balance)
}

//? Should I make this return new total postion size?
pub async fn open_futures_position(config: Config, symbol: String, side: Side, usdt_quantity: f32) -> Result<()> {
	let full_key = config.binance.full_key.clone();
	let full_secret = config.binance.full_secret.clone();

	let current_price_handler = binance_api::futures_price(symbol.clone());
	let quantity_percision_handler = binance_api::futures_quantity_precision(symbol.clone());
	let current_price = current_price_handler.await?;
	let quantity_precision: usize = quantity_percision_handler.await?;

	let coin_quantity = usdt_quantity / current_price;
	let factor = 10_f32.powi(quantity_precision as i32);
	let coin_quantity_adjusted = (coin_quantity * factor).round() / factor;

	binance_api::post_futures_trade(full_key, full_secret, binance_api::OrderType::Market, symbol, side, coin_quantity_adjusted).await?;
	Ok(())
}
