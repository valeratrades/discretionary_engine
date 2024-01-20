use crate::exchange_interactions::Market;
use crate::protocols::Protocol;
use atomic_float::AtomicF64;
use chrono::{DateTime, Utc};
use std::collections::HashMap;
use v_utils::trades::Side;

pub struct Positions {
	positions: Vec<Position>,
	//TODO!!!: implement Symbol struct in the v_utils.
	unaccounted: HashMap<String, f64>,
	//percent_accounted: f64,
}

#[derive(Debug)]
pub struct Position {
	pub market: Market,
	pub side: Side,
	pub qty_notional: AtomicF64,
	pub realised_qty_usdt: AtomicF64,
	pub target_qty_usdt: AtomicF64,
	// I now think it should be possible to slap multiple protocols on it. Say 1) tp+sl 2) trailing_stop
	// And then the protocols can have traits indicating their type. Like momentum, preset, fundamental, or what have you. Just need to figure out rules for their interaction amongst themselthes.
	pub follow: Vec<Protocol>,
	pub timestamp: DateTime<Utc>,
}
