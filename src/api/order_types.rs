use crate::api::Symbol;
use anyhow::Result;
use v_utils::trades::Side;

//TODO!: Move order_types to v_utils when stable

//TODO!!: automatically derive the Protocol Order types (by substituting `size` with `percent_size`, then auto-implementation of the conversion. Looks like I'm making a `discretionary_engine_macros` crate specifically to for this.

// I rarely want to post actual market orders, but most of the time it's going to be above-the-ask limit. Thus I need to somehow mark `effectively_market` orders.
// probably will add a mark that alters a function returning the order_bucket_type of initialized order. No clue how though.
#[derive(Debug, Clone, PartialEq)]
pub enum OrderBucketType {
	Market,
	Normal,
	Stop,
}

/// Generics for defining order types and their whereabouts. Specific `size` and `market` are to be added in the api-specific part of the implementation.
#[derive(Debug, Clone, PartialEq)]
pub enum Order {
	Market(Market),
	Limit(Limit),
	StopMarket(StopMarket),
}
impl Order {
	pub fn order_bucket_type(&self) -> OrderBucketType {
		match self {
			Order::Market(_) => OrderBucketType::Market,
			Order::Limit(_) => OrderBucketType::Normal,
			Order::StopMarket(_) => OrderBucketType::Stop,
		}
	}

	pub fn price(&self) -> Result<f64> {
		match self {
			Order::Market(_) => anyhow::bail!("Market orders don't have a price"),
			Order::Limit(l) => Ok(l.price),
			Order::StopMarket(s) => Ok(s.price),
		}
	}

	pub fn notional(&self) -> f64 {
		match self {
			Order::Market(m) => m.qty_notional,
			Order::Limit(l) => l.qty_notional,
			Order::StopMarket(s) => s.qty_notional,
		}
	}

	pub fn cut_size(&mut self, new: f64) {
		match self {
			Order::Market(m) => m.qty_notional = new,
			Order::Limit(l) => l.qty_notional = new,
			Order::StopMarket(s) => s.qty_notional = new,
		}
	}
}

#[derive(Debug, Clone, PartialEq)]
pub struct Market {
	pub owner: String,
	pub symbol: Symbol,
	pub side: Side,
	pub qty_notional: f64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct StopMarket {
	pub owner: String,
	pub symbol: Symbol,
	pub side: Side,
	pub price: f64,
	pub qty_notional: f64,
}
#[derive(Debug, Clone, PartialEq)]
pub struct Limit {
	pub owner: String,
	pub symbol: Symbol,
	pub side: Side,
	pub price: f64,
	pub qty_notional: f64,
}

//=============================================================================
// Apparently, this is how we're pushing orders up to later be chosen and assigned sizes
//=============================================================================

#[derive(Debug, Clone, PartialEq)]
pub enum OrderP {
	Market(MarketP),
	Limit(LimitP),
	StopMarket(StopMarketP),
}

impl OrderP {
	pub fn to_exact(self, total_controled_size: f64, owner: String) -> Order {
		match self {
			OrderP::Market(m) => Order::Market(m.to_exact(total_controled_size, owner)),
			OrderP::Limit(l) => Order::Limit(l.to_exact(total_controled_size, owner)),
			OrderP::StopMarket(s) => Order::StopMarket(s.to_exact(total_controled_size, owner)),
		}
	}
}

#[derive(Debug, Clone, PartialEq)]
pub struct MarketP {
	pub symbol: Symbol,
	pub side: Side,
	pub percent_size: f64,
}

impl MarketP {
	pub fn to_exact(self, total_controled_size: f64, owner: String) -> Market {
		Market {
			symbol: self.symbol,
			side: self.side,
			qty_notional: total_controled_size * self.percent_size,
			owner,
		}
	}
}

#[derive(Debug, Clone, PartialEq)]
pub struct StopMarketP {
	pub symbol: Symbol,
	pub side: Side,
	pub price: f64,
	pub percent_size: f64,
}

impl StopMarketP {
	pub fn to_exact(self, total_controled_size: f64, owner: String) -> StopMarket {
		StopMarket {
			symbol: self.symbol,
			side: self.side,
			price: self.price,
			qty_notional: total_controled_size * self.percent_size,
			owner,
		}
	}
}

#[derive(Debug, Clone, PartialEq)]
pub struct LimitP {
	pub symbol: Symbol,
	pub side: Side,
	pub price: f64,
	pub percent_size: f64,
}

impl LimitP {
	pub fn to_exact(self, total_controled_size: f64, owner: String) -> Limit {
		Limit {
			symbol: self.symbol,
			side: self.side,
			price: self.price,
			qty_notional: total_controled_size * self.percent_size,
			owner,
		}
	}
}
