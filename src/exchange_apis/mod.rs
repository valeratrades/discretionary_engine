pub mod binance;
use std::collections::HashMap;

use tokio::sync::mpsc;
use v_utils::prelude::*;

use crate::{
	positions::PositionCallback,
	protocols::{ProtocolFill, ProtocolFills},
	PositionOrderId,
};
pub mod order_types;
use eyre::{bail, Result};
use order_types::{ConceptualOrder, Order};
use serde::{Deserialize, Serialize};
use url::Url;
use uuid::Uuid;
use v_utils::macros::graphemics;

use self::order_types::{ConceptualOrderType, ProtocolOrderId};
use crate::config::AppConfig;

pub async fn compile_total_balance(config: AppConfig) -> Result<f64> {
	let read_key = config.binance.read_key.clone();
	let read_secret = config.binance.read_secret.clone();

	let handlers = vec![
		binance::get_balance(read_key.clone(), read_secret.clone(), Market::BinanceFutures),
		binance::get_balance(read_key.clone(), read_secret.clone(), Market::BinanceSpot),
	];

	let mut total_balance = 0.0;
	for handler in handlers {
		let balance = handler.await?;
		total_balance += balance;
	}
	Ok(total_balance)
}

//? is there a conventional way to introduce these communication locks?
#[derive(Clone, Debug, Default, derive_new::new)]
pub struct HubCallback {
	key: Uuid,
	fill_qty: f64,
	order: Order<PositionOrderId>,
}

#[derive(Clone, Debug, Default, derive_new::new)]
pub struct HubPassforward {
	key: Uuid,
	orders: Vec<Order<PositionOrderId>>,
}

pub fn init_hub(config: AppConfig, parent_js: &mut JoinSet<Result<()>>) -> tokio::sync::mpsc::Sender<HubRx> {
	let (tx, rx) = tokio::sync::mpsc::channel(32);
	parent_js.spawn(hub(config.clone(), rx));
	tx
}

