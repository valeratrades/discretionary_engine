#![allow(non_snake_case, dead_code)]
pub mod info;
mod orders;
use crate::config::AppConfig;
use crate::exchange_apis::order_types::Order;
use crate::exchange_apis::Market;
use crate::PositionOrderId;
use anyhow::Result;
use v_utils::io::confirm;
use chrono::Utc;
use hmac::{Hmac, Mac};
pub use info::FUTURES_EXCHANGE_INFO;
pub use orders::*;
use reqwest::header::{HeaderMap, HeaderValue, CONTENT_TYPE};
use serde::{Deserialize, Serialize};
use serde_json::Number;
use serde_json::Value;
use sha2::Sha256;
use std::collections::HashMap;
use std::sync::{Arc, RwLock};
use tokio::select;
use tracing::info;
use url::Url;
use uuid::Uuid;

use super::order_types::IdRequirements;
use super::HubCallback;
use super::HubPassforward;
type HmacSha256 = Hmac<Sha256>;

#[allow(dead_code)]
pub struct Binance {
	// And so then many calls will be replaced with just finding info here.
	futures_symbols: HashMap<String, FuturesSymbol>,
}

pub async fn signed_request<S: AsRef<str>>(
	http_method: reqwest::Method,
	endpoint_str: &str,
	mut params: HashMap<&'static str, String>,
	key: S,
	secret: S,
) -> Result<reqwest::Response> {
	let mut headers = HeaderMap::new();
	headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/json;charset=utf-8"));
	headers.insert("X-MBX-APIKEY", HeaderValue::from_str(key.as_ref()).unwrap());
	let client = reqwest::Client::builder().default_headers(headers).build()?;

	let time_ms = Utc::now().timestamp_millis();
	params.insert("timestamp", format!("{}", time_ms));

	let query_string = serde_urlencoded::to_string(&params)?;

	let mut mac = HmacSha256::new_from_slice(secret.as_ref().as_bytes()).unwrap();
	mac.update(query_string.as_bytes());
	let mac_bytes = mac.finalize().into_bytes();
	let signature = hex::encode(mac_bytes);

	let url = format!("{}?{}&signature={}", endpoint_str, query_string, signature);
	let r = client.request(http_method, &url).send().await?;

	Ok(r)
}

pub async fn get_balance(key: String, secret: String, market: Market) -> Result<f64> {
	let params = HashMap::<&str, String>::new();
	match market {
		Market::BinanceFutures => {
			let base_url = market.get_base_url();
			let url = base_url.join("fapi/v2/balance")?;

			let r = signed_request(reqwest::Method::GET, url.as_str(), params, key, secret).await?;
			let asset_balances: Vec<FuturesBalance> = r.json().await?;

			let mut total_balance = 0.0;
			for asset in asset_balances {
				total_balance += asset.balance.parse::<f64>()?;
			}
			Ok(total_balance)
		}
		Market::BinanceSpot => {
			let base_url = market.get_base_url();
			let url = base_url.join("/api/v3/account")?;

			let r = signed_request(reqwest::Method::GET, url.as_str(), params, key, secret).await?;
			let account_details: SpotAccountDetails = r.json().await?;
			let asset_balances = account_details.balances;

			let mut total_balance = 0.0;
			for asset in asset_balances {
				total_balance += asset.free.parse::<f64>()?;
				total_balance += asset.locked.parse::<f64>()?;
			}
			Ok(total_balance)
		}
		Market::BinanceMargin => {
			let base_url = market.get_base_url();
			let url = base_url.join("/sapi/v1/margin/account")?;

			let r = signed_request(reqwest::Method::GET, url.as_str(), params, key, secret).await?;
			let account_details: MarginAccountDetails = r.json().await?;
			let total_balance: f64 = account_details.TotalCollateralValueInUSDT.parse()?;

			Ok(total_balance)
		}
	}
}

