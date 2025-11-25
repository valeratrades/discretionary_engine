use std::sync::Arc;

use color_eyre::eyre::{Context, Result};
use nautilus_bybit::http::client::{BybitHttpClient, BybitRawHttpClient};
use secrecy::ExposeSecret;
use tracing::info;
use v_exchanges::ExchangeName;

use crate::config::AppConfig;

pub fn convert_symbol_to_bybit(symbol: &str) -> String {
	let without_suffix = symbol.split('.').next().unwrap_or(symbol);
	without_suffix.replace('-', "").to_uppercase()
}

pub fn create_bybit_clients(config: &Arc<AppConfig>, exchange_name: ExchangeName, testnet: bool) -> Result<(BybitRawHttpClient, BybitHttpClient)> {
	let exchange_config = config.get_exchange(exchange_name)?;

	let base_url = if testnet {
		info!("Using TESTNET");
		Some("https://api-testnet.bybit.com".to_string())
	} else {
		info!("Using MAINNET");
		None
	};

	let raw_client = BybitRawHttpClient::new(
		base_url.clone(),
		Some(60), // timeout_secs
		None,     // max_retries
		None,     // retry_delay_ms
		None,     // retry_delay_max_ms
		None,     // recv_window_ms
		None,     // proxy_url
	)
	.context("Failed to create Bybit raw HTTP client")?;

	let client = BybitHttpClient::with_credentials(
		exchange_config.api_pubkey.clone(),
		exchange_config.api_secret.expose_secret().to_string(),
		base_url,
		None, // timeout_secs
		None, // max_retries
		None, // retry_delay_ms
		None, // retry_delay_max_ms
		None, // recv_window_ms
		None, // proxy_url
	)
	.context("Failed to create Bybit HTTP client")?;

	Ok((raw_client, client))
}
