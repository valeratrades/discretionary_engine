use std::sync::Arc;

use color_eyre::eyre::{Context, Result, bail};
use hmac::{Hmac, Mac};
use reqwest::Client;
use secrecy::ExposeSecret;
use serde::{Deserialize, Serialize};
use sha2::Sha256;
use tracing::info;
use v_exchanges::Ticker;
use v_utils::{log, trades::Timeframe};

use crate::config::AppConfig;

type HmacSha256 = Hmac<Sha256>;

#[derive(clap::Args, Debug)]
#[command(group(
    clap::ArgGroup::new("size_group")
        .required(true)
        .args(["quote", "notional", "size"]),
))]
pub(crate) struct AdjustPosArgs {
	/// Ticker to adjust position for.
	ticker: Ticker,

	/// Size in quote currency.
	#[arg(short = 'q', long)]
	quote: Option<f64>,

	/// Size in notional (USD)
	#[arg(short = 'n', long)]
	notional: Option<f64>,

	/// Size with suffix inference: "$" for USD, asset name (e.g., "BTC") for that asset, or plain number for quote
	#[arg(short = 's', long)]
	size: Option<String>,

	/// timeframe, in the format of "1m", "1h", "3M", etc.
	/// determines the target period for which we expect the edge to persist.
	#[arg(short, long)]
	tf: Option<Timeframe>,

	/// Reduce-only mode: only reduce existing position, don't increase it
	#[arg(long)]
	reduce: bool,

	/// Use testnet instead of mainnet
	#[arg(long)]
	testnet: bool,
}

#[derive(Debug, Serialize)]
struct BybitOrderRequest {
	category: String,
	symbol: String,
	side: String,
	#[serde(rename = "orderType")]
	order_type: String,
	qty: String,
	#[serde(rename = "timeInForce")]
	time_in_force: String,
	#[serde(rename = "orderLinkId")]
	order_link_id: String,
	#[serde(rename = "reduceOnly")]
	reduce_only: bool,
}

#[derive(Debug, Deserialize)]
struct BybitResponse {
	#[serde(rename = "retCode")]
	ret_code: i32,
	#[serde(rename = "retMsg")]
	ret_msg: String,
	result: Option<serde_json::Value>,
}

/// Convert v_exchanges symbol format to Bybit format
/// Examples:
/// - "twt-usdt.p" -> "TWTUSDT"
/// - "btc-usdt" -> "BTCUSDT"
/// - "eth-usdt.p" -> "ETHUSDT"
fn convert_symbol_to_bybit(symbol: &str) -> String {
	// Remove the ".p" suffix if present (perpetual futures marker)
	let without_suffix = symbol.split('.').next().unwrap_or(symbol);

	// Remove hyphens and convert to uppercase
	without_suffix.replace('-', "").to_uppercase()
}

/// Round quantity to the appropriate step size
fn round_to_step(value: f64, step: f64) -> f64 {
	(value / step).round() * step
}

/// Sign Bybit API request
fn sign_request(api_secret: &str, timestamp: &str, api_key: &str, recv_window: &str, params: &str) -> String {
	let sign_str = format!("{}{}{}{}", timestamp, api_key, recv_window, params);
	let mut mac = HmacSha256::new_from_slice(api_secret.as_bytes()).expect("HMAC can take key of any size");
	mac.update(sign_str.as_bytes());
	hex::encode(mac.finalize().into_bytes())
}

