use color_eyre::eyre::{Result, eyre};
use futures_util::StreamExt;
use nautilus_bybit::{
	common::enums::{BybitEnvironment, BybitProductType},
	http::client::BybitHttpClient,
	websocket::{client::BybitWebSocketClient, messages::NautilusWsMessage},
};
use nautilus_model::data::Data;
use tokio::{pin, signal};
use tracing::info;

pub async fn start() -> Result<()> {
	// Fetch instrument data via HTTP first
	info!("Fetching BTCUSDT instrument data...");
	let http_client = BybitHttpClient::new(None, Some(60), None, None, None, None, None)?;
	let instruments = http_client
		.request_instruments(BybitProductType::Linear, Some("BTCUSDT".to_string()))
		.await
		.map_err(|e| eyre!("Failed to fetch instruments from Bybit: {e}"))?;

	if instruments.is_empty() {
		return Err(color_eyre::eyre::eyre!("Failed to fetch BTCUSDT instrument"));
	}

	info!("Fetched {} instrument(s)", instruments.len());

	// Create websocket client and cache the instrument
	let mut client = BybitWebSocketClient::new_public_with(BybitProductType::Linear, BybitEnvironment::Mainnet, None, None);

	for instrument in instruments {
		client.cache_instrument(instrument);
	}

	client.connect().await?;
	client.subscribe(vec!["publicTrade.BTCUSDT".to_string()]).await?;

	let stream = client.stream();
	let shutdown = signal::ctrl_c();
	pin!(stream);
	pin!(shutdown);

	info!("Streaming BTC trades from Bybit; press Ctrl+C to exit");

	loop {
		tokio::select! {
			Some(event) = stream.next() => {
				match event {
					NautilusWsMessage::Data(data_vec) => {
						for data in data_vec {
							if let Data::Trade(trade) = data {
								println!(
									"[TRADE] {} | price: {} | size: {} | side: {:?}",
									trade.instrument_id,
									trade.price,
									trade.size,
									trade.aggressor_side
								);
							}
						}
					}
					NautilusWsMessage::Error(err) => {
						tracing::error!(code = err.code, message = %err.message, "websocket error");
					}
					NautilusWsMessage::Reconnected => {
						tracing::warn!("WebSocket reconnected");
					}
					_ => {}
				}
			}
			_ = &mut shutdown => {
				info!("Received Ctrl+C, closing connection");
				client.close().await?;
				break;
			}
			else => break,
		}
	}

	Ok(())
}
