use std::{collections::HashMap, sync::Arc};

use color_eyre::eyre::Result;
use reqwest::Method;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use serde_with::{serde_as, DisplayFromStr};
use url::Url;

use super::unsigned_request;
use crate::{
	config::AppConfig,
	exchange_apis::{order_types::ConceptualOrderType, Market, Symbol},
	utils::deser_reqwest,
};

// FuturesExchangeInfo structs {{{
#[derive(Debug, Clone, Default, Deserialize, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct BinanceExchangeFutures {
	pub exchange_filters: Vec<String>,
	pub rate_limits: Vec<RateLimit>,
	pub server_time: i64,
	pub assets: Vec<Value>,
	pub symbols: Vec<FuturesSymbol>,
	pub timezone: String,
}
impl BinanceExchangeFutures {
	pub async fn init(_config: Arc<AppConfig>) -> Result<Self> {
		let url = Self::url().to_string();
		let r = unsigned_request(Method::GET, &url, HashMap::new()).await?;
		let binance_exchange_futures: Self = deser_reqwest(r).await?;
		Ok(binance_exchange_futures)
	}

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

#[derive(Debug, Clone, Default, Deserialize, Serialize, PartialEq)]
pub struct RateLimit {
	pub interval: String,
	pub intervalNum: u32,
	pub limit: u32,
	pub rateLimitType: String,
}

// the thing with multiplying orders due to weird limits should be here.
//#[derive(Debug, Deserialize, Serialize)]
//#[allow(non_snake_case)]
// struct SymbolFilter {
// 	filterType: String,
// 	maxPrice: String,
// 	minPrice: String,
// 	tickSize: String,
// 	maxQty: String,
// 	minQty: String,
// 	stepSize: String,
// 	limit: u32,
// 	notional: String,
// 	multiplierUp: String,
// 	multiplierDown: String,
// 	multiplierDecimal: u32,
//}

#[derive(Debug, Clone, Default, Deserialize, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct FuturesSymbol {
	pub symbol: String,
	pub pair: String,
	pub contract_type: String,
	pub delivery_date: i64,
	pub onboard_date: i64,
	pub status: String,
	pub maint_margin_percent: String,
	pub required_margin_percent: String,
	pub base_asset: String,
	pub quote_asset: String,
	pub margin_asset: String,
	pub price_precision: u32,
	pub quantity_precision: usize,
	pub base_asset_precision: u32,
	pub quote_precision: u32,
	pub underlying_type: String,
	pub underlying_sub_type: Vec<String>,
	pub settle_plan: u32,
	pub trigger_protect: String,
	pub filters: Vec<Value>,
	pub order_type: Option<Vec<String>>,
	pub time_in_force: Vec<String>,
	pub liquidation_fee: String,
	pub market_take_bound: String,
}
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(tag = "filterType")]
pub enum Filter {
	#[serde(rename = "PRICE_FILTER")]
	PriceFilter(PriceFilter),
	#[serde(rename = "LOT_SIZE")]
	LotSize(LotSizeFilter),
	#[serde(rename = "MARKET_LOT_SIZE")]
	MarketLotSize(MarketLotSizeFilter),
	#[serde(rename = "MAX_NUM_ORDERS")]
	MaxNumOrders(MaxNumOrdersFilter),
	#[serde(rename = "MAX_NUM_ALGO_ORDERS")]
	MaxNumAlgoOrders(MaxNumAlgoOrdersFilter),
	#[serde(rename = "MIN_NOTIONAL")]
	MinNotional(MinNotionalFilter),
	#[serde(rename = "PERCENT_PRICE")]
	PercentPrice(PercentPriceFilter),
}

#[serde_as]
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PriceFilter {
	#[serde_as(as = "DisplayFromStr")]
	pub min_price: f64,
	#[serde_as(as = "DisplayFromStr")]
	pub max_price: f64,
	#[serde_as(as = "DisplayFromStr")]
	pub tick_size: f64,
}

#[serde_as]
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LotSizeFilter {
	#[serde_as(as = "DisplayFromStr")]
	pub max_qty: f64,
	#[serde_as(as = "DisplayFromStr")]
	pub min_qty: f64,
	#[serde_as(as = "DisplayFromStr")]
	pub step_size: f64,
}

#[serde_as]
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MarketLotSizeFilter {
	#[serde_as(as = "DisplayFromStr")]
	pub max_qty: f64,
	#[serde_as(as = "DisplayFromStr")]
	pub min_qty: f64,
	#[serde_as(as = "DisplayFromStr")]
	pub step_size: f64,
}

#[serde_as]
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct MaxNumOrdersFilter {
	pub limit: u32,
}

#[serde_as]
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct MaxNumAlgoOrdersFilter {
	pub limit: u32,
}

#[serde_as]
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct MinNotionalFilter {
	#[serde_as(as = "DisplayFromStr")]
	pub notional: f64,
}

#[serde_as]
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PercentPriceFilter {
	#[serde_as(as = "DisplayFromStr")]
	pub multiplier_up: f64,
	#[serde_as(as = "DisplayFromStr")]
	pub multiplier_down: f64,
	pub multiplier_decimal: u8,
}

impl FuturesSymbol {
	fn get_filter<T: for<'de> Deserialize<'de>>(&self, filter_type: &str) -> Option<T> {
		self.filters.iter().find_map(|filter| {
			if filter["filterType"] == filter_type {
				serde_json::from_value(filter.clone()).ok()
			} else {
				None
			}
		})
	}

	pub fn price_filter(&self) -> Option<PriceFilter> {
		self.get_filter("PRICE_FILTER")
	}

	pub fn lot_size_filter(&self) -> Option<LotSizeFilter> {
		self.get_filter("LOT_SIZE")
	}

	pub fn market_lot_size_filter(&self) -> Option<MarketLotSizeFilter> {
		self.get_filter("MARKET_LOT_SIZE")
	}

	pub fn max_num_orders_filter(&self) -> Option<MaxNumOrdersFilter> {
		self.get_filter("MAX_NUM_ORDERS")
	}

	pub fn max_num_algo_orders_filter(&self) -> Option<MaxNumAlgoOrdersFilter> {
		self.get_filter("MAX_NUM_ALGO_ORDERS")
	}

	pub fn min_notional_filter(&self) -> Option<MinNotionalFilter> {
		self.get_filter("MIN_NOTIONAL")
	}

	pub fn percent_price_filter(&self) -> Option<PercentPriceFilter> {
		self.get_filter("PERCENT_PRICE")
	}

	pub fn min_trade_qty_notional(&self, order_type: &ConceptualOrderType) -> f64 {
		let min_notional_for_limit = self.min_notional_filter().unwrap(); //HACK: this only checks limit orders.
		match order_type {
			ConceptualOrderType::Market(_) => min_notional_for_limit.notional,
			ConceptualOrderType::StopMarket(_) => min_notional_for_limit.notional,
			_ => min_notional_for_limit.notional,
		}
	}
}

#[derive(Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct FuturesAllPositionsResponse {
	pub entry_price: String,
	pub break_even_price: String,
	pub margin_type: String,
	pub is_auto_add_margin: Value,
	pub isolated_margin: String,
	pub leverage: String,
	pub liquidation_price: String,
	pub mark_price: String,
	pub max_notional_value: String,
	pub position_amt: String,
	pub notional: String,
	pub isolated_wallet: String,
	pub symbol: String,
	pub unrealized_profit: String,
	pub position_side: Value,
	pub update_time: i64,
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