pub async fn futures_price(asset: &str) -> Result<f64> {
	let symbol = crate::exchange_apis::Symbol {
		base: asset.to_string(),
		quote: "USDT".to_string(),
		market: Market::BinanceFutures,
	};
	let base_url = Market::BinanceFutures.get_base_url();
	let url = base_url.join("/fapi/v2/ticker/price")?;

	let mut params = HashMap::<&str, String>::new();
	params.insert("symbol", symbol.to_string());

	let client = reqwest::Client::new();
	let r = client.get(url).json(&params).send().await?;
	//let r_json: serde_json::Value = r.json().await?;
	//let price = r_json.get("price").unwrap().as_str().unwrap().parse::<f64>()?;
	// for some reason, can't sumbit with the symbol, so effectively requesting all for now
	let prices: Vec<serde_json::Value> = r.json().await?;
	let price = prices
		.iter()
		.find(|x| *x.get("symbol").unwrap().as_str().unwrap() == symbol.to_string())
		.unwrap()
		.get("price")
		.unwrap()
		.as_str()
		.unwrap()
		.parse::<f64>()?;
	
	Ok(price)
}

pub async fn close_orders(key: String, secret: String, orders: Vec<BinanceOrder>) -> Result<()> {
	let base_url = Market::BinanceFutures.get_base_url();
	let url = base_url.join("/fapi/v1/order").unwrap();

	let handles = orders.iter().map(|o| {
		let mut params = HashMap::<&str, String>::new();
		params.insert("symbol", o.symbol.clone());
		params.insert("orderId", o.binance_id.unwrap().to_string());
		signed_request(reqwest::Method::DELETE, url.as_str(), params, key.clone(), secret.clone())
	});
	for handle in handles {
		let _ = handle.await?;
	}

	Ok(())
}

pub async fn get_futures_positions(key: String, secret: String) -> Result<HashMap<String, f64>> {
	let url = FuturesAllPositionsResponse::get_url();

	let r = signed_request(reqwest::Method::GET, url.as_str(), HashMap::new(), key, secret).await?;
	let positions: Vec<FuturesAllPositionsResponse> = r.json().await?;

	let mut positions_map = HashMap::<String, f64>::new();
	for position in positions {
		let symbol = position.symbol.clone();
		let qty = position.positionAmt.parse::<f64>()?;
		positions_map.entry(symbol).and_modify(|e| *e += qty).or_insert(qty);
	}
	Ok(positions_map)
}

/// Returns (price_precision, quantity_precision)
pub async fn futures_precisions(coin: &str) -> Result<(u32, usize)> {
	let base_url = Market::BinanceFutures.get_base_url();
	let url = base_url.join("/fapi/v1/exchangeInfo")?;
	let symbol_str = format!("{}USDT", coin.to_uppercase());

	let r = reqwest::get(url).await?;
	let futures_exchange_info: info::FuturesExchangeInfo = r.json().await?;
	let symbol_info = futures_exchange_info.symbols.iter().find(|x| x.symbol == symbol_str).unwrap();

	Ok((symbol_info.pricePrecision, symbol_info.quantityPrecision))
}

pub async fn post_futures_order<S: AsRef<str>, Id: IdRequirements>(key: S, secret: S, order: &Order<Id>) -> Result<BinanceOrder> {
	let url = FuturesPositionResponse::get_url();

	let mut binance_order = BinanceOrder::from_standard(order).await;
	let params = binance_order.to_params();

	let r = signed_request(reqwest::Method::POST, url.as_str(), params, key, secret).await?;
	dbg!(&r);
	let __why_text_fn_consumes_self = format!("{:?}", r);
	let response: FuturesPositionResponse = match r.json().await {
		Ok(r) => r,
		Err(e) => {
			println!("Error: {:?}", e);
			println!("Response: {:?}", __why_text_fn_consumes_self);
			return Err(e.into());
		}
	};
	binance_order.binance_id = Some(response.orderId);
	Ok(binance_order)
}

/// Normally, the only cases where the return from this poll is going to be _reacted_ to, is when response.status == OrderStatus::Filled or an error is returned.
//TODO!: translate to websockets
pub async fn poll_futures_order<S: AsRef<str>>(key: S, secret: S, binance_order: &BinanceOrder) -> Result<FuturesPositionResponse> {
	let url = FuturesPositionResponse::get_url();

	let mut params = HashMap::<&str, String>::new();
	params.insert("symbol", binance_order.symbol.to_string());
	params.insert("orderId", format!("{}", &binance_order.binance_id.unwrap()));

	let r = signed_request(reqwest::Method::GET, url.as_str(), params, key, secret).await?;
	let response: FuturesPositionResponse = r.json().await?;
	Ok(response)
}

