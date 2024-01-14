use crate::binance_api;
use crate::config::Config;
use anyhow::Result;

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
