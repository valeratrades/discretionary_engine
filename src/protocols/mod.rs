mod approaching_limit;
mod dummy_market;
mod sar;
mod trailing_stop;
use std::{collections::HashSet, str::FromStr, sync::Arc};

use approaching_limit::{ApproachingLimit, ApproachingLimitWrapper};
use dummy_market::DummyMarketWrapper;
use eyre::{bail, Result};
use sar::{Sar, SarWrapper};
use tokio::{sync::mpsc, task::JoinSet};
use trailing_stop::{TrailingStop, TrailingStopWrapper};
use uuid::Uuid;
use v_utils::trades::Side;

use crate::exchange_apis::{
	exchanges::Exchanges,
	order_types::{ConceptualOrder, ConceptualOrderPercents, ProtocolOrderId},
};

/// Used when determining sizing or the changes in it, in accordance to the current distribution of rm on types of algorithms.
/// Size is by default equally distributed amongst the protocols of the same `ProtocolType`, to total 100% for each type with at least one representative.
/// Note that total size is is 100% for both the stop and normal orders (because they are on the different sides of the price).
#[derive(Debug, PartialEq, Eq, Hash, Clone, Copy, derive_new::new)]
pub enum ProtocolType {
	Momentum,
	TP,
	SL,
	StopEntry,
}

pub trait ProtocolTrait {
	type Params;
	/// Requested orders are being sent over the mspc with uuid of the protocol on each batch, as we want to replace the previous requested batch if any.
	fn attach(&self, set: &mut JoinSet<Result<()>>, tx_orders: mpsc::Sender<ProtocolOrders>, asset: String, protocol_side: Side) -> Result<()>;
	fn update_params(&self, params: Self::Params) -> Result<()>;
	fn get_subtype(&self) -> ProtocolType;
}

// HACK: Protocol enum. Seems suboptimal {\{{
#[derive(Debug, Clone)]
pub enum Protocol {
	TrailingStop(TrailingStopWrapper),
	Sar(SarWrapper),
	ApproachingLimit(ApproachingLimitWrapper),
	DummyMarket(DummyMarketWrapper),
}
impl FromStr for Protocol {
	type Err = eyre::Report;

	fn from_str(spec: &str) -> Result<Self> {
		if let Ok(ts) = TrailingStopWrapper::from_str(spec) {
			Ok(Protocol::TrailingStop(ts))
		} else if let Ok(sar) = SarWrapper::from_str(spec) {
			Ok(Protocol::Sar(sar))
		} else if let Ok(al) = ApproachingLimitWrapper::from_str(spec) {
			Ok(Protocol::ApproachingLimit(al))
		} else if let Ok(dm) = DummyMarketWrapper::from_str(spec) {
			Ok(Protocol::DummyMarket(dm))
		} else {
			bail!("Could not convert string to any Protocol\nString: {spec}")
		}
	}
}
impl Protocol {
	pub fn attach(&self, position_set: &mut JoinSet<Result<()>>, tx_orders: mpsc::Sender<ProtocolOrders>, asset: String, protocol_side: Side) -> Result<()> {
		match self {
			Protocol::TrailingStop(ts) => ts.attach(position_set, tx_orders, asset, protocol_side),
			Protocol::Sar(sar) => sar.attach(position_set, tx_orders, asset, protocol_side),
			Protocol::ApproachingLimit(al) => al.attach(position_set, tx_orders, asset, protocol_side),
			Protocol::DummyMarket(dm) => dm.attach(position_set, tx_orders, asset, protocol_side),
		}
	}

	pub fn update_params(&self, params: ProtocolParams) -> Result<()> {
		match self {
			Protocol::TrailingStop(ts) => match params {
				ProtocolParams::TrailingStop(ts_params) => ts.update_params(ts_params),
				_ => Err(eyre::Report::msg("Mismatched params")),
			},
			Protocol::Sar(sar) => match params {
				ProtocolParams::Sar(sar_params) => sar.update_params(sar_params),
				_ => Err(eyre::Report::msg("Mismatched params")),
			},
			Protocol::ApproachingLimit(al) => match params {
				ProtocolParams::ApproachingLimit(al_params) => al.update_params(al_params),
				_ => Err(eyre::Report::msg("Mismatched params")),
			},
			Protocol::DummyMarket(_) => Ok(()),
		}
	}

