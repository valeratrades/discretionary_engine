use crate::exchange_interactions::Market;
use anyhow::Result;
use chrono::Utc;
use hmac::{Hmac, Mac};
use reqwest::header::{HeaderMap, HeaderValue, CONTENT_TYPE};
use serde::{Deserialize, Serialize};
use serde_json::Number;
use serde_json::Value;
use serde_urlencoded;
use sha2::Sha256;
use std::collections::HashMap;
use url::Url;
use v_utils::trades::Side;

type HmacSha256 = Hmac<Sha256>;

#[allow(dead_code)]
pub enum HttpMethod {
	GET,
	POST,
	PUT,
	DELETE,
}

#[allow(dead_code)]
pub struct Binance {
	// And so then many calls will be replaced with just finding info here.
	futures_symbols: HashMap<String, FuturesSymbol>,
}

pub async fn signed_request(
	http_method: HttpMethod,
	endpoint_str: &str,
	mut params: HashMap<&'static str, String>,
	key: String,
	secret: String,
) -> Result<reqwest::Response> {
	let mut headers = HeaderMap::new();
	headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/json;charset=utf-8"));
	headers.insert("X-MBX-APIKEY", HeaderValue::from_str(&key).unwrap());
	let client = reqwest::Client::builder().default_headers(headers).build()?;

	let time_ms = Utc::now().timestamp_millis();
	params.insert("timestamp", format!("{}", time_ms));

	let query_string = serde_urlencoded::to_string(&params)?;

	let mut mac = HmacSha256::new_from_slice(secret.as_bytes()).unwrap();
	mac.update(query_string.as_bytes());
	let mac_bytes = mac.finalize().into_bytes();
	let signature = hex::encode(mac_bytes);

	let url = format!("{}?{}&signature={}", endpoint_str, query_string, signature);

	let r = match http_method {
		HttpMethod::GET => client.get(&url).send().await?,
		HttpMethod::POST => client.post(&url).send().await?,
		_ => panic!("Not implemented"),
	};
	Ok(r)
}

pub enum OrderType {
	Market,
	Limit,
	StopLoss,
	StopLossLimit,
	TakeProfit,
	TakeProfitLimit,
	LimitMaker,
}
impl ToString for OrderType {
	fn to_string(&self) -> String {
		match self {
			OrderType::Market => "MARKET".to_string(),
			OrderType::Limit => "LIMIT".to_string(),
			OrderType::StopLoss => "STOP_LOSS".to_string(),
			OrderType::StopLossLimit => "STOP_LOSS_LIMIT".to_string(),
			OrderType::TakeProfit => "TAKE_PROFIT".to_string(),
			OrderType::TakeProfitLimit => "TAKE_PROFIT_LIMIT".to_string(),
			OrderType::LimitMaker => "LIMIT_MAKER".to_string(),
		}
	}
}

pub async fn get_balance(key: String, secret: String, market: Market) -> Result<f32> {
	let params = HashMap::<&str, String>::new();
	match market {
		Market::BinanceFutures => {
			let base_url = market.get_base_url();
			let url = base_url.join("fapi/v2/balance")?;

			let r = signed_request(HttpMethod::GET, url.as_str(), params, key, secret).await?;
			let asset_balances: Vec<FuturesBalance> = r.json().await?;

			let mut total_balance = 0.0;
			for asset in asset_balances {
				total_balance += asset.balance.parse::<f32>()?;
			}
			Ok(total_balance)
		}
		Market::BinanceSpot => {
			let base_url = market.get_base_url();
			let url = base_url.join("/api/v3/account")?;

			let r = signed_request(HttpMethod::GET, url.as_str(), params, key, secret).await?;
			let account_details: SpotAccountDetails = r.json().await?;
			let asset_balances = account_details.balances;

			let mut total_balance = 0.0;
			for asset in asset_balances {
				total_balance += asset.free.parse::<f32>()?;
				total_balance += asset.locked.parse::<f32>()?;
			}
			Ok(total_balance)
		}
		Market::BinanceMargin => {
			let base_url = market.get_base_url();
			let url = base_url.join("/sapi/v1/margin/account")?;

			let r = signed_request(HttpMethod::GET, url.as_str(), params, key, secret).await?;
			let account_details: MarginAccountDetails = r.json().await?;
			let total_balance: f32 = account_details.TotalCollateralValueInUSDT.parse()?;

			Ok(total_balance)
		}
	}
}

pub async fn futures_price(symbol: String) -> Result<f32> {
	let base_url = Market::BinanceFutures.get_base_url();
	let url = base_url.join("/fapi/v2/ticker/price")?;

	let mut params = HashMap::<&str, String>::new();
	params.insert("symbol", symbol.clone());

	let client = reqwest::Client::new();
	let r = client.get(url).json(&params).send().await?;
	//let r_json: serde_json::Value = r.json().await?;
	//let price = r_json.get("price").unwrap().as_str().unwrap().parse::<f32>()?;
	// for some reason, can't sumbit with the symbol, so effectively requesting all for now
	let prices: Vec<serde_json::Value> = r.json().await?;
	let price = prices
		.iter()
		.find(|x| x.get("symbol").unwrap().as_str().unwrap().to_string() == symbol)
		.unwrap()
		.get("price")
		.unwrap()
		.as_str()
		.unwrap()
		.parse::<f32>()?;

	Ok(price)
}

pub async fn futures_quantity_precision(symbol: String) -> Result<usize> {
	let base_url = Market::BinanceFutures.get_base_url();
	let url = base_url.join("/fapi/v1/exchangeInfo")?;

	let r = reqwest::get(url).await?;
	let futures_exchange_info: FuturesExchangeInfo = r.json().await?;
	let symbol_info = futures_exchange_info.symbols.iter().find(|x| x.symbol == symbol).unwrap();

	Ok(symbol_info.quantityPrecision)
}

