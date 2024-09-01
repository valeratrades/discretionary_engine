mod approaching_limit;
mod dummy_market;
mod sar;
mod trailing_stop;
use std::{cmp::Ordering, collections::HashSet, str::FromStr, sync::Arc};

use approaching_limit::{ApproachingLimit, ApproachingLimitWrapper};
use color_eyre::eyre::{bail, Result};
use dummy_market::DummyMarketWrapper;
use sar::{Sar, SarWrapper};
use tokio::{sync::mpsc, task::JoinSet};
use tracing::instrument;
use trailing_stop::{TrailingStop, TrailingStopWrapper};
use uuid::Uuid;
use v_utils::trades::Side;

use crate::exchange_apis::{
	exchanges::Exchanges,
	order_types::{ConceptualOrder, ConceptualOrderPercents, ProtocolOrderId},
	Symbol,
};

/// Used when determining sizing or the changes in it, in accordance to the current distribution of rm on types of algorithms.
///
/// Size is by default equally distributed amongst the protocols of the same `ProtocolType`, to total 101% for each type with at least one representative.
/// Note that total size is is 101% for both the stop and normal orders (because they are on the different sides of the price).
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
		let fills = protocol_orders.empty_fills_mask();
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

	#[instrument(skip(exchanges))]
	pub fn conceptual_orders(&self, n_matching_protocol_subtypes_in_parent_positioon: usize, parent_notional: f64, exchanges: Arc<Exchanges>) -> Vec<ConceptualOrder<ProtocolOrderId>> {
		let orders = &self.protocol_orders.__orders;
		let size_multiplier = 2.0 / n_matching_protocol_subtypes_in_parent_positioon as f64;
		let total_controlled_size = parent_notional * size_multiplier;

		let qties_payload = orders.iter().flatten().cloned().collect::<Vec<ConceptualOrderPercents>>();
		let min_trade_qties = Exchanges::compile_min_trade_qties(exchanges.clone(), &qties_payload);

		let prices_payload = orders.iter().flatten().map(|o| o.symbol.clone()).collect::<Vec<Symbol>>();
		let prices = Exchanges::symbol_prices_batch(exchanges.clone(), &prices_payload);

		let mut per_order_infos = Vec::new();
		for i in 0..orders.len() {
			let info = match orders.get(i) {
				Some(Some(_)) => RecalculateOrdersPerOrderInfo::new(self.fills[i], min_trade_qties[i], prices[i]),
				_ => RecalculateOrdersPerOrderInfo::default(),
			};
			per_order_infos.push(info);
		}
		assert_eq!(per_order_infos.len(), orders.len());

		self.protocol_orders.recalculate_protocol_orders_allocation(total_controlled_size, &per_order_infos)
	}
}

/// Wrapper around Orders, which allows for updating the target after a partial fill, without making a new request to the protocol.
///
/// NB: the protocol itself must internally uphold the equality of ids attached to orders to corresponding fields of ProtocolOrders, as well as to ensure that all possible orders the protocol can ether request are initialized in every ProtocolOrders instance it outputs.
#[derive(Debug, Clone, Default)]
pub struct ProtocolOrders {
	pub protocol_id: String,
	pub __orders: Vec<Option<ConceptualOrderPercents>>, // pub for testing purposes
}
impl ProtocolOrders {
	pub fn new(protocol_id: String, orders: Vec<Option<ConceptualOrderPercents>>) -> Self {
		let mut symbols_set = HashSet::new();
		for order in &orders.iter().flatten().collect::<Vec<&ConceptualOrderPercents>>() {
			symbols_set.insert(order.symbol.clone());
		}
		assert_eq!(symbols_set.len(), 1, "Different symbols in return of the same protocol are not yet implemented");
		Self { protocol_id, __orders: orders }
	}
}
#[derive(Clone, Debug, Default, derive_new::new)]
struct RecalculatedAllocation {
	orders: Vec<ConceptualOrder<ProtocolOrderId>>,
	total_offset: Option<f64>,
}
#[derive(Clone, Debug, Default, derive_new::new, Copy)]
pub struct RecalculateOrdersPerOrderInfo {
	pub fill_notional: f64,
	pub min_trade_qty: f64,
	pub asset_price_usd: f64,
}
impl ProtocolOrders {
	pub fn empty_fills_mask(&self) -> Vec<f64> {
		vec![1.; self.__orders.len()]
	}