	pub fn get_subtype(&self) -> ProtocolType {
		match self {
			Protocol::TrailingStop(ts) => ts.get_subtype(),
			Protocol::Sar(sar) => sar.get_subtype(),
			Protocol::ApproachingLimit(al) => al.get_subtype(),
			Protocol::DummyMarket(dm) => dm.get_subtype(),
		}
	}
}

#[derive(Debug, Clone, derive_new::new)]
pub enum ProtocolParams {
	TrailingStop(TrailingStop),
	Sar(Sar),
	ApproachingLimit(ApproachingLimit),
}
impl From<TrailingStop> for ProtocolParams {
	fn from(ts: TrailingStop) -> Self {
		ProtocolParams::TrailingStop(ts)
	}
}
impl From<Sar> for ProtocolParams {
	fn from(sar: Sar) -> Self {
		ProtocolParams::Sar(sar)
	}
}
impl From<ApproachingLimit> for ProtocolParams {
	fn from(al: ApproachingLimit) -> Self {
		ProtocolParams::ApproachingLimit(al)
	}
}
//,}}}

pub fn interpret_protocol_specs(protocol_specs: Vec<String>) -> Result<Vec<Protocol>> {
	let protocol_specs: Vec<String> = protocol_specs.into_iter().filter(|s| s != "").collect();
	if protocol_specs.len() == 0 {
		bail!("No protocols specified");
	}
	assert_eq!(protocol_specs.len(), protocol_specs.iter().collect::<HashSet<&String>>().len()); // protocol specs are later used as their IDs
	let mut protocols = Vec::new();
	for spec in protocol_specs {
		let protocol = Protocol::from_str(&spec)?;
		protocols.push(protocol);
	}
	Ok(protocols)
}

#[derive(Clone, Debug, Default, derive_new::new)]
pub struct ProtocolFill {
	pub id: ProtocolOrderId,
	pub qty: f64,
}

#[derive(Clone, Debug, Default, derive_new::new)]
pub struct ProtocolFills {
	pub key: Uuid,
	pub fills: Vec<ProtocolFill>,
}

/// Position's knowledge of the protocols in use.
#[derive(Clone, Debug, Default)]
pub struct ProtocolDynamicInfo {
	fills: Vec<f64>,
	protocol_orders: ProtocolOrders,
}
impl ProtocolDynamicInfo {
	pub fn new(protocol_orders: ProtocolOrders) -> Self {
		let fills = protocol_orders.empty_mask();
		Self { fills, protocol_orders }
	}

	pub fn update_fills(&mut self, fills: Vec<f64>) {
		self.fills = fills;
	}

	pub fn update_fill_at(&mut self, i: usize, fill: f64) {
		self.fills[i] += fill;
	}

	pub fn update_orders(&mut self, orders: ProtocolOrders) {
		self.protocol_orders = orders;
	}

	pub fn conceptual_orders(&self, parent_matching_subtype_n: usize, parent_notional: f64, exchanges: Arc<Exchanges>) -> Vec<ConceptualOrder<ProtocolOrderId>> {
		let orders = &self.protocol_orders.__orders;
		let size_multiplier = 1.0 / parent_matching_subtype_n as f64;
		let total_controlled_size = parent_notional * size_multiplier;
		let qties_payload = orders.iter().cloned().filter(|o| o.is_some()).map(|o| o.unwrap()).collect::<Vec<ConceptualOrderPercents>>();
		let min_trade_qties = Exchanges::compile_min_trade_qties(exchanges.clone(), qties_payload);

		let mut all_min_trade_qties = Vec::new();
		for i in 0..orders.len() {
			match orders.get(i) {
				Some(Some(_)) => all_min_trade_qties.push(*min_trade_qties.get(i).unwrap()),
				_ => all_min_trade_qties.push(f64::NAN),
			}
		}

		self.protocol_orders
			.recalculate_protocol_orders_allocation(&self.fills, total_controlled_size, &all_min_trade_qties)
	}
}