pub(crate) async fn main(args: AdjustPosArgs, config: Arc<AppConfig>) -> Result<()> {
	info!("Starting adjust-pos for ticker: {:?}", args.ticker);

	// Determine whether we have a quote amount (quantity) or notional amount (USD)
	enum SizeType {
		Quote(f64),    // Actual quantity of asset
		Notional(f64), // USD value
	}

	let size_type = if let Some(notional) = args.notional {
		SizeType::Notional(notional)
	} else if let Some(quote) = args.quote {
		SizeType::Quote(quote)
	} else if let Some(size_str) = args.size {
		// Parse size with suffix inference
		if size_str.ends_with('$') {
			// Strip $ and parse as USD
			let usd = size_str.trim_end_matches('$').parse::<f64>().context("Failed to parse USD amount from --size")?;
			SizeType::Notional(usd)
		} else if let Some(pos) = size_str.chars().position(|c| c.is_alphabetic()) {
			// Has a suffix like "BTC", "ETH", etc.
			let (number_part, asset_part) = size_str.split_at(pos);
			let amount = number_part.parse::<f64>().context("Failed to parse amount from --size")?;
			// TODO: Handle conversion from other assets to USD
			bail!("Asset conversion not yet implemented. Got {} {}, need to convert to USD", amount, asset_part);
		} else {
			// Plain number - treat as quote currency (actual quantity)
			let qty = size_str.parse::<f64>().context("Failed to parse --size as number")?;
			SizeType::Quote(qty)
		}
	} else {
		bail!("No size specified");
	};

	// Get exchange config based on ticker's exchange
	let exchange_config = config.get_exchange(args.ticker.exchange_name)?;

	// Select base URL based on testnet flag
	let base_url = if args.testnet {
		info!("Using TESTNET");
		"https://api-testnet.bybit.com"
	} else {
		info!("Using MAINNET");
		"https://api.bybit.com"
	};

	// Get current ticker price first
	let symbol_raw = args.ticker.symbol.to_string();
	let symbol = convert_symbol_to_bybit(&symbol_raw);
	info!("Fetching current price for {} (converted from {})", symbol, symbol_raw);

	let client = Client::new();
	let ticker_url = format!("{}/v5/market/tickers?category=linear&symbol={}", base_url, symbol);
	let ticker_response: serde_json::Value = client
		.get(&ticker_url)
		.send()
		.await
		.context("Failed to fetch ticker data")?
		.json()
		.await
		.context("Failed to parse ticker response as JSON")?;

	info!("Ticker response: {}", serde_json::to_string_pretty(&ticker_response)?);

	let current_price: f64 = ticker_response["result"]["list"][0]["lastPrice"]
		.as_str()
		.ok_or_else(|| color_eyre::eyre::eyre!("Failed to get current price from response: {}", ticker_response))?
		.parse()
		.context("Failed to parse price as float")?;

	info!("Current price: ${}", current_price);

	// Fetch instrument info to get lot size filter
	let instruments_url = format!("{}/v5/market/instruments-info?category=linear&symbol={}", base_url, symbol);
	let instruments_response: serde_json::Value = client
		.get(&instruments_url)
		.send()
		.await
		.context("Failed to fetch instrument info")?
		.json()
		.await
		.context("Failed to parse instrument info")?;

	let lot_size_filter = &instruments_response["result"]["list"][0]["lotSizeFilter"];
	let qty_step: f64 = lot_size_filter["qtyStep"].as_str().ok_or_else(|| color_eyre::eyre::eyre!("Failed to get qtyStep"))?.parse()?;
	let min_order_qty: f64 = lot_size_filter["minOrderQty"]
		.as_str()
		.ok_or_else(|| color_eyre::eyre::eyre!("Failed to get minOrderQty"))?
		.parse()?;
	let max_order_qty: f64 = lot_size_filter["maxOrderQty"]
		.as_str()
		.ok_or_else(|| color_eyre::eyre::eyre!("Failed to get maxOrderQty"))?
		.parse()?;

	info!("Instrument info - qtyStep: {}, minOrderQty: {}, maxOrderQty: {}", qty_step, min_order_qty, max_order_qty);

	// Calculate quantity based on size type, extracting sign for order side
	let (raw_quantity, side) = match size_type {
		SizeType::Quote(qty) => (qty, if qty >= 0.0 { "Buy" } else { "Sell" }),
		SizeType::Notional(usd) => {
			let qty = usd / current_price;
			(qty, if usd >= 0.0 { "Buy" } else { "Sell" })
		}
	};

	// Work with absolute value for rounding
	let abs_raw_qty = raw_quantity.abs();
	let quantity = round_to_step(abs_raw_qty, qty_step);

	let actual_notional = quantity * current_price;
	log!("{} order: {:.6} -> rounded to {:.6} (notional: ${:.2})", side, abs_raw_qty, quantity, actual_notional);

	// Format quantity properly based on step size
	let qty_str = if qty_step >= 1.0 {
		format!("{:.0}", quantity)
	} else if qty_step >= 0.1 {
		format!("{:.1}", quantity)
	} else if qty_step >= 0.01 {
		format!("{:.2}", quantity)
	} else {
		format!("{}", quantity)
	};

	// Prepare order request
	let order_request = BybitOrderRequest {
		category: "linear".to_string(),
		symbol: symbol.clone(),
		side: side.to_string(),
		order_type: "Market".to_string(),
		qty: qty_str.clone(),
		time_in_force: "IOC".to_string(),
		order_link_id: format!("adjust-{}", uuid::Uuid::new_v4()),
		reduce_only: args.reduce,
	};

	let params_json = serde_json::to_string(&order_request)?;
	let timestamp = chrono::Utc::now().timestamp_millis().to_string();
	let recv_window = "5000";

	let signature = sign_request(exchange_config.api_secret.expose_secret(), &timestamp, &exchange_config.api_pubkey, recv_window, &params_json);

	info!("Submitting market buy order for {} {}", quantity, symbol);

	// Submit order
	let order_url = format!("{}/v5/order/create", base_url);
	let response = client
		.post(&order_url)
		.header("X-BAPI-API-KEY", &exchange_config.api_pubkey)
		.header("X-BAPI-TIMESTAMP", &timestamp)
		.header("X-BAPI-SIGN", &signature)
		.header("X-BAPI-RECV-WINDOW", recv_window)
		.header("Content-Type", "application/json")
		.body(params_json)
		.send()
		.await?;

	let bybit_response: BybitResponse = response.json().await?;

	if bybit_response.ret_code == 0 {
		println!("✅ Order submitted successfully!");
		info!("Order submitted successfully!");
		if let Some(result) = bybit_response.result {
			let order_id = result.get("orderId").and_then(|v| v.as_str()).unwrap_or("unknown");
			let order_link_id = result.get("orderLinkId").and_then(|v| v.as_str()).unwrap_or("unknown");
			println!("   Order ID: {}", order_id);
			println!("   Quantity: {} {} (notional: ${:.2})", qty_str, symbol, actual_notional);
			info!("Order ID: {}, Order Link ID: {}", order_id, order_link_id);
		}
		Ok(())
	} else {
		bail!("Order failed: {} (code: {})", bybit_response.ret_msg, bybit_response.ret_code);
	}
}