/// Binance wants both qty and price in orders to always respect the minimum step of the price
//TODO!!!: Store all needed exchange info locally
pub async fn apply_price_precision(coin: &str, price: f64) -> Result<f64> {
	let (price_precision, _) = futures_precisions(coin).await?;
	let factor = 10_f64.powi(price_precision as i32);
	let adjusted = (price * factor).round() / factor;
	Ok(adjusted)
}

pub async fn apply_quantity_precision(coin: &str, qty: f64) -> Result<f64> {
	let (_, qty_precision) = futures_precisions(coin).await?;
	let factor = 10_f64.powi(qty_precision as i32);
	let adjusted = (qty * factor).round() / factor;
	Ok(adjusted)
}

pub async fn dirty_hardcoded_exec<Id: IdRequirements>(order: Order<Id>, config: &AppConfig) -> Result<()> {
	assert!(order.qty_notional > 0.0);
	//FIXME: works with Market orders but not StopMarket

	let symbol = order.symbol.clone();

	let full_key = config.binance.full_key.clone();
	let full_secret = config.binance.full_secret.clone();

	let order_id = post_futures_order(full_key.clone(), full_secret.clone(), &order).await.unwrap();
	let binance_order = BinanceOrder::from_standard(&order).await;

	//info!(target: "/tmp/discretionary_engine.lock", "placed order: {:?}", order_id);
	loop {
		let r = poll_futures_order(full_key.clone(), full_secret.clone(), &binance_order).await?;
		if r.status == OrderStatus::Filled {
			let order_notional = r.origQty.parse::<f64>()?;
			info!("Order filled: {:?}", order_notional);
			break;
		}
	}

	Ok(())
}

///NB: must be communicating back to the hub, can't shortcut and talk back directly to positions.
pub async fn binance_runtime(
	config: AppConfig,
	hub_callback: tokio::sync::mpsc::Sender<HubCallback>,
	mut hub_rx: tokio::sync::watch::Receiver<HubPassforward>,
) {
	println!("dbg: binance_runtime started"); //dbg
	let full_key = config.binance.full_key.clone();
	let full_secret = config.binance.full_secret.clone();
	let currently_deployed: Arc<RwLock<Vec<BinanceOrder>>> = Arc::new(RwLock::new(Vec::new()));

	let mut last_received_fill_key = Uuid::new_v4();
	let mut last_processed_fill_key = last_received_fill_key;

	let (local_fills_tx, mut local_fills_rx) = tokio::sync::mpsc::channel(100);
	let currently_deployed_clone = currently_deployed.clone();
	let (full_key_clone, full_secret_clone) = (full_key.clone(), full_secret.clone());
	tokio::spawn(async move {
		//TODO!!!: make a websocket
		loop {
			tokio::time::sleep(std::time::Duration::from_secs(5)).await;
			println!("dbg: tik-tok"); //dbg

			let orders: Vec<_> = {
				let currently_deployed_read = currently_deployed_clone.read().unwrap();
				currently_deployed_read.iter().cloned().collect()
			};
			for order in orders {
				let r = poll_futures_order(&full_key_clone, &full_secret_clone, &order).await.unwrap();
				if r.status == OrderStatus::Filled {
					last_received_fill_key = Uuid::new_v4();
					local_fills_tx.send((last_received_fill_key, order.clone(), r.clone())).await.unwrap();
				}
			}
		}
	});

	loop {
		select! {
			Ok(_) = hub_rx.changed(), if last_received_fill_key == last_processed_fill_key => {
				let target_orders: Vec<Order<PositionOrderId>>;
				{
					let hub_passforward = hub_rx.borrow();
					last_received_fill_key = hub_passforward.key; //dbg
					target_orders = match hub_passforward.key == last_received_fill_key {
						true => hub_passforward.orders.clone(), //?  take()
						false => {
							continue;
						},
					};
				}
				dbg!(&target_orders, &currently_deployed);

				last_processed_fill_key = last_received_fill_key; //dbg

				let currently_deployed_clone;
				{
					currently_deployed_clone = currently_deployed.read().unwrap().clone();
				}
				close_orders(full_key.clone(), full_secret.clone(), currently_deployed_clone).await.unwrap();

				let mut just_deployed = Vec::new();
				for o in target_orders {
					let b = post_futures_order(full_key.clone(), full_secret.clone(), &o).await.unwrap();
					just_deployed.push(b);
				}
				{
					let mut current_lock = currently_deployed.write().unwrap();
					*current_lock = just_deployed;
				}

				if !confirm("step") {
					break;
				}
			},
			
			// this doesn't have to be async. But fucking select! macro has its own mini-language brewing which I ain't learning.
			_ = async {
				while let Ok(fills) = local_fills_rx.try_recv() {
					last_processed_fill_key = fills.0;
					println!("Fills: {:?} on order: {:?}", fills.1, fills.2);
				}
				tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
			} => {},
		}
	}
	// Later on we will be devising a strategy of transefing current orders to the new target, but for now all orders are simply closed than target ones are opened.
	//Binance docs: currently only LIMIT order modification is supported

	//- must be aware of all active binance orders

	//- ability to modify and poll currently oustanding orders
}