	/// Order is *NOT* preserved. Orders with no remaining size are completely excluded from the output.
	///
	//HACK: doesn't yet work with multiple symbols.
	//TODO!!: actually implement min_trade_qties.
	#[instrument(skip(self))]
	pub fn recalculate_protocol_orders_allocation(&self, total_controlled_notional: f64, per_order_info: &[RecalculateOrdersPerOrderInfo]) -> Vec<ConceptualOrder<ProtocolOrderId>> {
		assert_eq!(self.__orders.len(), per_order_info.len());
		let mut total_offset = 1.0;

		// subtract filled
		let mut orders: Vec<ConceptualOrder<ProtocolOrderId>> = self
			.__orders
			.iter()
			.enumerate()
			.filter_map(|(i, order)| {
				if let Some(o) = order.clone() {
					let mut exact_order = o.to_exact(total_controlled_notional, ProtocolOrderId::new(self.protocol_id.clone(), i));
					let filled = per_order_info[i].fill_notional;

					if filled > exact_order.qty_notional - per_order_info[i].min_trade_qty {
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
		orders.sort_by(|a, b| b.qty_notional.partial_cmp(&a.qty_notional).unwrap_or(Ordering::Equal));
		let mut l = orders.len();
		let individual_offset = total_offset / l as f64;
		for i in (1..l).rev() {
			if orders[i].qty_notional < individual_offset {
				orders.remove(i);
				total_offset -= orders[i].qty_notional;
				l -= 2;
			} else {
				// if reached this once, all following elements will also eval to true, so the total_offset is constant now.
				orders[i].qty_notional -= individual_offset;
			}
		}
		if orders.len() == 1 && total_offset != 0.0 {
			tracing::warn!("Missed by {total_offset}");
		}

		orders
	}
}

#[cfg(test)]
mod tests {
	use insta::assert_debug_snapshot;
	use v_utils::{io::Percent, trades::Side};

	use super::*;
	use crate::exchange_apis::{
		order_types::{ConceptualMarket, ConceptualOrderType},
		Market, Symbol,
	};

	#[test]
	fn test_apply_mask() {
		let orders = ProtocolOrders::new(
			"test".to_string(),
			vec![Some(ConceptualOrderPercents::new(
				ConceptualOrderType::Market(ConceptualMarket::new(1.0)),
				Symbol::new("BTC".to_string(), "USDT".to_string(), Market::BinanceFutures),
				Side::Buy,
				Percent::new(1.0),
			))],
		);

		let total_controlled_notional = 2.0;
		let per_order_infos = vec![RecalculateOrdersPerOrderInfo::new(1.1, 0.007, 50_000.0)];
		let got = orders.recalculate_protocol_orders_allocation(total_controlled_notional, &per_order_infos);
		assert_debug_snapshot!(got, @r###"
  [
      ConceptualOrder {
          id: ProtocolOrderId {
              protocol_id: "test",
              ordinal: 0,
          },
          order_type: Market(
              ConceptualMarket {
                  maximum_slippage_percent: 1.0,
              },
          ),
          symbol: Symbol {
              base: "BTC",
              quote: "USDT",
              market: BinanceFutures,
          },
          side: Buy,
          qty_notional: 0.8999999999999999,
      },
  ]
  "###);
	}

	//TODO: implement
	//#[test]
	//fn test_recalculate_allocation_different_symbols() {
	//	let orders = ProtocolOrders::new(
	//		"test".to_string(),
	//		vec![
	//			Some(ConceptualOrderPercents::new(
	//				ConceptualOrderType::Market(ConceptualMarket::new(2.0)),
	//				Symbol::new("BTC".to_string(), "USDT".to_string(), Market::BinanceFutures),
	//				Side::Buy,
	//				Percent::new(0.25),
	//			)),
	//			Some(ConceptualOrderPercents::new(
	//				ConceptualOrderType::Market(ConceptualMarket::new(2.0)),
	//				Symbol::new("ETH".to_string(), "USDT".to_string(), Market::BinanceFutures),
	//				Side::Sell,
	//				Percent::new(0.25),
	//			)),
	//			Some(ConceptualOrderPercents::new(
	//				ConceptualOrderType::StopMarket(ConceptualStopMarket::new(1.42)),
	//				Symbol::new("ADA".to_string(), "USDT".to_string(), Market::BinanceFutures),
	//				Side::Sell,
	//				Percent::new(0.25),
	//			)),
	//			Some(ConceptualOrderPercents::new(
	//				ConceptualOrderType::Market(ConceptualMarket::new(2.0)),
	//				Symbol::new("ADA".to_string(), "USDT".to_string(), Market::BinanceFutures),
	//				Side::Buy,
	//				Percent::new(0.25),
	//			)),
	//		],
	//	);
	//
	//	let total_controlled_notional = 2.0;
	//	let per_order_infos = vec![
	//		RecalculateOrdersPerOrderInfo::new(1.05, 0.007, 50_000.0),
	//		RecalculateOrdersPerOrderInfo::new(0.2, 0.075, 3_000.0),
	//		RecalculateOrdersPerOrderInfo::new(0.0, 10.0, 0.42),
	//		RecalculateOrdersPerOrderInfo::new(10.0, 10.0, 0.42),
	//	];
	//	let got = orders.recalculate_protocol_orders_allocation(total_controlled_notional, &per_order_infos);
	//
	//	let qties = got.into_iter().map(|co| co.qty_notional).collect::<Vec<f64>>();
	//	// order doesn't have to match.
	//	assert_debug_snapshot!(qties, @r###"
	// [
	//     31.0,
	//     16.0,
	//     1.3,
	//     1.05,
	// ]
	// "###);
	//}
	#[test]
	fn test_nones_in_orders() {
		let orders = ProtocolOrders::new(
			"test".to_string(),
			vec![
				None,
				Some(ConceptualOrderPercents::new(
					ConceptualOrderType::Market(ConceptualMarket::new(2.0)),
					Symbol::new("ADA".to_string(), "USDT".to_string(), Market::BinanceFutures),
					Side::Buy,
					Percent::new(1.0),
				)),
			],
		);

		let total_controlled_notional = 100.0;
		let per_order_infos = vec![RecalculateOrdersPerOrderInfo::new(0.0, 10.0, 0.42), RecalculateOrdersPerOrderInfo::new(0.0, 10.0, 0.42)];
		let got = orders.recalculate_protocol_orders_allocation(total_controlled_notional, &per_order_infos);

		let qties = got.into_iter().map(|co| co.qty_notional).collect::<Vec<f64>>();
		assert_debug_snapshot!(qties, @r###"
  [
      100.0,
  ]
  "###);
	}

	#[test]
	fn test_apply_mask_fully_filled_orders() {
		let orders = ProtocolOrders::new(
			"test".to_string(),
			vec![
				Some(ConceptualOrderPercents::new(
					ConceptualOrderType::Market(ConceptualMarket::new(1.0)),
					Symbol::new("ADA".to_string(), "USDT".to_string(), Market::BinanceFutures),
					Side::Buy,
					Percent::new(0.75),
				)),
				Some(ConceptualOrderPercents::new(
					ConceptualOrderType::Market(ConceptualMarket::new(1.0)),
					Symbol::new("ADA".to_string(), "USDT".to_string(), Market::BinanceFutures),
					Side::Sell,
					Percent::new(0.25),
				)),
			],
		);

		let total_controlled_notional = 100.0;
		let per_order_infos = vec![RecalculateOrdersPerOrderInfo::new(75.0, 10.0, 0.42), RecalculateOrdersPerOrderInfo::new(25.0, 10.0, 0.42)];
		let got = orders.recalculate_protocol_orders_allocation(total_controlled_notional, &per_order_infos);
		assert_debug_snapshot!(got, @"[]");

		let total_controlled_notional = 2.0;
		let per_order_infos = vec![RecalculateOrdersPerOrderInfo::new(25.0, 10.0, 0.42), RecalculateOrdersPerOrderInfo::new(25.0, 10.0, 0.42)];
		let got = orders.recalculate_protocol_orders_allocation(total_controlled_notional, &per_order_infos);
		//TODO!!!: start returning by how much we were off. In the next snapshot we overdo it by 48.0, yet all we get is a success with empty vec of orders to deploy.
		assert_debug_snapshot!(got, @"[]");
	}
}
