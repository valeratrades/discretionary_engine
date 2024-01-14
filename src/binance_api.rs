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

#[derive(Serialize, Deserialize, Debug)]
#[allow(non_snake_case)]
struct FapiBalance {
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

type HmacSha256 = Hmac<Sha256>;

pub async fn signed_request(endpoint_str: &str, key: String, secret: String) -> Result<reqwest::Response> {
	dbg!(&endpoint_str);
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
	dbg!(&url);

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
			Market::Margin => Url::parse("https://dapi.binance.com/").unwrap(),
		}
	}
}

pub async fn get_balance(key: String, secret: String, market: Market) -> Result<f32> {
	let base_url = market.get_base_url();
	//TODO!!!: do cases for different markets \
	let url = base_url.join("fapi/v2/balance")?;

	let r = signed_request(url.as_str(), key, secret).await?;
	//let json_r: Value = request().await.unwrap().json().await.unwrap();
	dbg!(&r);
	let asset_balances: Vec<FapiBalance> = r.json().await?;

	let mut total_balance = 0.0;
	for asset in asset_balances {
		total_balance += asset.balance.parse::<f32>()?;
	}
	Ok(total_balance)
}