//TODO!!: make the symbol be from utils \
pub async fn post_futures_trade(
	key: String,
	secret: String,
	order_type: OrderType,
	symbol: String,
	side: Side,
	quantity: f32,
) -> Result<FuturesPositionResponse> {
	let url = FuturesPositionResponse::get_url();

	let mut params = HashMap::<&str, String>::new();
	params.insert("symbol", symbol);
	params.insert("side", side.to_string());
	params.insert("type", order_type.to_string());
	params.insert("quantity", format!("{}", quantity));

	let r = signed_request(HttpMethod::POST, url.as_str(), params, key, secret).await?;
	let response: FuturesPositionResponse = r.json().await?;
	Ok(response)
}

//=============================================================================
// Response structs
//=============================================================================

//? Should I be doing `impl get_url` on these? Unless we have high degree of shared feilds between the markets, this is a big "YES".
//? What if in cases when the struct is shared, I just implement market_specific commands to retrieve the url?
// Trying this out now. So far so good.

#[derive(Serialize, Deserialize, Debug)]
#[allow(non_snake_case)]
pub struct FuturesPositionResponse {
	clientOrderId: String,
	cumQty: Option<String>,
	cumQuote: String,
	executedQty: String,
	orderId: i64,
	avgPrice: Option<String>,
	origQty: String,
	price: String,
	reduceOnly: bool,
	side: String,
	positionSide: Option<String>, // only sent when in hedge mode
	status: String,
	stopPrice: String,
	closePosition: bool,
	symbol: String,
	timeInForce: String,
	r#type: String,
	origType: String,
	activatePrice: Option<f32>, // only returned on TRAILING_STOP_MARKET order
	priceRate: Option<f32>,     // only returned on TRAILING_STOP_MARKET order
	updateTime: i64,
	workingType: Option<String>, // no clue what this is
	priceProtect: bool,
	priceMatch: Option<String>, // huh
	selfTradePreventionMode: Option<String>,
	goodTillDate: Option<i64>,
}
impl FuturesPositionResponse {
	pub fn get_url() -> Url {
		let base_url = Market::BinanceFutures.get_base_url();
		base_url.join("/fapi/v1/order/test").unwrap() //TODO!!!!!: remove `/test` when done
	}
}

#[derive(Serialize, Deserialize, Debug)]
#[allow(non_snake_case)]
struct FuturesBalance {
	accountAlias: String,
	asset: String,
	availableBalance: String,
	balance: String,
	crossUnPnl: String,
	crossWalletBalance: String,
	marginAvailable: bool,
	maxWithdrawAmount: String,
	updateTime: Number,
}

#[derive(Serialize, Deserialize, Debug)]
#[allow(non_snake_case)]
struct SpotAccountDetails {
	makerCommission: i32,
	takerCommission: i32,
	buyerCommission: i32,
	sellerCommission: i32,
	commissionRates: CommissionRates,
	canTrade: bool,
	canWithdraw: bool,
	canDeposit: bool,
	brokered: bool,
	requireSelfTradePrevention: bool,
	preventSor: bool,
	updateTime: u64,
	accountType: String,
	balances: Vec<SpotBalance>,
	permissions: Vec<String>,
	uid: u64,
}
#[derive(Serialize, Deserialize, Debug)]
struct CommissionRates {
	maker: String,
	taker: String,
	buyer: String,
	seller: String,
}
#[derive(Serialize, Deserialize, Debug)]
struct SpotBalance {
	asset: String,
	free: String,
	locked: String,
}
#[derive(Serialize, Deserialize, Debug)]
#[allow(non_snake_case)]
struct MarginAccountDetails {
	borrowEnabled: bool,
	marginLevel: String,
	CollateralMarginLevel: String,
	totalAssetOfBtc: String,
	totalLiabilityOfBtc: String,
	totalNetAssetOfBtc: String,
	TotalCollateralValueInUSDT: String,
	tradeEnabled: bool,
	transferEnabled: bool,
	accountType: String,
	userAssets: Vec<MarginUserAsset>,
}

#[derive(Serialize, Deserialize, Debug)]
#[allow(non_snake_case)]
struct MarginUserAsset {
	asset: String,
	borrowed: String,
	free: String,
	interest: String,
	locked: String,
	netAsset: String,
}

// FuturesExchangeInfo structs {{{
#[derive(Debug, Deserialize, Serialize)]
#[allow(non_snake_case)]
struct FuturesExchangeInfo {
	exchangeFilters: Vec<String>,
	rateLimits: Vec<RateLimit>,
	serverTime: i64,
	assets: Vec<Value>,
	symbols: Vec<FuturesSymbol>,
	timezone: String,
}
#[derive(Debug, Deserialize, Serialize)]
#[allow(non_snake_case)]
struct RateLimit {
	interval: String,
	intervalNum: u32,
	limit: u32,
	rateLimitType: String,
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
#[allow(non_snake_case)]
struct FuturesSymbol {
	symbol: String,
	pair: String,
	contractType: String,
	deliveryDate: i64,
	onboardDate: i64,
	status: String,
	maintMarginPercent: String,
	requiredMarginPercent: String,
	baseAsset: String,
	quoteAsset: String,
	marginAsset: String,
	pricePrecision: u32,
	quantityPrecision: usize,
	baseAssetPrecision: u32,
	quotePrecision: u32,
	underlyingType: String,
	underlyingSubType: Vec<String>,
	settlePlan: u32,
	triggerProtect: String,
	filters: Vec<Value>,
	OrderType: Option<Vec<String>>,
	timeInForce: Vec<String>,
	liquidationFee: String,
	marketTakeBound: String,
}
//,}}}
