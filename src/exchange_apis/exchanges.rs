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

	/// Currently a dummy function with sole role of establishing architecture that would work with multi-symbol returns from protocols. Right now just hardcodes the answer.
	pub fn symbol_prices_batch(_s: Arc<Self>, symbols: &[Symbol]) -> Vec<f64> {
		let diff_symbols: std::collections::HashSet<Symbol> = symbols.iter().cloned().collect();
		assert_eq!(diff_symbols.len(), 1, "Different symbols are not yet supported");

		symbols.iter().map(|_| 1.0).collect()
	}

	pub fn compile_min_trade_qties(_s: Arc<Self>, orders_on_symbols: &[ConceptualOrderPercents]) -> Vec<f64> {
		let mut min_notional_qty_accross_exchanges = Vec::with_capacity(orders_on_symbols.len());
		for q in min_notional_qty_accross_exchanges.iter_mut() {
			*q = f64::MAX;
		}

		let binances_qties_batch_payload = orders_on_symbols.iter().map(|o| (o.symbol.base.clone(), o.order_type)).collect::<Vec<_>>();
		let binance_min_notional_qties = {
			let binance_lock = _s.binance.read().unwrap();
			binance_lock.min_qties_batch(&binances_qties_batch_payload)
		};
		assert_eq!(binance_min_notional_qties.len(), orders_on_symbols.len());
		for (i, q) in binance_min_notional_qties.iter().enumerate() {
			if *q < min_notional_qty_accross_exchanges[i] {
				min_notional_qty_accross_exchanges[i] = *q;
			}
		}

		//- same for other exchanges

		min_notional_qty_accross_exchanges
	}
}
