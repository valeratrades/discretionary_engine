pub mod binance;
use uuid::Uuid;
pub mod order_types;
use crate::{config::AppConfig, PositionCallback};
use anyhow::Result;
use order_types::ConceptualOrder;
use url::Url;
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
// 	+ send back the total_notional of an executed order immediately

// 	+ well, do the execution, and in a manner that the target order distribution can be updated midway
// So in practice, we want to write to a local Arc<Mutex<T>>, which contains updated target orders for each exchange, which are uploaded according to the maximum frequency they allow.

//TODO!: require providing id of the position of origin.
/// the global access rx for this is shared among all positions. Each position provides a watch::Sender, to receiver
/// Currently hard-codes for a single position.
pub fn i_have_no_clue_how_to_represent_this(rx: mpsc::Receiver<(Vec<ConceptualOrder>, PositionCallback)>) {
	//- init the runtime of exchanges

	//- merge new recv() with the rest of the known orders globally across all positions.

	//- translate all into exact actual orders on specific exchanges if we were placing them now.
	// // each ActualOrder must pertain the id of the ConceptualOrder instance it is expressing

	//- compare with the current, calculate the costs of moving (tx between exchanges, latency exposure, spinning the limit), produce final target actual orders for each exchange.

	//- send the batch of new exact orders to the controlling runtime of each exchange.
	// // these are started locally, as none can be initiated through other means.

	// let mut orders = HashMap::new();
	// let mut total_notional = 0.0;
	// let mut total_notional_executed = 0.0;
	// let mut total_notional_remaining = 0.0;

	// loop {
	// 	let order = rx.recv().unwrap();
	// 	match order {
	// 		ConceptualOrder::Market(m) => {
	// 			total_notional += m.qty_notional;
	// 			total_notional_remaining += m.qty_notional;
	// 		}
	// 		ConceptualOrder::Limit(l) => {
	// 			total_notional += l.qty_notional;
	// 			total_notional_remaining += l.qty_notional;
	// 		}
	// 		ConceptualOrder::StopMarket(s) => {
	// 			total_notional += s.qty_notional;
	// 			total_notional_remaining += s.qty_notional;
	// 		}
	// 	}
	// }
}

// translation layer: Vec<ConceptualOrder> -> ActualOrders

// want one runtime handling all of the positions at once, so as not to have to impose artificial requirements on positions containing the same ticker.

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