/// Wrapper around Orders, which allows for updating the target after a partial fill, without making a new request to the protocol.
/// NB: the protocol itself must internally uphold the equality of ids attached to orders to corresponding fields of ProtocolOrders, as well as to ensure that all possible orders the protocol can ether request are initialized in every ProtocolOrders instance it outputs.
#[derive(Debug, Clone, derive_new::new, Default)]
pub struct ProtocolOrders {
	pub protocol_id: String,
	pub __orders: Vec<Option<ConceptualOrderPercents>>, // pub for testing purposes
}
#[derive(Clone, Debug, Default, derive_new::new)]
struct RecalculatedAllocation {
	orders: Vec<ConceptualOrder<ProtocolOrderId>>,
	total_offset: Option<f64>,
}
impl ProtocolOrders {
	pub fn empty_mask(&self) -> Vec<f64> {
		vec![0.; self.__orders.len()]
	}

	pub fn recalculate_protocol_orders_allocation(&self, filled_mask: &[f64], total_controlled_notional: f64, min_trade_qties: &[f64]) -> Vec<ConceptualOrder<ProtocolOrderId>> {
		assert_eq!(self.__orders.len(), filled_mask.len());
		assert_eq!(self.__orders.len(), min_trade_qties.len());
		dbg!(&total_controlled_notional, &filled_mask, &self.__orders);

		let mut total_offset = 0.0;

		// subtract filled
		let mut orders: Vec<ConceptualOrder<ProtocolOrderId>> = self
			.__orders
			.iter()
			.enumerate()
			.filter_map(|(i, order)| {
				if let Some(o) = order.clone() {
					let mut exact_order = o.to_exact(total_controlled_notional, ProtocolOrderId::new(self.protocol_id.clone(), i));
					let filled = *filled_mask.get(i).unwrap_or(&0.0);

					if filled > exact_order.qty_notional * 0.99 {
						total_offset += filled - exact_order.qty_notional;
						return None;
					}

					exact_order.qty_notional -= filled;
					Some(exact_order)
				} else {
					None
				}
			})
			.collect();

		// redistribute the total size
		orders.sort_by(|a, b| b.qty_notional.partial_cmp(&a.qty_notional).unwrap_or(std::cmp::Ordering::Equal));
		let mut l = orders.len();
		let individual_offset = total_offset / l as f64;
		for i in (0..l).rev() {
			if orders[i].qty_notional < individual_offset {
				orders.remove(i);
				total_offset -= orders[i].qty_notional;
				l -= 1;
			} else {
				// if reached this once, all following elements will also eval to true, so the total_offset is constant now.
				orders[i].qty_notional -= individual_offset;
			}
		}
		if orders.len() == 0 && total_offset != 0.0 {
			tracing::warn!("Missed by {total_offset}");
		}

		orders
	}
}

#[cfg(test)]
mod tests {
	use insta::assert_debug_snapshot;
	use v_utils::trades::Side;

	use super::*;
	use crate::exchange_apis::{
		order_types::{ConceptualMarket, ConceptualOrderType, ConceptualStopMarket},
		Market, Symbol,
	};

	#[test]
	fn test_apply_mask() {
		let orders = ProtocolOrders::new(
			"test".to_string(),
			vec![Some(ConceptualOrderPercents::new(
				ConceptualOrderType::Market(ConceptualMarket::new(0.0)),
				Symbol::new("BTC".to_string(), "USDT".to_string(), Market::BinanceFutures),
				Side::Buy,
				0.5,
			))],
		);

		let filled_mask = vec![0.1];
		let total_controlled_notional = 1.0;
		let min_trade_qties = [0.007];
		let got = orders.recalculate_protocol_orders_allocation(&filled_mask, total_controlled_notional, &min_trade_qties);
		assert_eq!(got.len(), 1);
		assert_eq!(got[0].qty_notional, 0.4);
	}

