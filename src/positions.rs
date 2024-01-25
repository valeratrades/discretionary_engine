use crate::exchange_interactions::Market;
use crate::protocols::Protocol;
use atomic_float::AtomicF64;
use chrono::{DateTime, Utc};
use std::collections::HashMap;
use v_utils::trades::Side;

/// for now a struct, that is initialized on start, but later on all the positions will be accounted for in a .lock file.
pub struct Positions {
	positions: Vec<Position>,
	//TODO!!!: implement Symbol struct in the v_utils.
	unaccounted: HashMap<String, f64>,
	//percent_accounted: f64,
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
	pub follow: Vec<Protocol>,
	pub timestamp: DateTime<Utc>,
	//? add `realised_usdt` field?
}

impl Position {
	//TODO!!!: implement the darn Symbol
	pub fn new(market: Market, side: Side, symbol: String, target_qty_usdt: f64, timestamp: DateTime<Utc>) -> Self {
		Self {
			market,
			side,
			symbol,
			qty_notional: AtomicF64::new(0.0),
			qty_usdt: AtomicF64::new(0.0),
			target_qty_usdt: AtomicF64::from(target_qty_usdt),
			follow: Vec::new(),
			timestamp,
		}
	}
}
