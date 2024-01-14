use crate::binance_api;
use crate::config::Config;
use anyhow::Result;

pub async fn compile_total_balance(config: Config) -> Result<f32> {
	let mut handlers = Vec::new();
	handlers.push(binance_api::get_balance(
		config.binance.read_key.clone(),
		config.binance.read_secret.clone(),
		binance_api::Market::Futures,
	));
	//TODO!: same for spot and margin

	// let balance_bybit_futures = ...

	let mut total_balance = 0.0;
	for handler in handlers {
		let balance = handler.await?;
		total_balance += balance;
	}
	Ok(total_balance)
}
