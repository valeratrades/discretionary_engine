pub mod binance;
use crate::positions::PositionCallback;
use std::collections::HashMap;
pub mod order_types;
use self::order_types::{ConceptualOrderType, OrderType, ProtocolOrderId};
use crate::config::AppConfig;
use anyhow::Result;
use order_types::{ConceptualOrder, Order};
use tracing::{debug, info};
use url::Url;
use uuid::Uuid;
use v_utils::macros::graphemics;

pub async fn compile_total_balance(config: AppConfig) -> Result<f64> {
	let read_key = config.binance.read_key.clone();
	let read_secret = config.binance.read_secret.clone();

	let mut handlers = Vec::new();
	handlers.push(binance::get_balance(read_key.clone(), read_secret.clone(), Market::BinanceFutures));
	handlers.push(binance::get_balance(read_key.clone(), read_secret.clone(), Market::BinanceSpot));

	let mut total_balance = 0.0;
	for handler in handlers {
		let balance = handler.await?;
		total_balance += balance;
	}
	Ok(total_balance)
}

//TODO!!: All positions should have ability to clone tx to this
/// Currently hard-codes for a single position.
/// Uuid in the Receiver is of Position
pub async fn hub_ish(mut rx: tokio::sync::mpsc::Receiver<(Vec<ConceptualOrder>, PositionCallback)>) {
	//- init the runtime of exchanges

	let mut stupid_filled_one = false;

	let mut callback: HashMap<Uuid, tokio::sync::mpsc::Sender<Vec<(f64, ProtocolOrderId)>>> = HashMap::new();
	let mut known_orders: HashMap<Uuid, Vec<ConceptualOrder>> = HashMap::new();

	while let Some((new_orders, position_callback)) = rx.recv().await {
		//TODO!!!!!!!: check that the sender provided correct uuid we sent with the notification of the last fill to it.
		known_orders.insert(position_callback.position_uuid, new_orders);

		let mut actual_orders: HashMap<Market, Vec<Order>> = HashMap::new();
		for (key, vec) in known_orders.iter() {
			for o in vec {
				match &o.order_type {
					ConceptualOrderType::Market(_) => {
						let order = Order::new(o.id.uuid.clone(), order_types::OrderType::Market, o.symbol.clone(), o.side, o.qty_notional);
						actual_orders.entry(Market::BinanceFutures).or_insert_with(Vec::new).push(order);
					}
					ConceptualOrderType::StopMarket(stop_market) => {
						let order = Order::new(
							o.id.uuid.clone(),
							order_types::OrderType::StopMarket(order_types::StopMarketOrder::new(stop_market.price)),
							o.symbol.clone(),
							o.side,
							o.qty_notional,
						);
						actual_orders.entry(Market::BinanceFutures).or_insert_with(Vec::new).push(order);
					}
					_ => panic!("Unsupported order type"),
				}
			}
		}

		for (key, vec) in actual_orders.iter() {
			match key {
				Market::BinanceSpot => todo!(),
				Market::BinanceMargin => todo!(),
				Market::BinanceFutures => {
					//TODO!!!!!!: generalize and move to the binance module
					if !stupid_filled_one {
						let order = vec.get(0).unwrap();
						binance::dirty_hardcoded_exec(order.clone()).await.unwrap();
						stupid_filled_one = true;
					}
				}
			}
		}
	}

	//- translate all into exact actual orders on specific exchanges if we were placing them now.
	// // each ActualOrder must pertain the id of the ConceptualOrder instance it is expressing

	//- compare with the current, calculate the costs of moving (tx between exchanges, latency exposure, spinning the limit), produce final target actual orders for each exchange.

	//- send the batch of new exact orders to the controlling runtime of each exchange.
	// // these are started locally, as none can be initiated through other means.

	// HashMap<Exchange, Vec<Order>> // On fill notif of an exchange, we find the according PositionCallback, by searching for ConceptualOrder with matching uuid

	//+ hardcode following binance orders here
}

//pub fn init_hub() -> tokio::sync::mpsc::Sender<(Vec<api::order_types::ConceptualOrder>, positions::PositionCallback)> {
//	let (tx, rx) = tokio::sync::mpsc::channel(32);
//	tokio::spawn(crate::api::hub_ish(rx));
//	tx
//}

#[allow(dead_code)]
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum Market {
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
	type Err = anyhow::Error;

	fn from_str(s: &str) -> Result<Self> {
		match s {
			_ if graphemics!(BinanceFutures).contains(&s) => Ok(Market::BinanceFutures),
			_ if graphemics!(BinanceSpot).contains(&s) => Ok(Market::BinanceSpot),
			_ if graphemics!(BinanceMargin).contains(&s) => Ok(Market::BinanceMargin),
			_ => Err(anyhow::anyhow!("Unknown market: {}", s)),
		}
	}
}

/// Contains information sufficient to identify the exact orderbook.
///```rust
///let symbol = "BTC-USDT-BinanceFutures".parse::<discretionary_engine::api::Symbol>().unwrap();
///```
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Symbol {
	pub base: String,
	pub quote: String,
	pub market: Market,
}
impl std::fmt::Display for Symbol {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		write!(f, "{}", self.market.format_symbol(&self.base))
	}
}
impl std::str::FromStr for Symbol {
	type Err = anyhow::Error;

	fn from_str(s: &str) -> Result<Self> {
		let split = s.split('-').collect::<Vec<&str>>();
		Ok(Self {
			base: split.get(0).unwrap().to_string(),
			quote: split.get(1).unwrap().to_string(),
			market: Market::from_str(split.get(2).unwrap())?,
		})
	}
}
