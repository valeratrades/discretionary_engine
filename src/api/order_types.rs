use crate::api::Symbol;
use anyhow::Result;
use uuid::Uuid;
use v_utils::trades::Side;

//TODO!: Move order_types to v_utils when stable

//TODO!!: automatically derive the Protocol Order types (by substituting `size` with `percent_size`, then auto-implementation of the conversion. Looks like I'm making a `discretionary_engine_macros` crate specifically to for this.

#[derive(Clone, Debug, PartialEq)]
pub struct Order {
	pub order_type: OrderType,
	pub id: Uuid,
	pub symbol: Symbol,
	pub side: Side,
	pub qty_notional: f64,
}
impl Order {
	pub fn new(order_type: OrderType, id: Uuid, symbol: Symbol, side: Side, qty_notional: f64) -> Self {
		Self {
			order_type,
			id,
			symbol,
			side,
			qty_notional,
		}
	}
}

///NB: id of all orders must match uuid field of parent ConceptualOrder if any
#[derive(Clone, Debug, PartialEq)]
pub enum OrderType {
	Market,
	StopMarket(StopMarketOrder),
	//Limit(LimitOrder),
	//StopLimit(StopLimitOrder),
	//TrailingStop(TrailingStopOrder),
	//TWAP(TWAPOrder),
	//Reverse(ReverseOrder),
	//ScaledOrder(ScaledOrder),
	//StopMarket(StopMarketOrder),
}

#[derive(Clone, Debug, PartialEq)]
pub struct StopMarketOrder {
	pub price: f64,
}
impl StopMarketOrder {
	pub fn new(price: f64) -> Self {
		Self { price }
	}
}

//=============================================================================
// Conceptual Orders
//=============================================================================

#[derive(Debug, Hash, Clone, PartialEq)]
pub struct ProtocolOrderId {
	pub produced_by: String,
	pub uuid: Uuid,
}
impl ProtocolOrderId {
	pub fn new(produced_by: String, uuid: Uuid) -> Self {
		Self { produced_by, uuid }
	}
}

/// Generics for defining order types and their whereabouts. Details of execution do not concern us here. We are only trying to specify what we are trying to capture.
#[derive(Debug, Clone, PartialEq)]
pub enum ConceptualOrder {
	Market(ConceptualMarket),
	Limit(ConceptualLimit),
	StopMarket(ConceptualStopMarket),
}
impl ConceptualOrder {
	pub fn price(&self) -> Result<f64> {
		match self {
			ConceptualOrder::Market(_) => anyhow::bail!("Market orders don't have a price"),
			ConceptualOrder::Limit(l) => Ok(l.price),
			ConceptualOrder::StopMarket(s) => Ok(s.price),
		}
	}

	pub fn notional(&self) -> f64 {
		match self {
			ConceptualOrder::Market(m) => m.qty_notional,
			ConceptualOrder::Limit(l) => l.qty_notional,
			ConceptualOrder::StopMarket(s) => s.qty_notional,
		}
	}

	pub fn cut_size(&mut self, new: f64) {
		match self {
			ConceptualOrder::Market(m) => m.qty_notional = new,
			ConceptualOrder::Limit(l) => l.qty_notional = new,
			ConceptualOrder::StopMarket(s) => s.qty_notional = new,
		}
	}
}

/// Will be executed via above-the-price limits most of the time to prevent excessive slippages.
#[derive(Debug, Clone, PartialEq)]
pub struct ConceptualMarket {
	pub id: ProtocolOrderId,
	/// 1.0 will be translated into an actual Market order. Others, most of the time, will be expressed via limit orders.
	pub maximum_slippage_percent: f64,
	pub symbol: Symbol,
	pub side: Side,
	pub qty_notional: f64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ConceptualStopMarket {
	pub id: ProtocolOrderId,
	/// 1.0 will be translated into an actual Market order. Others, most of the time, will be expressed via limit orders.
	pub maximum_slippage_percent: f64,
	pub symbol: Symbol,
	pub side: Side,
	pub price: f64,
	pub qty_notional: f64,
}
#[derive(Debug, Clone, PartialEq)]
pub struct ConceptualLimit {
	pub id: ProtocolOrderId,
	pub symbol: Symbol,
	pub side: Side,
	pub price: f64,
	pub qty_notional: f64,
	pub limit_only: bool,
}

//=============================================================================
// Apparently, this is how we're pushing orders up to later be chosen and assigned sizes
//=============================================================================

#[derive(Debug, Clone, PartialEq)]
pub enum ConceptualOrderPercents {
	Market(ConceptualMarketPercents),
	Limit(ConceptualLimitPercents),
	StopMarket(ConceptualStopMarketPercents),
}

impl ConceptualOrderPercents {
	pub fn to_exact(self, total_controled_size: f64, produced_by: String, uuid: Uuid) -> ConceptualOrder {
		match self {
			ConceptualOrderPercents::Market(m) => ConceptualOrder::Market(m.to_exact(total_controled_size, produced_by, uuid)),
			ConceptualOrderPercents::Limit(l) => ConceptualOrder::Limit(l.to_exact(total_controled_size, produced_by, uuid)),
			ConceptualOrderPercents::StopMarket(s) => ConceptualOrder::StopMarket(s.to_exact(total_controled_size, produced_by, uuid)),
		}
	}
}

#[derive(Debug, Clone, PartialEq)]
pub struct ConceptualMarketPercents {
	pub maximum_slippage_percent: f64,
	pub symbol: Symbol,
	pub side: Side,
	pub percent_size: f64,
}

impl ConceptualMarketPercents {
	pub fn to_exact(self, total_controled_size: f64, produced_by: String, uuid: Uuid) -> ConceptualMarket {
		ConceptualMarket {
			id: ProtocolOrderId::new(produced_by, uuid),
			maximum_slippage_percent: self.maximum_slippage_percent,
			symbol: self.symbol,
			side: self.side,
			qty_notional: total_controled_size * self.percent_size,
		}
	}
}

#[derive(Debug, Clone, PartialEq)]
pub struct ConceptualStopMarketPercents {
	pub maximum_slippage_percent: f64,
	pub symbol: Symbol,
	pub side: Side,
	pub price: f64,
	pub percent_size: f64,
}

impl ConceptualStopMarketPercents {
	pub fn to_exact(self, total_controled_size: f64, produced_by: String, uuid: Uuid) -> ConceptualStopMarket {
		ConceptualStopMarket {
			id: ProtocolOrderId::new(produced_by, uuid),
			maximum_slippage_percent: self.maximum_slippage_percent,
			symbol: self.symbol,
			side: self.side,
			price: self.price,
			qty_notional: total_controled_size * self.percent_size,
		}
	}
}

#[derive(Debug, Clone, PartialEq)]
pub struct ConceptualLimitPercents {
	pub symbol: Symbol,
	pub side: Side,
	pub price: f64,
	pub percent_size: f64,
	pub limit_only: bool,
}

impl ConceptualLimitPercents {
	pub fn to_exact(self, total_controled_size: f64, produced_by: String, uuid: Uuid) -> ConceptualLimit {
		ConceptualLimit {
			id: ProtocolOrderId::new(produced_by, uuid),
			side: self.side,
			symbol: self.symbol,
			price: self.price,
			qty_notional: total_controled_size * self.percent_size,
			limit_only: self.limit_only,
		}
	}
}
