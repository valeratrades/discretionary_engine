use std::sync::{Arc, RwLock};

use color_eyre::eyre::Result;
use tracing::instrument;

use super::{
	binance::BinanceExchange,
	order_types::ConceptualOrderType,
	Market,
};
use crate::{config::AppConfig, exchange_apis::binance};

/// [Exchange] itself is passed around as Arc<Self>, RwLock is only present at the level of individual exchanges, as to not lock it all at once when writing.
#[derive(Clone, Debug, Default)]
pub struct Exchanges {
	pub binance: Arc<RwLock<BinanceExchange>>,
}

impl Exchanges {
	pub async fn init(config_arc: Arc<AppConfig>) -> Result<Self> {
		let binance = BinanceExchange::init(config_arc.clone()).await?;
		Ok(Self {
			binance: Arc::new(RwLock::new(binance)),
		})
	}

	#[instrument(skip(_s, config))]
	pub async fn compile_total_balance(_s: Arc<Self>, config: Arc<AppConfig>) -> Result<f64> {
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

	//TODO!: update so that we don't concern ourselves with potential of differing base assets (here or lower down the callstack this starts, don't remember).
	/// Returns the absolute minimum trade quantity for (order_type, base_asset) pair, /*as min trade qty can depend on whever the order is market or not*/
	#[instrument(skip(_s))]
	pub fn compile_min_trade_qties(_s: Arc<Self>, base_asset: &str, ordertypes: &[ConceptualOrderType]) -> Vec<f64> {
		let mut min_notional_qties_accross_exchanges = Vec::with_capacity(ordertypes.len());
		for _ in 0..ordertypes.len() {
			min_notional_qties_accross_exchanges.push(f64::MAX);
		}

		let binance_min_notional_qties = {
			let binance_lock = _s.binance.read().unwrap();
			binance_lock.min_qties_batch(base_asset, ordertypes)
		};
		assert_eq!(binance_min_notional_qties.len(), ordertypes.len());
		assert_ne!(min_notional_qties_accross_exchanges.len(), 0);
		for (i, q) in binance_min_notional_qties.iter().enumerate() {
			if *q < min_notional_qties_accross_exchanges[i] {
				min_notional_qties_accross_exchanges[i] = *q;
			}
		}

		//- same for other exchanges

		min_notional_qties_accross_exchanges
	}
}
