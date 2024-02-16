use crate::api::Symbol;
use v_utils::trades::{Side, Timeframe, Timestamp};

//TODO!: Move order_types to v_utils when stable

/// Generics for defining order types and their whereabouts. Specific `size` and `market` are to be added in the api-specific part of the implementation.
pub enum OrderType {
	Market(Market),
	Limit(Limit),
	StopMarket(StopMarket),
}

//ref: https://binance-docs.github.io/apidocs/futures/en/#new-order-trade

pub struct Market {
	pub symbol: Symbol,
	pub side: Side,
	pub size: f64,
}

pub struct StopMarket {
	pub symbol: Symbol,
	pub side: Side,
	pub size: f64,
	pub price: f64,
}

pub struct Limit {
	pub symbol: Symbol,
	pub side: Side,
	pub size: f64,
	pub price: f64,
}

//=============================================================================
// Apparently, this is how we're pushing orders up to later be chosen and assigned sizes
//=============================================================================

pub struct MarketWhere {
	pub symbol: Symbol,
	pub side: Side,
}

pub struct StopMarketWhere {
	pub symbol: Symbol,
	pub side: Side,
	pub price: f64,
}

pub struct LimitWhere {
	pub symbol: Symbol,
	pub side: Side,
	pub price: f64,
}
