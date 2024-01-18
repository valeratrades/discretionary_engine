use crate::exchange_interactions::Market;
use crate::follow::Protocol;
use std::collections::HashMap;
use v_utils::trades::Side;

pub struct Positions {
	positions: Vec<Position>,
	//TODO!!!: implement Symbol struct in the v_utils.
	unaccounted: HashMap<String, f32>,
	//percent_accounted: f32,
}

#[derive(Clone, Debug)]
pub struct Position {
	market: Market,
	side: Side,
	qty_notional: f32,
	qty_usdt: f32,
	follow: Option<Protocol>,
}
