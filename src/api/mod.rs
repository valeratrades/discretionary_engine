pub mod binance;
pub mod order_types;
use crate::config::Config;
use crate::positions::Position;
use anyhow::Result;
use binance::OrderStatus;
use chrono::Utc;
use order_types::OrderType;
use std::collections::HashMap;
use std::sync::atomic::Ordering;
use std::sync::Arc;
use tokio::time::Duration;
use url::Url;
use v_utils::macros::graphemics;
use v_utils::trades::{Side, Timeframe};

pub async fn compile_total_balance(config: Config) -> Result<f64> {
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

///NB: Temporary function that assumes BinanceFutures, and will be replaced with making the same request to a BinanceExchange struct, with preloaded values.
pub async fn round_to_required_precision(symbol: String, quantity: f64) -> Result<f64> {
	let quantity_precision = binance::futures_quantity_precision(symbol.clone()).await?;
	let factor = 10_f64.powi(quantity_precision as i32);
	let quantity_adjusted = (quantity * factor).round() / factor;
	Ok(quantity_adjusted)
}

//? Should I make this return new total postion size?
pub async fn open_futures_position(
	config: Config,
	positions: &Positions,
	symbol: String,
	side: Side,
	usdt_quantity: f64,
	protocols: Protocols,
) -> Result<()> {
	let full_key = config.binance.full_key.clone();
	let full_secret = config.binance.full_secret.clone();
	let position = Position::new(Market::BinanceFutures, side, symbol.clone(), usdt_quantity, protocols, Utc::now());

	let current_price_handler = binance::futures_price(symbol.clone());
	let quantity_percision_handler = binance::futures_quantity_precision(symbol.clone());
	let current_price = current_price_handler.await?;
	let quantity_precision: usize = quantity_percision_handler.await?;

	let coin_quantity = usdt_quantity / current_price;
	let factor = 10_f64.powi(quantity_precision as i32);
	let coin_quantity_adjusted = (coin_quantity * factor).round() / factor;

	let order_id = binance::post_futures_order(
		full_key.clone(),
		full_secret.clone(),
		binance::OrderType::Market,
		symbol.clone(),
		side.clone(),
		coin_quantity_adjusted,
	)
	.await?;
	//info!(target: "/tmp/discretionary_engine.lock", "placed order: {:?}", order_id);
	loop {
		let order = binance::poll_futures_order(full_key.clone(), full_secret.clone(), order_id.clone(), symbol.clone()).await?;
		if order.status == OrderStatus::Filled {
			let order_notional = order.origQty.parse::<f64>()?;
			let order_usdt = order.avgPrice.unwrap().parse::<f64>()? * order_notional;
			//NB: currently assuming there is nothing else to the position.
			position.qty_notional.store(order_notional, Ordering::SeqCst);
			position.qty_usdt.store(order_usdt, Ordering::SeqCst);

			//info!(target: "/tmp/discretionary_engine.lock", "Order filled; new position: {:?}", &position);
			position.protocols.attach(&position).await?;
			{
				positions.positions.lock().unwrap().push(position); // function to execute the orders, that start being proposed after `attach`, is on the entire Positions master struct, so they have no chance of being executed or accounted before this line.
			}
			break;
		}
		tokio::time::sleep(Duration::from_secs(1)).await;
	}
	positions.sync(config.clone()).await?;

	Ok(())
}

//TODO!: \
pub async fn get_positions(config: &Config) -> Result<HashMap<String, f64>> {
	binance::get_futures_positions(config.binance.full_key.clone(), config.binance.full_secret.clone()).await
}

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
///let symbol = "BTC-USDT-BinanceFutures".parse::<Symbol>().unwrap();
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

/// Order spec and human-interpretable unique name of the structure requesting it. Ex: `"trailing_stop"`
/// Later on the submission engine just looks at the market, and creates according api-specific structure. However, user only sees this.
#[derive(Debug, Clone)]
pub struct OrderSpec {
	pub order: OrderType,
	pub name: String,
}
//? would it not make more sense to just pass around tuples (OrderType, String), where String is obviously name. Might be simpler and actually more explicit than having yet another struct.
