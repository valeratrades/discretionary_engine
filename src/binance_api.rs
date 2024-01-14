use anyhow::Result;
use chrono::Utc;
use hmac::{Hmac, Mac};
use reqwest::header::{HeaderMap, HeaderValue, CONTENT_TYPE};
use serde::{Deserialize, Serialize};
use serde_json::Number;
use serde_urlencoded;
use sha2::Sha256;
use std::collections::HashMap;
use url::Url;

type HmacSha256 = Hmac<Sha256>;

pub async fn signed_request(endpoint_str: &str, key: String, secret: String) -> Result<reqwest::Response> {
	let mut headers = HeaderMap::new();
	headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/json;charset=utf-8"));
	headers.insert("X-MBX-APIKEY", HeaderValue::from_str(&key).unwrap());
	let client = reqwest::Client::builder().default_headers(headers).build()?;

	let time_ms = Utc::now().timestamp_millis();
	let mut params = HashMap::<&str, String>::new();
	params.insert("timestamp", format!("{}", time_ms));

	let query_string = serde_urlencoded::to_string(&params)?;

	let mut mac = HmacSha256::new_from_slice(secret.as_bytes()).unwrap();
	mac.update(query_string.as_bytes());
	let mac_bytes = mac.finalize().into_bytes();
	let signature = hex::encode(mac_bytes);

	let url = format!("{}?{}&signature={}", endpoint_str, query_string, signature);

	let r = client.get(&url).send().await?;
	Ok(r)
}

pub enum Market {
	Futures,
	Spot,
	Margin,
}
impl Market {
	fn get_base_url(&self) -> Url {
		match self {
			Market::Futures => Url::parse("https://fapi.binance.com/").unwrap(),
			Market::Spot => Url::parse("https://api.binance.com/").unwrap(),
			Market::Margin => Url::parse("https://api.binance.com/").unwrap(),
		}
	}
}

pub async fn get_balance(key: String, secret: String, market: Market) -> Result<f32> {
	match market {
		Market::Futures => {
			let base_url = market.get_base_url();
			let url = base_url.join("fapi/v2/balance")?;

			let r = signed_request(url.as_str(), key, secret).await?;
			let asset_balances: Vec<FuturesBalance> = r.json().await?;

			let mut total_balance = 0.0;
			for asset in asset_balances {
				total_balance += asset.balance.parse::<f32>()?;
			}
			Ok(total_balance)
		}
		Market::Spot => {
			let base_url = market.get_base_url();
			let url = base_url.join("/api/v3/account")?;

			let r = signed_request(url.as_str(), key, secret).await?;
			let account_details: SpotAccountDetails = r.json().await?;
			let asset_balances = account_details.balances;

			let mut total_balance = 0.0;
			for asset in asset_balances {
				total_balance += asset.free.parse::<f32>()?;
				total_balance += asset.locked.parse::<f32>()?;
			}
			Ok(total_balance)
		}
		Market::Margin => {
			let base_url = market.get_base_url();
			let url = base_url.join("/sapi/v1/margin/account")?;

			let r = signed_request(url.as_str(), key, secret).await?;
			let account_details: MarginAccountDetails = r.json().await?;
			let total_balance: f32 = account_details.TotalCollateralValueInUSDT.parse()?;

			Ok(total_balance)
		}
	}
}

//=============================================================================
// Response structs
//=============================================================================

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
