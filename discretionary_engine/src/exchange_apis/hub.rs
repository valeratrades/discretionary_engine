use std::{collections::HashMap, sync::Arc};

use color_eyre::eyre::Result;
use tokio::{sync::mpsc, task::JoinSet};
use uuid::Uuid;

use super::exchanges::Exchanges;
use crate::{
	config::AppConfig,
	exchange_apis::{
		binance, order_types,
		order_types::{ConceptualOrder, ConceptualOrderType, Order, ProtocolOrderId},
		Market,
	},
	positions::PositionCallback,
	protocols::{ProtocolFill, ProtocolFills},
	PositionOrderId,
};

//? is there a conventional way to introduce these communication locks?
#[derive(Clone, Debug, Default, derive_new::new)]
pub struct HubCallback {
	pub key: Uuid,
	pub fill_qty: f64,
	pub order: Order<PositionOrderId>,
}

#[derive(Clone, Debug, Default, derive_new::new)]
pub struct HubPassforward {
	pub key: Uuid,
	pub orders: Vec<Order<PositionOrderId>>,
}

pub fn init_hub(config_arc: Arc<AppConfig>, parent_js: &mut JoinSet<Result<()>>, exchanges: Arc<Exchanges>) -> mpsc::Sender<HubRx> {
	let (tx, rx) = mpsc::channel(32);
	parent_js.spawn(hub(config_arc.clone(), rx, exchanges));
	tx
}

#[derive(Clone, Debug, derive_new::new)]
pub struct HubRx {
	key: Uuid,
	orders: Vec<ConceptualOrder<ProtocolOrderId>>,
	position_callback: PositionCallback,
}
pub async fn hub(config_arc: Arc<AppConfig>, mut rx: mpsc::Receiver<HubRx>, exchanges: Arc<Exchanges>) -> Result<()> {
	// TODO!!: assert all protocol orders here with trigger prices have them above/below current price in accordance to order's side.
	//- init the runtime of exchanges

	let (fills_tx, mut fills_rx) = tokio::sync::mpsc::channel::<HubCallback>(32);
	let (orders_tx, orders_rx) = tokio::sync::watch::channel::<HubPassforward>(HubPassforward::default());
	let mut js = JoinSet::new();

	// Spawn Binance
	let exchanges_clone = exchanges.clone();
	let config_arc_clone = config_arc.clone();
	js.spawn(async move {
		let mut exchange_runtimes_js = JoinSet::new();
		binance::binance_runtime(config_arc_clone, &mut exchange_runtimes_js, fills_tx, orders_rx, exchanges_clone.binance.clone()).await;
		exchange_runtimes_js.join_all().await;
	});

	let mut last_fill_key = Uuid::default();
	let mut position_callbacks: HashMap<Uuid, mpsc::Sender<ProtocolFills>> = HashMap::new();
	let mut requested_orders: HashMap<Uuid, Vec<ConceptualOrder<ProtocolOrderId>>> = HashMap::new();

	fn handle_hub_rx(
		hub_rx: HubRx,
		last_fill_key: &Uuid,
		requested_orders: &mut HashMap<Uuid, Vec<ConceptualOrder<ProtocolOrderId>>>,
		position_callbacks: &mut HashMap<Uuid, mpsc::Sender<ProtocolFills>>,
		orders_tx: &tokio::sync::watch::Sender<HubPassforward>,
	) -> Result<()> {
		if *last_fill_key != hub_rx.key {
			tracing::debug!("Key mismatch, ignoring the request. Requested HubRx:\n{:?}\nCorrect key: {last_fill_key}", &hub_rx);
			return Ok(());
		}
		requested_orders.insert(hub_rx.position_callback.position_id, hub_rx.orders);
		position_callbacks.insert(hub_rx.position_callback.position_id, hub_rx.position_callback.sender);

		let flat_requested_orders = requested_orders.values().flatten().cloned().collect::<Vec<ConceptualOrder<ProtocolOrderId>>>();
		let flat_requested_orders_position_id: Vec<ConceptualOrder<PositionOrderId>> = flat_requested_orders
			.into_iter()
			.map(|o| {
				let new_id = PositionOrderId::new_from_protocol_id(hub_rx.position_callback.position_id, o.id);
				ConceptualOrder { id: new_id, ..o }
			})
			.collect();

		let target_orders = hub_process_orders(flat_requested_orders_position_id);

		let binance_futures_orders = target_orders
			.iter()
			.filter(|o| o.symbol.market == Market::BinanceFutures)
			.cloned()
			.collect::<Vec<Order<PositionOrderId>>>();

		let acceptance_token = Uuid::now_v7();
		let passforward = HubPassforward::new(acceptance_token, binance_futures_orders);
		orders_tx.send(passforward)?;
		Ok(())
	}

	async fn handle_fill(fill: HubCallback, last_fill_key: &mut Uuid, position_callbacks: &HashMap<Uuid, mpsc::Sender<ProtocolFills>>) -> Result<()> {
		*last_fill_key = fill.key;
		let position_id = fill.order.id.position_id;
		let sender = position_callbacks.get(&position_id).unwrap();
		let vec_fill = vec![ProtocolFill::new(fill.order.id.into(), fill.fill_qty)];
		sender.send(ProtocolFills::new(*last_fill_key, vec_fill)).await?;
		Ok(())
	}

	loop {
		tokio::select! {
			Some(hub_rx) = rx.recv() => {
				handle_hub_rx(hub_rx, &last_fill_key, &mut requested_orders, &mut position_callbacks, &orders_tx)?;
			},
			Some(fill) = fills_rx.recv() => {
				handle_fill(fill, &mut last_fill_key, &position_callbacks).await?;
			},
			else => break,
		}
	}

	js.join_all().await;
	Ok(())
}

