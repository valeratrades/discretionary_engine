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
	market: Market,
	side: Side,
	qty_notional: AtomicF64,
	realised_qty_usdt: AtomicF64,
	target_qty_usdt: AtomicF64,
	// I now think it should be possible to slap multiple protocols on it. Say 1) tp+sl 2) trailing_stop
	// And then the protocols can have traits indicating their type. Like momentum, preset, fundamental, or what have you. Just need to figure out rules for their interaction amongst themselthes.
	follow: Vec<Protocol>,
	timestamp: DateTime<Utc>,
}
