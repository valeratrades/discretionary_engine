pub mod binance;
use crate::positions::PositionCallback;
use std::collections::HashMap;
pub mod order_types;
use self::order_types::{ConceptualOrderType, OrderType, ProtocolOrderId};
use crate::config::AppConfig;
use anyhow::Result;
use derive_new::new;
use order_types::{ConceptualOrder, Order};
use url::Url;
use uuid::Uuid;
use v_utils::macros::graphemics;

pub async fn compile_total_balance(config: AppConfig) -> Result<f64> {
	let read_key = config.binance.read_key.clone();
	let read_secret = config.binance.read_secret.clone();

	let mut handlers = vec![
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
#[derive(Clone, Debug, derive_new::new)]
pub struct HubCallback {
	key: Uuid,
	fill_qty: f64,
	order: Order,
}

#[derive(Clone, Debug, derive_new::new)]
pub struct HubPassforward {
	key: Uuid,
	orders: Vec<Order>,
}

//TODO!!: All positions should have ability to clone tx to this
/// Currently hard-codes for a single position.
/// Uuid in the Receiver is of Position
pub async fn hub(config: AppConfig, mut rx: tokio::sync::mpsc::Receiver<(Vec<ConceptualOrder>, PositionCallback)>) {
	//TODO!!: assert all protocol orders here with trigger prices have them above/below current price in accordance to order's side.
	//- init the runtime of exchanges

	//TODO!!!!!!: \
	//let config_clone = config.clone();
	//tokio::spawn(async move {
	//	binance::binance_runtime(config_clone, todo!(), todo!()).await;
	//});

	let ex = &crate::exchange_apis::binance::info::FUTURES_EXCHANGE_INFO;

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
						let order = Order::new(o.id.uuid, order_types::OrderType::Market, o.symbol.clone(), o.side, o.qty_notional);
						actual_orders.entry(Market::BinanceFutures).or_default().push(order);
					}
					ConceptualOrderType::StopMarket(stop_market) => {
						let order = Order::new(
							o.id.uuid,
							order_types::OrderType::StopMarket(order_types::StopMarketOrder::new(stop_market.price)),
							o.symbol.clone(),
							o.side,
							o.qty_notional,
						);
						actual_orders.entry(Market::BinanceFutures).or_default().push(order);
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
						binance::dirty_hardcoded_exec(order.clone(), &config).await.unwrap();
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

#[allow(dead_code)]
#[derive(Clone, Debug, PartialEq, Eq, Hash, Default)]
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
///let symbol = "BTC-USDT-BinanceFutures".parse::<discretionary_engine::exchange_apis::Symbol>().unwrap();
///```
#[derive(Debug, Default, Clone, PartialEq, Eq, Hash, new)]
pub struct Symbol {
	pub base: String,
	pub quote: String,
	pub market: Market,
}
impl Symbol {
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