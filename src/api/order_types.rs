use crate::api::Symbol;
use anyhow::Result;
use v_utils::trades::Side;

//TODO!: Move order_types to v_utils when stable

//TODO!!: automatically derive the Protocol Order types (by substituting `size` with `percent_size`, then auto-implementation of the conversion. Looks like I'm making a `discretionary_engine_macros` crate specifically to for this.

pub trait OrderStuff {
	fn is_stop_order(&self) -> Option<bool>;
	fn price(&self) -> Result<f64>;
}

/// Generics for defining order types and their whereabouts. Specific `size` and `market` are to be added in the api-specific part of the implementation.
#[derive(Debug, Clone, PartialEq)]
pub enum Order {
	Market(Market),
	Limit(Limit),
	StopMarket(StopMarket),
}
impl OrderStuff for Order {
	fn is_stop_order(&self) -> Option<bool> {
		match self {
			Order::Market(m) => m.is_stop_order(),
			Order::Limit(l) => l.is_stop_order(),
			Order::StopMarket(s) => s.is_stop_order(),
		}
	}
	fn price(&self) -> Result<f64> {
		match self {
			Order::Market(m) => m.price(),
			Order::Limit(l) => l.price(),
			Order::StopMarket(s) => s.price(),
		}
	}
}


#[derive(Debug, Clone, PartialEq)]
pub struct Market {
	pub owner: String,
	pub symbol: Symbol,
	pub side: Side,
	pub size_notional: f64,
}
impl OrderStuff for Market {
	fn is_stop_order(&self) -> Option<bool> {
		None
	}
	fn price(&self) -> Result<f64> {
		anyhow::bail!("Market orders don't have a price")
	}
}
#[derive(Debug, Clone, PartialEq)]
pub struct StopMarket {
	pub owner: String,
	pub symbol: Symbol,
	pub side: Side,
	pub price: f64,
	pub size_notional: f64,
}
impl OrderStuff for StopMarket {
	fn is_stop_order(&self) -> Option<bool> {
		Some(true)
	}
	fn price(&self) -> Result<f64> {
		Ok(self.price)
	}
}
#[derive(Debug, Clone, PartialEq)]
pub struct Limit {
	pub owner: String,
	pub symbol: Symbol,
	pub side: Side,
	pub price: f64,
	pub size_notional: f64,
}
impl OrderStuff for Limit {
	fn is_stop_order(&self) -> Option<bool> {
		Some(false)
	}
	fn price(&self) -> Result<f64> {
		Ok(self.price)
	}
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
			size_notional: total_controled_size * self.percent_size,
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
			size_notional: total_controled_size * self.percent_size,
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
			size_notional: total_controled_size * self.percent_size,
			owner,
		}
	}
}
