use crate::exchange_apis::binance;
use crate::exchange_apis::order_types::{Order, OrderType, StopMarketOrder};
use crate::positions::PositionOrderId;
use derive_new::new;
use std::collections::HashMap;

#[derive(Debug, Clone, new)]
pub struct BinanceOrder {
	pub base_info: Order<PositionOrderId>,
	pub binance_id: Option<i64>,
}
impl BinanceOrder {
	pub fn to_params(&self) -> HashMap<&'static str, String> {
		let mut params = HashMap::<&'static str, String>::new();
		params.insert("symbol", self.base_info.symbol.to_string());
		params.insert("side", self.base_info.side.to_string());
		params.insert("quantity", format!("{}", self.base_info.qty_notional));

		let type_params = match self.base_info.order_type {
			OrderType::Market => {
				let mut params = HashMap::<&'static str, String>::new();
				params.insert("type", "MARKET".to_string());
				params
			}
			OrderType::StopMarket(sm) => {
				let mut params = HashMap::<&'static str, String>::new();
				params.insert("type", "STOP_MARKET".to_string());
				params.insert("stopPrice", sm.price.to_string());
				params
			}
		};

		params.extend(type_params);

		dbg!(&params);
		params
	}

	pub async fn from_standard(order: &Order<PositionOrderId>) -> Self {
		let coin_quantity_adjusted = binance::apply_quantity_precision(&order.symbol.base, order.qty_notional).await.unwrap();

		let order_type = match &order.order_type {
			OrderType::Market => OrderType::Market,
			OrderType::StopMarket(sm) => OrderType::StopMarket(StopMarketOrder::new({
				binance::apply_price_precision(&order.symbol.base, sm.price).await.unwrap()
			})),
		};

		Self::new(order.clone(), None)
	}
}
