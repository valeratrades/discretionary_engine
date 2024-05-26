use crate::exchange_apis::binance;
use crate::exchange_apis::order_types::{Order, OrderType};
use derive_new::new;
use std::collections::HashMap;
use v_utils::trades::Side;

#[derive(Debug, Clone, new)]
pub struct BinanceOrder {
	pub order_type: BinanceOrderType,
	pub symbol: String,
	pub side: Side,
	pub qty_notional: f64,
	pub binance_id: Option<i64>,
}
impl BinanceOrder {
	pub fn to_params(&self) -> HashMap<&'static str, String> {
		let mut params = HashMap::<&'static str, String>::new();
		params.insert("symbol", self.symbol.clone());
		params.insert("side", self.side.to_string());
		params.insert("quantity", format!("{}", self.qty_notional));

		let type_params = self.order_type.to_params();
		params.extend(type_params);

		dbg!(&params);
		params
	}

	pub async fn from_standard(order: &Order) -> Self {
		let coin_quantity_adjusted = binance::apply_quantity_precision(&order.symbol.base, order.qty_notional).await.unwrap();

		let order_type = match &order.order_type {
			OrderType::Market => BinanceOrderType::Market,
			OrderType::StopMarket(sm) => BinanceOrderType::StopMarket(BinanceStopMarket::new({
				binance::apply_price_precision(&order.symbol.base, sm.price).await.unwrap()
			})),
		};

		Self::new(order_type, order.symbol.to_string(), order.side, coin_quantity_adjusted, None)
	}
}

/// All the interactions with submitting orders use this
#[derive(Debug, Clone, PartialEq)]
pub enum BinanceOrderType {
	Market,
	StopMarket(BinanceStopMarket),
	//Limit,
	//StopLoss,
	//StopLossLimit,
	//TakeProfit,
	//TakeProfitLimit,
	//LimitMaker,
}
#[derive(Debug, Clone, PartialEq, new)]
pub struct BinanceStopMarket {
	stop_price: f64,
}

impl BinanceOrderType {
	fn to_params(&self) -> HashMap<&'static str, String> {
		match self {
			BinanceOrderType::Market => {
				let mut params = HashMap::<&'static str, String>::new();
				params.insert("type", "MARKET".to_string());
				params
			}
			BinanceOrderType::StopMarket(sm) => {
				let mut params = HashMap::<&'static str, String>::new();
				params.insert("type", "STOP_MARKET".to_string());
				params.insert("stopPrice", sm.stop_price.to_string());
				params
			}
		}
	}
}