// HACK
/// Thing that applies all the logic for deciding on how to best express ensemble of requested orders.
fn hub_process_orders(conceptual_orders: Vec<ConceptualOrder<PositionOrderId>>) -> Vec<Order<PositionOrderId>> {
	let mut orders: Vec<Order<PositionOrderId>> = Vec::new();
	for o in conceptual_orders {
		match &o.order_type {
			ConceptualOrderType::Market(_) => {
				let order = Order::new(o.id, order_types::OrderType::Market, o.symbol.clone(), o.side, o.qty_notional);
				orders.push(order);
			}
			ConceptualOrderType::StopMarket(stop_market) => {
				let order = Order::new(
					o.id,
					order_types::OrderType::StopMarket(order_types::StopMarketOrder::new(stop_market.price)),
					o.symbol.clone(),
					o.side,
					o.qty_notional,
				);
				orders.push(order);
			}
			_ => panic!("Unsupported order type"),
		}
	}
	orders
}

mod tests {
	#[allow(unused_imports)] // RA being dumb
	use order_types::{ConceptualMarket, ConceptualStopMarket};
	#[allow(unused_imports)] // RA being dumb
	use v_utils::trades::Side;

	use super::*;
	#[allow(unused_imports)] // RA being dumb
	use crate::exchange_apis::Symbol;

	#[test]
	fn test_hub_process() {
		let from_orders = vec![
			ConceptualOrder {
				id: PositionOrderId::new(Uuid::parse_str("058a3b5d-7ce0-465c-9339-b43261e99b19").unwrap(), "ts:p0.02".to_string(), 0),
				order_type: ConceptualOrderType::Market(ConceptualMarket::default()),
				symbol: Symbol::new("BTC".to_string(), "USDT".to_string(), Market::BinanceFutures),
				side: Side::Buy,
				qty_notional: 100.0,
			},
			ConceptualOrder {
				id: PositionOrderId::new(Uuid::parse_str("86acfda1-ef53-4bae-9f20-bbad6cbc8504").unwrap(), "ts:p0.02".to_string(), 1),
				order_type: ConceptualOrderType::StopMarket(ConceptualStopMarket::default()),
				symbol: Symbol::new("BTC".to_string(), "USDT".to_string(), Market::BinanceFutures),
				side: Side::Buy,
				qty_notional: 100.0,
			},
		];

		let converted = hub_process_orders(from_orders);
		insta::assert_json_snapshot!(converted, @r###"
  [
    {
      "id": {
        "position_id": "058a3b5d-7ce0-465c-9339-b43261e99b19",
        "protocol_id": "ts:p0.02",
        "ordinal": 0
      },
      "order_type": "Market",
      "symbol": {
        "base": "BTC",
        "quote": "USDT",
        "market": "BinanceFutures"
      },
      "side": "Buy",
      "qty_notional": 100.0
    },
    {
      "id": {
        "position_id": "86acfda1-ef53-4bae-9f20-bbad6cbc8504",
        "protocol_id": "ts:p0.02",
        "ordinal": 1
      },
      "order_type": {
        "StopMarket": {
          "price": 0.0
        }
      },
      "symbol": {
        "base": "BTC",
        "quote": "USDT",
        "market": "BinanceFutures"
      },
      "side": "Buy",
      "qty_notional": 100.0
    }
  ]
  "###);
	}
}
