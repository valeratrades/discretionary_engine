pub mod binance;
use uuid::Uuid;
pub mod order_types;
use crate::config::Config;
use anyhow::Result;
use order_types::ConceptualOrder;
use url::Url;
use v_utils::macros::graphemics;

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
pub async fn round_to_required_precision(coin: String, quantity: f64) -> Result<f64> {
	let quantity_precision = binance::futures_quantity_precision(&coin).await?;
	let factor = 10_f64.powi(quantity_precision as i32);
	let quantity_adjusted = (quantity * factor).round() / factor;
	Ok(quantity_adjusted)
}

// Orders struct
// needs to:
//	+ receive updates of the target order placement
// 	+ send back the total_notional of an executed order immmediately
// 		- I probably want a mechanic for ensuring that the side requesting target_orders update is aware of the last close. Do I just attach a uuid, and then check if the same one is sent with the order update?
//		- ? But how the fuck would we know which orders to exclude from target????
// 		- ? And then after being told that an order is closed, I need to subtract it from the total size allocated to the according protocol.
// 	+ well, do the execution, and in a manner that the target order distribution can be updated midway

pub struct ActualOrders {
	pub snapshot_target_orders: Vec<ConceptualOrder>,
	pub current_uuid: Uuid,
	// channel here somehow. Needs to go both ways.
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
	pub order: ConceptualOrder,
	pub name: String,
}
//? would it not make more sense to just pass around tuples (OrderType, String), where String is obviously name. Might be simpler and actually more explicit than having yet another struct.
