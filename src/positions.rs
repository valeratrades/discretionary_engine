use crate::api::order_types::{Order, OrderP, OrderStuff};
use crate::api::{binance, Symbol};
use crate::protocols::{FollowupProtocol, ProtocolType};
use anyhow::Result;
use std::collections::HashMap;
use std::str::FromStr;
use tracing::{info, instrument};
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
	protocols_spec: Option<String>, //Vec<AcquisitionProtocol>,
}
impl PositionAcquisition {
	pub async fn dbg_new(spec: PositionSpec) -> Result<Self> {
		Ok(Self {
			_spec: spec,
			target_notional: 10.0,
			acquired_notional: 10.0,
			protocols_spec: None,
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
				current_state.acquired_notional += order_notional;
				break;
			}
		}

		Ok(current_state)
	}
}

#[derive(Debug)]
pub struct PositionFollowup {
	_acquisition: PositionAcquisition,
	protocols_spec: Vec<FollowupProtocol>,
	open_orders: OpenOrders,
	closed_notional: f64,
}
#[derive(Debug)]
struct OpenOrders {
	normal: f64,
	stop: f64,
}
/// Internal representation of desired orders. The actual orders are synchronized to this, so any details of actual execution are mostly irrelevant.
struct Orders {
	total_notional: f64,
	//total_usd: f64,
	orders: Vec<Order>,
}

impl PositionFollowup {
	#[instrument]
	pub async fn do_followup(acquired: PositionAcquisition, protocols: Vec<FollowupProtocol>) -> Result<Self> {
		let mut counted_subtypes: HashMap<ProtocolType, usize> = HashMap::new();
		for protocol in &protocols {
			let subtype = protocol.get_subtype();
			*counted_subtypes.entry(subtype).or_insert(0) += 1;
		}

		let (tx_orders, rx_orders) = std::sync::mpsc::channel::<(Vec<OrderP>, String)>();
		for protocol in protocols {
			protocol.attach(tx_orders.clone(), &acquired._spec)?;
		}

		let mut all_requested: HashMap<String, Vec<Order>> = HashMap::new();
		let mut closed_notional = 0.0;

		while let Ok((order_batch, uid)) = rx_orders.recv() {
			let protocol = FollowupProtocol::from_str(&uid).unwrap();
			let subtype = protocol.get_subtype();
			let size_multiplier = 1.0 / *counted_subtypes.get(&subtype).unwrap() as f64;
			let total_controlled_size = acquired.acquired_notional * size_multiplier;

			let order_batch = order_batch
				.into_iter()
				.map(|o| o.to_exact(total_controlled_size, uid.clone()))
				.collect::<Vec<Order>>();

			all_requested.insert(uid, order_batch);

			let mut stop_orders = Vec::new();
			let mut normal_orders = Vec::new();
			let mut market_orders = Vec::new();
			for (key, value) in all_requested.iter() {
				value.into_iter().for_each(|o| match o.is_stop_order() {
					Some(__s) => match __s {
						true => stop_orders.push(o),
						false => normal_orders.push(o),
					},
					None => market_orders.push(o),
				});
			}
		}

		let left_notional = acquired.acquired_notional - closed_notional;
		// both stop and normal orders that we choose should add up to this indiviudally. If any on the border - we cut it.

		// all market orders shall be executed immediately, before others are processed

		// direction of sorting is determined by the side of the position
		// if Buy, sort stop orders in descending order, if Sell, sort stop orders in ascending order. Opposite for normal orders.
		match acquired._spec.side {
			Side::Buy => {
				stop_orders.sort_by(|a, b| b.price().unwrap().partial_cmp(&a.price().unwrap()).unwrap());
				normal_orders.sort_by(|a, b| a.price().unwrap().partial_cmp(&b.price().unwrap()).unwrap());
			}
			Side::Sell => {
				stop_orders.sort_by(|a, b| a.price().unwrap().partial_cmp(&b.price().unwrap()).unwrap());
				normal_orders.sort_by(|a, b| b.price().unwrap().partial_cmp(&a.price().unwrap()).unwrap());
			}
		}

		let full_key = std::env::var("BINANCE_TIGER_FULL_KEY").unwrap();
		let full_secret = std::env::var("BINANCE_TIGER_FULL_SECRET").unwrap();

		//let _ = binance::post_futures_orders(full_key.clone(), full_secret.clone(), orders).await?;

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
