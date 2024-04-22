use crate::api::order_types::{Order, OrderType};
use derive_new::new;
use std::collections::HashMap;
use v_utils::trades::Side;

#[derive(Debug, Clone, new)]
pub struct BinanceOrder {
	pub order_type: BinanceOrderType,
	pub symbol: String,
	pub side: Side,
	pub qty_notional: f64,
}
impl BinanceOrder {
	pub fn into_params(self) -> HashMap<&'static str, String> {
		let mut params = HashMap::<&'static str, String>::new();
		params.insert("symbol", self.symbol);
		params.insert("side", self.side.to_string());
		params.insert("quantity", format!("{}", self.qty_notional));

		let type_params = self.order_type.into_params();
		params.extend(type_params);

		params
	}

	pub async fn from_standard(order: Order) -> Self {
		let quantity_precision = crate::api::binance::futures_quantity_precision(&order.symbol.base).await.unwrap();
		let factor = 10_f64.powi(quantity_precision as i32);
		let coin_quantity_adjusted = (order.qty_notional * factor).round() / factor;

		let order_type = match order.order_type {
			OrderType::Market => BinanceOrderType::Market,
			OrderType::StopMarket(sm) => BinanceOrderType::StopMarket(BinanceStopMarket::new(sm.price)),
		};

		let binance_order = Self::new(order_type, order.symbol.to_string(), order.side.clone(), coin_quantity_adjusted);

		binance_order
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
	fn into_params(self) -> HashMap<&'static str, String> {
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
