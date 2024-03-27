use crate::api::{binance, Symbol};
use crate::protocols::FollowupProtocol;
use crate::protocols::{ProtocolCache, TrailingStop, TrailingStopCache};
use anyhow::Result;
use std::str::FromStr;
use std::sync::{Arc, Mutex};
use tracing;
use tracing::info;
use v_utils::trades::Side;

/// What the Position _*is*_
#[derive(Debug, Clone)]
pub struct PositionSpec {
	pub asset: String,
	pub side: Side,
	pub size_usdt: f64,
}
impl PositionSpec {
	pub fn new(asset: String, side: Side, size_usdt: f64) -> Self {
		Self { asset, side, size_usdt }
	}
}

#[derive(Debug)]
pub struct PositionAcquisition {
	_spec: PositionSpec,
	target_notional: f64,
	acquired_notional: f64,
	protocols_spec: Option<String>, //AcquisitionProtocolsSpec,
	cache: Option<String>,          //AcquisitionCache,
}
impl PositionAcquisition {
	pub async fn dbg_new(spec: PositionSpec) -> Result<Self> {
		Ok(Self {
			_spec: spec,
			target_notional: 0.0,
			acquired_notional: 0.0,
			protocols_spec: None,
			cache: None,
		})
	}

	pub async fn do_acquisition(spec: PositionSpec) -> Result<Self> {
		// is this not in config?
		let full_key = std::env::var("BINANCE_TIGER_FULL_KEY").unwrap();
		let full_secret = std::env::var("BINANCE_TIGER_FULL_SECRET").unwrap();
		//let position = Position::new(Market::BinanceFutures, side, symbol.clone(), usdt_quantity, protocols, Utc::now());
		let coin = spec.asset.clone();
		let symbol = Symbol::from_str(format!("{coin}-USDT-BinanceFutures").as_str())?;
		info!(coin);

		let current_price_handler = binance::futures_price(&coin);
		let quantity_percision_handler = binance::futures_quantity_precision(&coin);
		let current_price = current_price_handler.await?;
		let quantity_precision: usize = quantity_percision_handler.await?;
		let factor = 10_f64.powi(quantity_precision as i32);
		let coin_quantity = spec.size_usdt / current_price;
		let coin_quantity_adjusted = (coin_quantity * factor).round() / factor;

		let mut current_state = Self {
			_spec: spec.clone(),
			target_notional: coin_quantity_adjusted,
			acquired_notional: 0.0,
			protocols_spec: None,
			cache: None,
		};

		let order_id = binance::post_futures_order(
			full_key.clone(),
			full_secret.clone(),
			"MARKET".to_string(),
			symbol.to_string(),
			spec.side.clone(),
			coin_quantity_adjusted,
		)
		.await?;
		//info!(target: "/tmp/discretionary_engine.lock", "placed order: {:?}", order_id);
		loop {
			let order = binance::poll_futures_order(full_key.clone(), full_secret.clone(), order_id.clone(), symbol.to_string()).await?;
			if order.status == binance::OrderStatus::Filled {
				let order_notional = order.origQty.parse::<f64>()?;
				let order_usdt = order.avgPrice.unwrap().parse::<f64>()? * order_notional;
				current_state.acquired_notional += order_usdt;
				break;
			}
		}

		Ok(current_state)
	}
}

#[derive(Debug)]
pub struct PositionFollowup {
	_acquisition: PositionAcquisition,
	protocols_spec: Option<String>, //FollowupProtocolsSpec,
	cache: Option<String>,          //FollowupCache,
}

impl PositionFollowup {
	pub async fn do_followup<T: FollowupProtocol>(acquired: PositionAcquisition, protocols: Vec<T>) -> Result<Self> {
		println!("{:?}", protocols);

		match protocols[0].as_any().downcast_ref::<TrailingStop>() {
			Some(trailing_stop_protocol) => {
				let cache = TrailingStopCache::build(&acquired._spec).await?;
				let orders = Vec::new();
				trailing_stop_protocol.attach(Arc::new(Mutex::new(orders)), Arc::new(Mutex::new(cache))).await?;
			}
			None => panic!(),
		}

		Ok(Self {
			_acquisition: acquired,
			protocols_spec: None,
			cache: None,
		})
	}
}

//pub struct PositionClosed {
//	_followup: PositionFollowup,
//	t_closed: DateTime<Utc>,
//}
