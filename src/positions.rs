use crate::api::{get_positions, Market};
use crate::config::Config;
use crate::protocols::{Cache, Protocols};
use anyhow::Result;
use atomic_float::AtomicF64;
use chrono::{DateTime, Utc};
use std::collections::HashMap;
use std::sync::atomic::Ordering;
use std::sync::{Arc, Mutex};
use v_utils::trades::Side;

/// for now a struct, that is initialized on start, but later on all the positions will be accounted for in a .lock file.
#[derive(Debug)]
pub struct Positions {
	pub positions: Arc<Mutex<Vec<Position>>>,
	//TODO!!!: implement Symbol struct in the v_utils.
	pub difference_from_exchange: Arc<Mutex<HashMap<String, f64>>>,
	//percent_accounted: f64,
}
impl Positions {
	/// reads positions the engine is aware of from positions.lock file in the provided positions_dir in config.
	pub async fn read_from_file(config: &Config) -> Result<Self> {
		let lock_file = config.positions_dir.clone().join("positions.lock");
		//TODO!!!!!: actually implement serde for Positions, then read the file. Currently just creating a new object instead.
		let positions = Self {
			positions: Arc::new(Mutex::new(Vec::new())),
			difference_from_exchange: Arc::new(Mutex::new(HashMap::new())),
		};
		Ok(positions)
	}

	//TODO!: do sync, but for all open orders instead of positions.
	pub async fn sync(&self, config: Config) -> anyhow::Result<()> {
		let mut accounted_positions: HashMap<String, f64> = HashMap::new();
		self.positions.lock().unwrap().iter().for_each(|position| {
			let symbol = position.symbol.clone();
			let qty_notional = position.qty_notional.load(Ordering::SeqCst);
			accounted_positions.entry(symbol).and_modify(|e| *e += qty_notional).or_insert(qty_notional);
		});

		let mut exchange_positions = get_positions(&config).await?;

		for (symbol, qty_notional) in accounted_positions.iter() {
			dbg!(&qty_notional);
			exchange_positions
				.entry(symbol.clone())
				.and_modify(|e| *e += qty_notional)
				.or_insert(-qty_notional);
		}
		let mut difference_lock = self.difference_from_exchange.lock().unwrap();
		for (symbol, qty_notional) in exchange_positions.iter() {
			if *qty_notional != 0.0 {
				difference_lock.insert(symbol.clone(), *qty_notional);
			}
		}
		drop(difference_lock);
		dbg!(&self);
		Ok(())
	}
}

//TODO!!: define `execution` field, alongside the protocols, which then would also store all the details of the execution, as it was attempted and performed.
#[derive(Debug)]
pub struct Position {
	pub market: Market,
	pub side: Side,
	pub symbol: String,
	pub qty_notional: AtomicF64,
	pub qty_usdt: AtomicF64,
	pub target_qty_usdt: AtomicF64,
	// I now think it should be possible to slap multiple protocols on it. Say 1) tp+sl 2) trailing_stop
	// And then the protocols can have traits indicating their type. Like momentum, preset, fundamental, or what have you. Just need to figure out rules for their interaction amongst themselthes.
	pub protocols: Protocols,
	pub cache: Arc<Mutex<Cache>>,
	pub timestamp: DateTime<Utc>,
	//? add `realised_usdt` field?
}

impl Position {
	//TODO!!!: implement the darn Symbol
	pub fn new(market: Market, side: Side, symbol: String, target_qty_usdt: f64, protocols: Protocols, timestamp: DateTime<Utc>) -> Self {
		Self {
			market,
			side,
			symbol,
			qty_notional: AtomicF64::new(0.0),
			qty_usdt: AtomicF64::new(0.0),
			target_qty_usdt: AtomicF64::from(target_qty_usdt),
			protocols,
			cache: Arc::new(Mutex::new(Cache::new())),
			timestamp,
		}
	}
}