	#[test]
	fn test_apply_mask_multiple_orders() {
		let orders = ProtocolOrders::new(
			"test".to_string(),
			vec![
				Some(ConceptualOrderPercents::new(
					ConceptualOrderType::Market(ConceptualMarket::new(1.0)),
					Symbol::new("BTC".to_string(), "USDT".to_string(), Market::BinanceFutures),
					Side::Buy,
					0.1,
				)),
				Some(ConceptualOrderPercents::new(
					ConceptualOrderType::Market(ConceptualMarket::new(1.0)),
					Symbol::new("ETH".to_string(), "USDT".to_string(), Market::BinanceFutures),
					Side::Sell,
					0.5,
				)),
				Some(ConceptualOrderPercents::new(
					ConceptualOrderType::StopMarket(ConceptualStopMarket::new(0.42)),
					Symbol::new("ADA".to_string(), "USDT".to_string(), Market::BinanceFutures),
					Side::Sell,
					30.0,
				)),
				Some(ConceptualOrderPercents::new(
					ConceptualOrderType::Market(ConceptualMarket::new(1.0)),
					Symbol::new("ADA".to_string(), "USDT".to_string(), Market::BinanceFutures),
					Side::Buy,
					25.0,
				)),
			],
		);

		let filled_mask = vec![0.05, 0.2, 0.0, 10.0];
		let total_controlled_notional = 1.0;
		let min_trade_qties = [0.007, 0.075, 10.0, 10.0];
		let got = orders.recalculate_protocol_orders_allocation(&filled_mask, total_controlled_notional, &min_trade_qties);

		let qties = got.into_iter().map(|co| co.qty_notional).collect::<Vec<f64>>();
		assert_debug_snapshot!(qties, @r###"
	[
			0.05,
			0.3,
			30.0
			15.0
	]
  "###);
	}

	#[test]
	fn test_nones_in_orders() {
		let orders = ProtocolOrders::new(
			"test".to_string(),
			vec![
				None,
				Some(ConceptualOrderPercents::new(
					ConceptualOrderType::Market(ConceptualMarket::new(1.0)),
					Symbol::new("ADA".to_string(), "USDT".to_string(), Market::BinanceFutures),
					Side::Buy,
					25.0,
				)),
			],
		);

		let filled_mask = vec![0.0, 0.0];
		let total_controlled_notional = 1.0;
		let min_trade_qties = [0.0, 10.0];
		let got = orders.recalculate_protocol_orders_allocation(&filled_mask, total_controlled_notional, &min_trade_qties);

		let qties = got.into_iter().map(|co| co.qty_notional).collect::<Vec<f64>>();
		assert_debug_snapshot!(qties, @r###"
  [
      25.0,
  ]
  "###);
	}

	#[test]
	fn test_apply_mask_fully_filled_orders() {
		let orders = ProtocolOrders::new(
			"test".to_string(),
			vec![
				Some(ConceptualOrderPercents::new(
					ConceptualOrderType::Market(ConceptualMarket::new(0.0)),
					Symbol::new("BTC".to_string(), "USDT".to_string(), Market::BinanceFutures),
					Side::Buy,
					25.0,
				)),
				Some(ConceptualOrderPercents::new(
					ConceptualOrderType::Market(ConceptualMarket::new(0.0)),
					Symbol::new("ETH".to_string(), "USDT".to_string(), Market::BinanceFutures),
					Side::Sell,
					25.0,
				)),
			],
		);

		let filled_mask = vec![25.0, 25.0];
		let total_controlled_notional = 1.0;
		let min_trade_qties = [10.0, 10.0];
		let got = orders.recalculate_protocol_orders_allocation(&filled_mask, total_controlled_notional, &min_trade_qties);

		assert_eq!(got.len(), 0);
	}
}
