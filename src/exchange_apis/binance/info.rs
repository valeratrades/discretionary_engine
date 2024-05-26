use serde::{Deserialize, Serialize};
use serde_json::Value;
use url::Url;

use crate::exchange_apis::{Market, Symbol};

lazy_static::lazy_static! {
	pub static ref FUTURES_EXCHANGE_INFO: FuturesExchangeInfo = {
		let base_url = Market::BinanceFutures.get_base_url();
		let url = base_url.join("/fapi/v1/exchangeInfo").unwrap();
		let r = reqwest::blocking::get(url).unwrap();
		let futures_exchange_info: FuturesExchangeInfo = r.json().unwrap();
		futures_exchange_info
	};
}

// FuturesExchangeInfo structs {{{
#[derive(Debug, Deserialize, Serialize)]
pub struct FuturesExchangeInfo {
	pub exchangeFilters: Vec<String>,
	pub rateLimits: Vec<RateLimit>,
	pub serverTime: i64,
	pub assets: Vec<Value>,
	pub symbols: Vec<FuturesSymbol>,
	pub timezone: String,
}
impl FuturesExchangeInfo {
	pub fn url() -> Url {
		let base_url = Market::BinanceFutures.get_base_url();
		base_url.join("/fapi/v1/exchangeInfo").unwrap()
	}

	pub fn min_notional(&self, symbol: Symbol) -> f64 {
		let symbol_info = self.symbols.iter().find(|s| s.symbol == symbol.ticker()).unwrap();
		let min_notional = symbol_info.filters.iter().find(|f| f["filterType"] == "MIN_NOTIONAL").unwrap();
		min_notional["minNotional"].as_str().unwrap().parse().unwrap()
	}
}

#[derive(Debug, Deserialize, Serialize)]
pub struct RateLimit {
	pub interval: String,
	pub intervalNum: u32,
	pub limit: u32,
	pub rateLimitType: String,
}

// the thing with multiplying orders due to weird limits should be here.
//#[derive(Debug, Deserialize, Serialize)]
//#[allow(non_snake_case)]
//struct SymbolFilter {
//	filterType: String,
//	maxPrice: String,
//	minPrice: String,
//	tickSize: String,
//	maxQty: String,
//	minQty: String,
//	stepSize: String,
//	limit: u32,
//	notional: String,
//	multiplierUp: String,
//	multiplierDown: String,
//	multiplierDecimal: u32,
//}

#[derive(Debug, Deserialize, Serialize)]
pub struct FuturesSymbol {
	pub symbol: String,
	pub pair: String,
	pub contractType: String,
	pub deliveryDate: i64,
	pub onboardDate: i64,
	pub status: String,
	pub maintMarginPercent: String,
	pub requiredMarginPercent: String,
	pub baseAsset: String,
	pub quoteAsset: String,
	pub marginAsset: String,
	pub pricePrecision: u32,
	pub quantityPrecision: usize,
	pub baseAssetPrecision: u32,
	pub quotePrecision: u32,
	pub underlyingType: String,
	pub underlyingSubType: Vec<String>,
	pub settlePlan: u32,
	pub triggerProtect: String,
	pub filters: Vec<Value>,
	pub OrderType: Option<Vec<String>>,
	pub timeInForce: Vec<String>,
	pub liquidationFee: String,
	pub marketTakeBound: String,
}

#[derive(Deserialize, Debug)]
pub struct FuturesAllPositionsResponse {
	pub entryPrice: String,
	pub breakEvenPrice: String,
	pub marginType: String,
	pub isAutoAddMargin: Value,
	pub isolatedMargin: String,
	pub leverage: String,
	pub liquidationPrice: String,
	pub markPrice: String,
	pub maxNotionalValue: String,
	pub positionAmt: String,
	pub notional: String,
	pub isolatedWallet: String,
	pub symbol: String,
	pub unRealizedProfit: String,
	pub positionSide: Value, // is "BOTH" in standard (non-hedge mode) requests, because designed by fucking morons. Apparently we now have negative values in `positionAmt`, if short.
	pub updateTime: i64,
}
impl FuturesAllPositionsResponse {
	pub fn get_url() -> Url {
		let base_url = Market::BinanceFutures.get_base_url();
		base_url.join("/fapi/v2/positionRisk").unwrap()
	}
}

#[derive(Serialize, Debug, Clone)]
pub struct FuturesOrder {
	pub symbol: String,
	pub price: f64,
	pub quantity: f64,
}

#[derive(Deserialize, Debug, Clone)]
pub struct ResponseKline {
	pub open_time: i64,
	pub open: String,
	pub high: String,
	pub low: String,
	pub close: String,
	pub volume: String,
	pub close_time: u64,
	pub quote_asset_volume: String,
	pub number_of_trades: usize,
	pub taker_buy_base_asset_volume: String,
	pub taker_buy_quote_asset_volume: String,
	pub ignore: String,
}
//,}}}