#[derive(Clone, Debug, derive_new::new)]
pub struct HubRx {
	key: Uuid,
	orders: Vec<ConceptualOrder<ProtocolOrderId>>,
	position_callback: PositionCallback,
}
pub async fn hub(config: AppConfig, mut rx: tokio::sync::mpsc::Receiver<HubRx>) -> Result<()> {
	// TODO!!: assert all protocol orders here with trigger prices have them above/below current price in accordance to order's side.
	//- init the runtime of exchanges

	let (fills_tx, mut fills_rx) = tokio::sync::mpsc::channel::<HubCallback>(32);
	let (orders_tx, orders_rx) = tokio::sync::watch::channel::<HubPassforward>(HubPassforward::default());
	let config_clone = config.clone();
	let mut js = JoinSet::new();

	js.spawn(async move {
		let mut exchange_runtimes_js = JoinSet::new();
		binance::binance_runtime(config_clone, &mut exchange_runtimes_js, fills_tx, orders_rx).await;
		exchange_runtimes_js.join_all().await;
	});

	let ex = &crate::exchange_apis::binance::info::futures_exchange_info;

	let mut last_fill_key = Uuid::default();
	let mut position_callbacks: HashMap<Uuid, mpsc::Sender<ProtocolFills>> = HashMap::new();
	let mut requested_orders: HashMap<Uuid, Vec<ConceptualOrder<ProtocolOrderId>>> = HashMap::new();

	fn handle_hub_rx(
		hub_rx: HubRx,
		last_fill_key: &Uuid,
		requested_orders: &mut HashMap<Uuid, Vec<ConceptualOrder<ProtocolOrderId>>>,
		position_callbacks: &mut HashMap<Uuid, mpsc::Sender<ProtocolFills>>,
		orders_tx: &tokio::sync::watch::Sender<HubPassforward>,
	) -> Result<()> {
		if *last_fill_key != hub_rx.key {
			tracing::debug!("Key mismatch, ignoring the request. Requested HubRx:\n{:?}\nCorrect key: {last_fill_key}", &hub_rx);
			return Ok(());
		}
		requested_orders.insert(hub_rx.position_callback.position_id, hub_rx.orders);
		position_callbacks.insert(hub_rx.position_callback.position_id, hub_rx.position_callback.sender);

		let flat_requested_orders = requested_orders.values().flatten().cloned().collect::<Vec<ConceptualOrder<ProtocolOrderId>>>();
		let flat_requested_orders_position_id: Vec<ConceptualOrder<PositionOrderId>> = flat_requested_orders
			.into_iter()
			.map(|o| {
				let new_id = PositionOrderId::new_from_protocol_id(hub_rx.position_callback.position_id, o.id);
				ConceptualOrder { id: new_id, ..o }
			})
			.collect();

		let target_orders = hub_process_orders(flat_requested_orders_position_id);

		let binance_futures_orders = target_orders
			.iter()
			.filter(|o| o.symbol.market == Market::BinanceFutures)
			.cloned()
			.collect::<Vec<Order<PositionOrderId>>>();

		let acceptance_token = Uuid::now_v7();
		let passforward = HubPassforward::new(acceptance_token, binance_futures_orders);
		orders_tx.send(passforward)?;
		Ok(())
	}

	async fn handle_fill(fill: HubCallback, last_fill_key: &mut Uuid, position_callbacks: &HashMap<Uuid, mpsc::Sender<ProtocolFills>>) -> Result<()> {
		*last_fill_key = fill.key;
		let position_id = fill.order.id.position_id;
		let sender = position_callbacks.get(&position_id).unwrap();
		let vec_fill = vec![ProtocolFill::new(fill.order.id.into(), fill.fill_qty)];
		sender.send(ProtocolFills::new(*last_fill_key, vec_fill)).await?;
		Ok(())
	}

	loop {
		tokio::select! {
			Some(hub_rx) = rx.recv() => {
				handle_hub_rx(hub_rx, &last_fill_key, &mut requested_orders, &mut position_callbacks, &orders_tx)?;
			},
			Some(fill) = fills_rx.recv() => {
				handle_fill(fill, &mut last_fill_key, &position_callbacks).await?;
			},
			else => break,
		}
	}

	js.join_all().await;
	Ok(())
}

// HACK
/// Thing that applies all the logic for deciding on how to best express ensemble of requested orders.
fn hub_process_orders(conceptual_orders: Vec<ConceptualOrder<PositionOrderId>>) -> Vec<Order<PositionOrderId>> {
	let mut orders: Vec<Order<PositionOrderId>> = Vec::new();
	for o in conceptual_orders {
		match &o.order_type {
			ConceptualOrderType::Market(_) => {
				let order = Order::new(o.id, order_types::OrderType::Market, o.symbol.clone(), o.side, o.qty_notional);
				orders.push(order);
			}
			ConceptualOrderType::StopMarket(stop_market) => {
				let order = Order::new(
					o.id,
					order_types::OrderType::StopMarket(order_types::StopMarketOrder::new(stop_market.price)),
					o.symbol.clone(),
					o.side,
					o.qty_notional,
				);
				orders.push(order);
			}
			_ => panic!("Unsupported order type"),
		}
	}
	orders
}

#[allow(dead_code)]
#[derive(Clone, Debug, PartialEq, Eq, Hash, Default, Serialize, Deserialize, Copy)]
pub enum Market {
	#[default]
	BinanceFutures,
	BinanceSpot,
	BinanceMargin,
}
impl Market {
	pub fn get_base_url(&self) -> Url {
		match self {
			Market::BinanceFutures => Url::parse("https://fapi.binance.com/").unwrap(),
			Market::BinanceSpot => Url::parse("https://api.binance.com/").unwrap(),
			Market::BinanceMargin => Url::parse("https://api.binance.com/").unwrap(),
		}
	}

	pub fn format_symbol(&self, symbol: &str) -> String {
		match self {
			Market::BinanceFutures => symbol.to_owned().to_uppercase() + "USDT",
			Market::BinanceSpot => symbol.to_owned().to_uppercase() + "USDT",
			Market::BinanceMargin => symbol.to_owned().to_uppercase() + "USDT",
		}
	}
}

