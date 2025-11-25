use std::sync::Arc;

use color_eyre::eyre::{Context, Result, bail};
use nautilus_bybit::{
	common::enums::{BybitPositionSide, BybitProductType},
	http::query::BybitPositionListParamsBuilder,
};
use tracing::info;
use v_exchanges::Ticker;
use v_utils::{log, trades::Timeframe};

use crate::{bybit_common::*, config::AppConfig};

#[derive(clap::Args, Debug)]
pub(crate) struct NukeArgs {
	/// Ticker to close position for.
	ticker: Ticker,

	/// Optional duration over which to close the position (for MM trailing strategy)
	#[arg(short, long)]
	duration: Option<Timeframe>,
}

pub(crate) async fn main(args: NukeArgs, config: Arc<AppConfig>, testnet: bool) -> Result<()> {
	log!("Nuke command for ticker: {:?}", args.ticker);

	if let Some(duration) = args.duration {
		log!("Duration: {:?} (MM trailing strategy - TODO)", duration);
		// TODO: Implement MM trailing strategy
		unimplemented!("MM trailing strategy not yet implemented");
	} else {
		// Market close: get current position and close it using nautilus client
		log!("Closing position with market order");

		// Create Bybit HTTP client
		let (_raw_client, client) = create_bybit_clients(&config, args.ticker.exchange_name, testnet)?;

		// Convert symbol format (twt-usdt.p -> TWTUSDT)
		let symbol_raw = args.ticker.symbol.to_string();
		let symbol = convert_symbol_to_bybit(&symbol_raw);

		// Get current position
		let params = BybitPositionListParamsBuilder::default()
			.category(BybitProductType::Linear)
			.symbol(symbol.clone())
			.build()
			.context("Failed to build position list params")?;

		let position_response = client.get_positions(&params).await.context("Failed to fetch positions")?;

		if position_response.result.list.is_empty() {
			log!("No position to close for {}", symbol);
			return Ok(());
		}

		let position = &position_response.result.list[0];
		let position_size: f64 = position.size.parse().context("Failed to parse position size")?;

		if position_size == 0.0 {
			log!("No position to close for {}", symbol);
			return Ok(());
		}

		log!("Current position: {:?} {} {}", position.side, position_size, symbol);

		// Determine order side (opposite of position side)
		let order_side = if position.side == BybitPositionSide::Buy { "Sell" } else { "Buy" };

		// Place reduce-only market order to close
		let order_request = serde_json::json!({
			"category": "linear",
			"symbol": symbol,
			"side": order_side,
			"orderType": "Market",
			"qty": position_size.to_string(),
			"timeInForce": "IOC",
			"orderLinkId": format!("nuke-{}", uuid::Uuid::new_v4()),
			"reduceOnly": true,
		});

		log!("Submitting market {} order to close {} {}", order_side, position_size, symbol);

		let order_response = client.place_order(&order_request).await.context("Failed to place order")?;

		if order_response.ret_code == 0 {
			println!("✅ Position closed successfully!");
			if let Some(order_id) = order_response.result.order_id {
				println!("   Order ID: {}", order_id);
			}
			println!("   Closed: {} {}", position_size, symbol);
			Ok(())
		} else {
			bail!("Order failed: {} (code: {})", order_response.ret_msg, order_response.ret_code);
		}
	}
}
