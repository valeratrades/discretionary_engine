use std::sync::Arc;

use eyre::Result;

use super::Market;
use crate::{config::AppConfig, exchange_apis::binance};


/// [Exchange] itsalf is passed around as Arc<Self>, RwLock is only present at the level of individual exchanges, as to not lock it all at once when writing.
#[derive(Clone, Debug, Default, derive_new::new, Copy)]
pub struct Exchanges {}

impl Exchanges {
	pub async fn compile_total_balance(s: Arc<Self>, config: AppConfig) -> Result<f64> {
		let read_key = config.binance.read_key.clone();
		let read_secret = config.binance.read_secret.clone();

		let handlers = vec![
			binance::get_balance(read_key.clone(), read_secret.clone(), Market::BinanceFutures),
			binance::get_balance(read_key.clone(), read_secret.clone(), Market::BinanceSpot),
		];

		let mut total_balance = 0.0;
		for handler in handlers {
			let balance = handler.await?;
			total_balance += balance;
		}
		Ok(total_balance)
	}
}
