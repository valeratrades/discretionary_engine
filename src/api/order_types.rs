use crate::api::Symbol;
use anyhow::Result;
use derive_new::new;
use uuid::Uuid;
use v_utils::trades::Side;

//TODO!: Move order_types to v_utils when stable

//TODO!!: automatically derive the Protocol Order types (by substituting `size` with `percent_size`, then auto-implementation of the conversion. Looks like I'm making a `discretionary_engine_macros` crate specifically to for this.

#[derive(Clone, Debug, PartialEq, new, Default)]
pub struct Order {
	pub id: Uuid,
	pub order_type: OrderType,
	pub symbol: Symbol,
	pub side: Side,
	pub qty_notional: f64,
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
impl Default for OrderType {
	fn default() -> Self {
		OrderType::Market
	}
}

#[derive(Clone, Debug, PartialEq, new)]
pub struct StopMarketOrder {
	pub price: f64,
}

//=============================================================================
// Conceptual Orders
//=============================================================================

#[derive(Debug, Hash, Clone, PartialEq, new)]
pub struct ProtocolOrderId {
	pub produced_by: String,
	pub uuid: Uuid,
}

#[derive(Debug, Clone, PartialEq, new)]
pub struct ConceptualOrder {
	pub id: ProtocolOrderId,
	pub order_type: ConceptualOrderType,
	pub symbol: Symbol,
	pub side: Side,
	pub qty_notional: f64,
}

impl ConceptualOrder {
	pub fn price(&self) -> Result<f64> {
		match &self.order_type {
			ConceptualOrderType::Market(_) => anyhow::bail!("Market orders don't have a price"),
			ConceptualOrderType::Limit(l) => Ok(l.price),
			ConceptualOrderType::StopMarket(s) => Ok(s.price),
		}
	}
}

/// Generics for defining order types and their whereabouts. Details of execution do not concern us here. We are only trying to specify what we are trying to capture.
#[derive(Debug, Clone, PartialEq)]
pub enum ConceptualOrderType {
	Market(ConceptualMarket),
	Limit(ConceptualLimit),
	StopMarket(ConceptualStopMarket),
}

/// Will be executed via above-the-price limits most of the time to prevent excessive slippages.
#[derive(Debug, Clone, PartialEq, new)]
pub struct ConceptualMarket {
	/// 1.0 will be translated into an actual Market order. Others, most of the time, will be expressed via limit orders.
	pub maximum_slippage_percent: f64,
}

#[derive(Debug, Clone, PartialEq, new)]
pub struct ConceptualStopMarket {
	/// 1.0 will be translated into an actual Market order. Others, most of the time, will be expressed via limit orders.
	pub maximum_slippage_percent: f64,
	pub price: f64,
}

#[derive(Debug, Clone, PartialEq, new)]
pub struct ConceptualLimit {
	pub price: f64,
	pub limit_only: bool,
}

#[derive(Debug, Clone, PartialEq, new)]
pub struct ConceptualOrderPercents {
	pub order_type: ConceptualOrderType,
	pub symbol: Symbol,
	pub side: Side,
	pub qty_percent_of_controlled: f64,
}
impl ConceptualOrderPercents {
	pub fn to_exact(self, total_controled_size: f64, produced_by: String, uuid: Uuid) -> ConceptualOrder {
		ConceptualOrder {
			id: ProtocolOrderId::new(produced_by, uuid),
			order_type: self.order_type,
			symbol: self.symbol,
			side: self.side,
			qty_notional: total_controled_size * self.qty_percent_of_controlled,
		}
	}
}