impl std::str::FromStr for Market {
	type Err = eyre::Report;

	fn from_str(s: &str) -> Result<Self> {
		match s {
			_ if graphemics!(BinanceFutures).contains(&s) => Ok(Market::BinanceFutures),
			_ if graphemics!(BinanceSpot).contains(&s) => Ok(Market::BinanceSpot),
			_ if graphemics!(BinanceMargin).contains(&s) => Ok(Market::BinanceMargin),
			_ => bail!("Unknown market: {}", s),
		}
	}
}

/// Contains information sufficient to identify the exact orderbook.
/// ```rust
/// let symbol = "BTC-USDT-BinanceFutures".parse::<discretionary_engine::exchange_apis::Symbol>().unwrap();
/// ```
#[derive(Debug, Default, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Symbol {
	pub base: String,
	pub quote: String,
	pub market: Market,
}
impl Symbol {
	pub fn new<T: AsRef<str>>(base: T, quote: T, market: Market) -> Self {
		let base = base.as_ref().to_string();
		let quote = quote.as_ref().to_string();
		Self { base, quote, market }
	}

	pub fn ticker(&self) -> String {
		format!("{}{}", self.base, self.quote)
	}
}
impl std::fmt::Display for Symbol {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		write!(f, "{}", self.market.format_symbol(&self.base))
	}
}
impl std::str::FromStr for Symbol {
	type Err = eyre::Report;

	fn from_str(s: &str) -> Result<Self> {
		let split = s.split('-').collect::<Vec<&str>>();
		Ok(Self {
			base: split.get(0).unwrap().to_string(),
			quote: split.get(1).unwrap().to_string(),
			market: Market::from_str(split.get(2).unwrap())?,
		})
	}
}

#[cfg(test)]
mod tests {
	use order_types::{ConceptualMarket, ConceptualOrderType, ConceptualStopMarket};
	use v_utils::trades::Side;

	use super::*;
	use crate::exchange_apis::Market;

	#[test]
	fn test_hub_process() {
		let from_orders = vec![
			ConceptualOrder {
				id: PositionOrderId::new(Uuid::parse_str("058a3b5d-7ce0-465c-9339-b43261e99b19").unwrap(), "ts:p0.02".to_string(), 0),
				order_type: ConceptualOrderType::Market(ConceptualMarket::default()),
				symbol: Symbol::new("BTC".to_string(), "USDT".to_string(), Market::BinanceFutures),
				side: Side::Buy,
				qty_notional: 100.0,
			},
			ConceptualOrder {
				id: PositionOrderId::new(Uuid::parse_str("86acfda1-ef53-4bae-9f20-bbad6cbc8504").unwrap(), "ts:p0.02".to_string(), 1),
				order_type: ConceptualOrderType::StopMarket(ConceptualStopMarket::default()),
				symbol: Symbol::new("BTC".to_string(), "USDT".to_string(), Market::BinanceFutures),
				side: Side::Buy,
				qty_notional: 100.0,
			},
		];

		let converted = hub_process_orders(from_orders);
		insta::assert_json_snapshot!(converted, @r###"
  [
    {
      "id": {
        "position_id": "058a3b5d-7ce0-465c-9339-b43261e99b19",
        "protocol_id": "ts:p0.02",
        "ordinal": 0
      },
      "order_type": "Market",
      "symbol": {
        "base": "BTC",
        "quote": "USDT",
        "market": "BinanceFutures"
      },
      "side": "Buy",
      "qty_notional": 100.0
    },
    {
      "id": {
        "position_id": "86acfda1-ef53-4bae-9f20-bbad6cbc8504",
        "protocol_id": "ts:p0.02",
        "ordinal": 1
      },
      "order_type": {
        "StopMarket": {
          "price": 0.0
        }
      },
      "symbol": {
        "base": "BTC",
        "quote": "USDT",
        "market": "BinanceFutures"
      },
      "side": "Buy",
      "qty_notional": 100.0
    }
  ]
  "###);
	}
}
