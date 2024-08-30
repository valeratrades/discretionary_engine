use std::sync::{Arc, RwLock};

use eyre::Result;

use super::{binance::BinanceExchange, order_types::ConceptualOrderPercents, Market, Symbol};
use crate::{config::AppConfig, exchange_apis::binance};

/// [Exchange] itself is passed around as Arc<Self>, RwLock is only present at the level of individual exchanges, as to not lock it all at once when writing.
#[derive(Clone, Debug, Default, derive_new::new)]
pub struct Exchanges {
	pub binance: Arc<RwLock<BinanceExchange>>,
}

impl Exchanges {
	pub async fn compile_total_balance(_s: Arc<Self>, config: AppConfig) -> Result<f64> {
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

	pub fn compile_min_trade_qties(_s: Arc<Self>, orders_on_symbols: Vec<ConceptualOrderPercents>) -> Vec<f64> {
		todo!()
	}
}