//=============================================================================
// Response structs {{{
//=============================================================================

#[derive(Serialize, Deserialize, Debug, PartialEq, Clone)]
pub enum OrderStatus {
	#[serde(rename = "NEW")]
	New,
	#[serde(rename = "PARTIALLY_FILLED")]
	PartiallyFilled,
	#[serde(rename = "FILLED")]
	Filled,
	#[serde(rename = "CANCELED")]
	Canceled,
	#[serde(rename = "EXPIRED")]
	Expired,
	#[serde(rename = "EXPIRED_IN_MATCH")]
	ExpiredInMatch,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct FuturesPositionResponse {
	pub clientOrderId: Option<String>,
	pub cumQty: Option<String>,
	pub cumQuote: String,
	pub executedQty: String,
	pub orderId: i64,
	pub avgPrice: Option<String>,
	pub origQty: String,
	pub price: String,
	pub reduceOnly: Value,
	pub side: String,
	pub positionSide: Option<String>, // only sent when in hedge mode
	pub status: OrderStatus,
	pub stopPrice: String,
	pub closePosition: Value,
	pub symbol: String,
	pub timeInForce: String,
	pub r#type: String,
	pub origType: String,
	pub activatePrice: Option<f64>, // only returned on TRAILING_STOP_MARKET order
	pub priceRate: Option<f64>,     // only returned on TRAILING_STOP_MARKET order
	pub updateTime: i64,
	pub workingType: Option<String>, // no clue what this is
	pub priceProtect: bool,
	pub priceMatch: Option<String>, // huh
	pub selfTradePreventionMode: Option<String>,
	pub goodTillDate: Option<i64>,
}
impl FuturesPositionResponse {
	pub fn get_url() -> Url {
		let base_url = Market::BinanceFutures.get_base_url();
		// the way this works - is we sumbir "New" and "Query" to the same endpoint. The action is then determined by the presence of the orderId parameter.
		base_url.join("/fapi/v1/order").unwrap()
	}
}

#[derive(Serialize, Deserialize, Debug)]
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
struct SpotAccountDetails {
	makerCommission: f64,
	takerCommission: f64,
	buyerCommission: f64,
	sellerCommission: f64,
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
struct MarginUserAsset {
	asset: String,
	borrowed: String,
	free: String,
	interest: String,
	locked: String,
	netAsset: String,
}

#[derive(Debug, Deserialize, Serialize)]
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

#[derive(Deserialize, Debug)]
struct FuturesAllPositionsResponse {
	entryPrice: String,
	breakEvenPrice: String,
	marginType: String,
	isAutoAddMargin: Value,
	isolatedMargin: String,
	leverage: String,
	liquidationPrice: String,
	markPrice: String,
	maxNotionalValue: String,
	positionAmt: String,
	notional: String,
	isolatedWallet: String,
	symbol: String,
	unRealizedProfit: String,
	positionSide: Value, // is "BOTH" in standard (non-hedge mode) requests, because designed by fucking morons. Apparently we now have negative values in `positionAmt`, if short.
	updateTime: i64,
}
impl FuturesAllPositionsResponse {
	pub fn get_url() -> Url {
		let base_url = Market::BinanceFutures.get_base_url();
		base_url.join("/fapi/v2/positionRisk").unwrap()
	}
}
