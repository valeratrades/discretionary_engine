#![feature(prelude_import)]
#[prelude_import]
use std::prelude::rust_2021::*;
#[macro_use]
extern crate std;
pub mod binance_api {
    use crate::exchange_interactions::Market;
    use anyhow::Result;
    use chrono::Utc;
    use hmac::{Hmac, Mac};
    use reqwest::header::{HeaderMap, HeaderValue, CONTENT_TYPE};
    use serde::{Deserialize, Serialize};
    use serde_json::Number;
    use serde_json::Value;
    use serde_urlencoded;
    use sha2::Sha256;
    use std::collections::HashMap;
    use url::Url;
    use v_utils::trades::Side;
    type HmacSha256 = Hmac<Sha256>;
    #[allow(dead_code)]
    pub enum HttpMethod {
        GET,
        POST,
        PUT,
        DELETE,
    }
    #[allow(dead_code)]
    pub struct Binance {
        futures_symbols: HashMap<String, FuturesSymbol>,
    }
    pub async fn signed_request(
        http_method: HttpMethod,
        endpoint_str: &str,
        mut params: HashMap<&'static str, String>,
        key: String,
        secret: String,
    ) -> Result<reqwest::Response> {
        let mut headers = HeaderMap::new();
        headers
            .insert(
                CONTENT_TYPE,
                HeaderValue::from_static("application/json;charset=utf-8"),
            );
        headers.insert("X-MBX-APIKEY", HeaderValue::from_str(&key).unwrap());
        let client = reqwest::Client::builder().default_headers(headers).build()?;
        let time_ms = Utc::now().timestamp_millis();
        params
            .insert(
                "timestamp",
                {
                    let res = ::alloc::fmt::format(format_args!("{0}", time_ms));
                    res
                },
            );
        let query_string = serde_urlencoded::to_string(&params)?;
        let mut mac = HmacSha256::new_from_slice(secret.as_bytes()).unwrap();
        mac.update(query_string.as_bytes());
        let mac_bytes = mac.finalize().into_bytes();
        let signature = hex::encode(mac_bytes);
        let url = {
            let res = ::alloc::fmt::format(
                format_args!(
                    "{0}?{1}&signature={2}",
                    endpoint_str,
                    query_string,
                    signature,
                ),
            );
            res
        };
        let r = match http_method {
            HttpMethod::GET => client.get(&url).send().await?,
            HttpMethod::POST => client.post(&url).send().await?,
            _ => {
                ::core::panicking::panic_fmt(format_args!("Not implemented"));
            }
        };
        Ok(r)
    }
    pub enum OrderType {
        Market,
        Limit,
        StopLoss,
        StopLossLimit,
        TakeProfit,
        TakeProfitLimit,
        LimitMaker,
    }
    impl ToString for OrderType {
        fn to_string(&self) -> String {
            match self {
                OrderType::Market => "MARKET".to_string(),
                OrderType::Limit => "LIMIT".to_string(),
                OrderType::StopLoss => "STOP_LOSS".to_string(),
                OrderType::StopLossLimit => "STOP_LOSS_LIMIT".to_string(),
                OrderType::TakeProfit => "TAKE_PROFIT".to_string(),
                OrderType::TakeProfitLimit => "TAKE_PROFIT_LIMIT".to_string(),
                OrderType::LimitMaker => "LIMIT_MAKER".to_string(),
            }
        }
    }
    pub async fn get_balance(
        key: String,
        secret: String,
        market: Market,
    ) -> Result<f32> {
        let params = HashMap::<&str, String>::new();
        match market {
            Market::BinanceFutures => {
                let base_url = market.get_base_url();
                let url = base_url.join("fapi/v2/balance")?;
                let r = signed_request(
                        HttpMethod::GET,
                        url.as_str(),
                        params,
                        key,
                        secret,
                    )
                    .await?;
                let asset_balances: Vec<FuturesBalance> = r.json().await?;
                let mut total_balance = 0.0;
                for asset in asset_balances {
                    total_balance += asset.balance.parse::<f32>()?;
                }
                Ok(total_balance)
            }
            Market::BinanceSpot => {
                let base_url = market.get_base_url();
                let url = base_url.join("/api/v3/account")?;
                let r = signed_request(
                        HttpMethod::GET,
                        url.as_str(),
                        params,
                        key,
                        secret,
                    )
                    .await?;
                let account_details: SpotAccountDetails = r.json().await?;
                let asset_balances = account_details.balances;
                let mut total_balance = 0.0;
                for asset in asset_balances {
                    total_balance += asset.free.parse::<f32>()?;
                    total_balance += asset.locked.parse::<f32>()?;
                }
                Ok(total_balance)
            }
            Market::BinanceMargin => {
                let base_url = market.get_base_url();
                let url = base_url.join("/sapi/v1/margin/account")?;
                let r = signed_request(
                        HttpMethod::GET,
                        url.as_str(),
                        params,
                        key,
                        secret,
                    )
                    .await?;
                let account_details: MarginAccountDetails = r.json().await?;
                let total_balance: f32 = account_details
                    .TotalCollateralValueInUSDT
                    .parse()?;
                Ok(total_balance)
            }
        }
    }
    pub async fn futures_price(symbol: String) -> Result<f32> {
        let base_url = Market::BinanceFutures.get_base_url();
        let url = base_url.join("/fapi/v2/ticker/price")?;
        let mut params = HashMap::<&str, String>::new();
        params.insert("symbol", symbol.clone());
        let client = reqwest::Client::new();
        let r = client.get(url).json(&params).send().await?;
        let prices: Vec<serde_json::Value> = r.json().await?;
        let price = prices
            .iter()
            .find(|x| x.get("symbol").unwrap().as_str().unwrap().to_string() == symbol)
            .unwrap()
            .get("price")
            .unwrap()
            .as_str()
            .unwrap()
            .parse::<f32>()?;
        Ok(price)
    }
    pub async fn futures_quantity_precision(symbol: String) -> Result<usize> {
        let base_url = Market::BinanceFutures.get_base_url();
        let url = base_url.join("/fapi/v1/exchangeInfo")?;
        let r = reqwest::get(url).await?;
        let futures_exchange_info: FuturesExchangeInfo = r.json().await?;
        let symbol_info = futures_exchange_info
            .symbols
            .iter()
            .find(|x| x.symbol == symbol)
            .unwrap();
        Ok(symbol_info.quantityPrecision)
    }
    pub async fn post_futures_trade(
        key: String,
        secret: String,
        order_type: OrderType,
        symbol: String,
        side: Side,
        quantity: f32,
    ) -> Result<FuturesPositionResponse> {
        let url = FuturesPositionResponse::get_url();
        let mut params = HashMap::<&str, String>::new();
        params.insert("symbol", symbol);
        params.insert("side", side.to_string());
        params.insert("type", order_type.to_string());
        params
            .insert(
                "quantity",
                {
                    let res = ::alloc::fmt::format(format_args!("{0}", quantity));
                    res
                },
            );
        let r = signed_request(HttpMethod::POST, url.as_str(), params, key, secret)
            .await?;
        let response: FuturesPositionResponse = r.json().await?;
        Ok(response)
    }
    #[allow(non_snake_case)]
    pub struct FuturesPositionResponse {
        clientOrderId: String,
        cumQty: Option<String>,
        cumQuote: String,
        executedQty: String,
        orderId: i64,
        avgPrice: Option<String>,
        origQty: String,
        price: String,
        reduceOnly: bool,
        side: String,
        positionSide: Option<String>,
        status: String,
        stopPrice: String,
        closePosition: bool,
        symbol: String,
        timeInForce: String,
        r#type: String,
        origType: String,
        activatePrice: Option<f32>,
        priceRate: Option<f32>,
        updateTime: i64,
        workingType: Option<String>,
        priceProtect: bool,
        priceMatch: Option<String>,
        selfTradePreventionMode: Option<String>,
        goodTillDate: Option<i64>,
    }
    #[doc(hidden)]
    #[allow(non_upper_case_globals, unused_attributes, unused_qualifications)]
    const _: () = {
        #[allow(unused_extern_crates, clippy::useless_attribute)]
        extern crate serde as _serde;
        #[automatically_derived]
        impl _serde::Serialize for FuturesPositionResponse {
            fn serialize<__S>(
                &self,
                __serializer: __S,
            ) -> _serde::__private::Result<__S::Ok, __S::Error>
            where
                __S: _serde::Serializer,
            {
                let mut __serde_state = _serde::Serializer::serialize_struct(
                    __serializer,
                    "FuturesPositionResponse",
                    false as usize + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1
                        + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1,
                )?;
                _serde::ser::SerializeStruct::serialize_field(
                    &mut __serde_state,
                    "clientOrderId",
                    &self.clientOrderId,
                )?;
                _serde::ser::SerializeStruct::serialize_field(
                    &mut __serde_state,
                    "cumQty",
                    &self.cumQty,
                )?;
                _serde::ser::SerializeStruct::serialize_field(
                    &mut __serde_state,
                    "cumQuote",
                    &self.cumQuote,
                )?;
                _serde::ser::SerializeStruct::serialize_field(
                    &mut __serde_state,
                    "executedQty",
                    &self.executedQty,
                )?;
                _serde::ser::SerializeStruct::serialize_field(
                    &mut __serde_state,
                    "orderId",
                    &self.orderId,
                )?;
                _serde::ser::SerializeStruct::serialize_field(
                    &mut __serde_state,
                    "avgPrice",
                    &self.avgPrice,
                )?;
                _serde::ser::SerializeStruct::serialize_field(
                    &mut __serde_state,
                    "origQty",
                    &self.origQty,
                )?;
                _serde::ser::SerializeStruct::serialize_field(
                    &mut __serde_state,
                    "price",
                    &self.price,
                )?;
                _serde::ser::SerializeStruct::serialize_field(
                    &mut __serde_state,
                    "reduceOnly",
                    &self.reduceOnly,
                )?;
                _serde::ser::SerializeStruct::serialize_field(
                    &mut __serde_state,
                    "side",
                    &self.side,
                )?;
                _serde::ser::SerializeStruct::serialize_field(
                    &mut __serde_state,
                    "positionSide",
                    &self.positionSide,
                )?;
                _serde::ser::SerializeStruct::serialize_field(
                    &mut __serde_state,
                    "status",
                    &self.status,
                )?;
                _serde::ser::SerializeStruct::serialize_field(
                    &mut __serde_state,
                    "stopPrice",
                    &self.stopPrice,
                )?;
                _serde::ser::SerializeStruct::serialize_field(
                    &mut __serde_state,
                    "closePosition",
                    &self.closePosition,
                )?;
                _serde::ser::SerializeStruct::serialize_field(
                    &mut __serde_state,
                    "symbol",
                    &self.symbol,
                )?;
                _serde::ser::SerializeStruct::serialize_field(
                    &mut __serde_state,
                    "timeInForce",
                    &self.timeInForce,
                )?;
                _serde::ser::SerializeStruct::serialize_field(
                    &mut __serde_state,
                    "type",
                    &self.r#type,
                )?;
                _serde::ser::SerializeStruct::serialize_field(
                    &mut __serde_state,
                    "origType",
                    &self.origType,
                )?;
                _serde::ser::SerializeStruct::serialize_field(
                    &mut __serde_state,
                    "activatePrice",
                    &self.activatePrice,
                )?;
                _serde::ser::SerializeStruct::serialize_field(
                    &mut __serde_state,
                    "priceRate",
                    &self.priceRate,
                )?;
                _serde::ser::SerializeStruct::serialize_field(
                    &mut __serde_state,
                    "updateTime",
                    &self.updateTime,
                )?;
                _serde::ser::SerializeStruct::serialize_field(
                    &mut __serde_state,
                    "workingType",
                    &self.workingType,
                )?;
                _serde::ser::SerializeStruct::serialize_field(
                    &mut __serde_state,
                    "priceProtect",
                    &self.priceProtect,
                )?;
                _serde::ser::SerializeStruct::serialize_field(
                    &mut __serde_state,
                    "priceMatch",
                    &self.priceMatch,
                )?;
                _serde::ser::SerializeStruct::serialize_field(
                    &mut __serde_state,
                    "selfTradePreventionMode",
                    &self.selfTradePreventionMode,
                )?;
                _serde::ser::SerializeStruct::serialize_field(
                    &mut __serde_state,
                    "goodTillDate",
                    &self.goodTillDate,
                )?;
                _serde::ser::SerializeStruct::end(__serde_state)
            }
        }
    };
    #[doc(hidden)]
    #[allow(non_upper_case_globals, unused_attributes, unused_qualifications)]
    const _: () = {
        #[allow(unused_extern_crates, clippy::useless_attribute)]
        extern crate serde as _serde;
        #[automatically_derived]
        impl<'de> _serde::Deserialize<'de> for FuturesPositionResponse {
            fn deserialize<__D>(
                __deserializer: __D,
            ) -> _serde::__private::Result<Self, __D::Error>
            where
                __D: _serde::Deserializer<'de>,
            {
                #[allow(non_camel_case_types)]
                #[doc(hidden)]
                enum __Field {
                    __field0,
                    __field1,
                    __field2,
                    __field3,
                    __field4,
                    __field5,
                    __field6,
                    __field7,
                    __field8,
                    __field9,
                    __field10,
                    __field11,
                    __field12,
                    __field13,
                    __field14,
                    __field15,
                    __field16,
                    __field17,
                    __field18,
                    __field19,
                    __field20,
                    __field21,
                    __field22,
                    __field23,
                    __field24,
                    __field25,
                    __ignore,
                }
                #[doc(hidden)]
                struct __FieldVisitor;
                impl<'de> _serde::de::Visitor<'de> for __FieldVisitor {
                    type Value = __Field;
                    fn expecting(
                        &self,
                        __formatter: &mut _serde::__private::Formatter,
                    ) -> _serde::__private::fmt::Result {
                        _serde::__private::Formatter::write_str(
                            __formatter,
                            "field identifier",
                        )
                    }
                    fn visit_u64<__E>(
                        self,
                        __value: u64,
                    ) -> _serde::__private::Result<Self::Value, __E>
                    where
                        __E: _serde::de::Error,
                    {
                        match __value {
                            0u64 => _serde::__private::Ok(__Field::__field0),
                            1u64 => _serde::__private::Ok(__Field::__field1),
                            2u64 => _serde::__private::Ok(__Field::__field2),
                            3u64 => _serde::__private::Ok(__Field::__field3),
                            4u64 => _serde::__private::Ok(__Field::__field4),
                            5u64 => _serde::__private::Ok(__Field::__field5),
                            6u64 => _serde::__private::Ok(__Field::__field6),
                            7u64 => _serde::__private::Ok(__Field::__field7),
                            8u64 => _serde::__private::Ok(__Field::__field8),
                            9u64 => _serde::__private::Ok(__Field::__field9),
                            10u64 => _serde::__private::Ok(__Field::__field10),
                            11u64 => _serde::__private::Ok(__Field::__field11),
                            12u64 => _serde::__private::Ok(__Field::__field12),
                            13u64 => _serde::__private::Ok(__Field::__field13),
                            14u64 => _serde::__private::Ok(__Field::__field14),
                            15u64 => _serde::__private::Ok(__Field::__field15),
                            16u64 => _serde::__private::Ok(__Field::__field16),
                            17u64 => _serde::__private::Ok(__Field::__field17),
                            18u64 => _serde::__private::Ok(__Field::__field18),
                            19u64 => _serde::__private::Ok(__Field::__field19),
                            20u64 => _serde::__private::Ok(__Field::__field20),
                            21u64 => _serde::__private::Ok(__Field::__field21),
                            22u64 => _serde::__private::Ok(__Field::__field22),
                            23u64 => _serde::__private::Ok(__Field::__field23),
                            24u64 => _serde::__private::Ok(__Field::__field24),
                            25u64 => _serde::__private::Ok(__Field::__field25),
                            _ => _serde::__private::Ok(__Field::__ignore),
                        }
                    }
                    fn visit_str<__E>(
                        self,
                        __value: &str,
                    ) -> _serde::__private::Result<Self::Value, __E>
                    where
                        __E: _serde::de::Error,
                    {
                        match __value {
                            "clientOrderId" => _serde::__private::Ok(__Field::__field0),
                            "cumQty" => _serde::__private::Ok(__Field::__field1),
                            "cumQuote" => _serde::__private::Ok(__Field::__field2),
                            "executedQty" => _serde::__private::Ok(__Field::__field3),
                            "orderId" => _serde::__private::Ok(__Field::__field4),
                            "avgPrice" => _serde::__private::Ok(__Field::__field5),
                            "origQty" => _serde::__private::Ok(__Field::__field6),
                            "price" => _serde::__private::Ok(__Field::__field7),
                            "reduceOnly" => _serde::__private::Ok(__Field::__field8),
                            "side" => _serde::__private::Ok(__Field::__field9),
                            "positionSide" => _serde::__private::Ok(__Field::__field10),
                            "status" => _serde::__private::Ok(__Field::__field11),
                            "stopPrice" => _serde::__private::Ok(__Field::__field12),
                            "closePosition" => _serde::__private::Ok(__Field::__field13),
                            "symbol" => _serde::__private::Ok(__Field::__field14),
                            "timeInForce" => _serde::__private::Ok(__Field::__field15),
                            "type" => _serde::__private::Ok(__Field::__field16),
                            "origType" => _serde::__private::Ok(__Field::__field17),
                            "activatePrice" => _serde::__private::Ok(__Field::__field18),
                            "priceRate" => _serde::__private::Ok(__Field::__field19),
                            "updateTime" => _serde::__private::Ok(__Field::__field20),
                            "workingType" => _serde::__private::Ok(__Field::__field21),
                            "priceProtect" => _serde::__private::Ok(__Field::__field22),
                            "priceMatch" => _serde::__private::Ok(__Field::__field23),
                            "selfTradePreventionMode" => {
                                _serde::__private::Ok(__Field::__field24)
                            }
                            "goodTillDate" => _serde::__private::Ok(__Field::__field25),
                            _ => _serde::__private::Ok(__Field::__ignore),
                        }
                    }
                    fn visit_bytes<__E>(
                        self,
                        __value: &[u8],
                    ) -> _serde::__private::Result<Self::Value, __E>
                    where
                        __E: _serde::de::Error,
                    {
                        match __value {
                            b"clientOrderId" => _serde::__private::Ok(__Field::__field0),
                            b"cumQty" => _serde::__private::Ok(__Field::__field1),
                            b"cumQuote" => _serde::__private::Ok(__Field::__field2),
                            b"executedQty" => _serde::__private::Ok(__Field::__field3),
                            b"orderId" => _serde::__private::Ok(__Field::__field4),
                            b"avgPrice" => _serde::__private::Ok(__Field::__field5),
                            b"origQty" => _serde::__private::Ok(__Field::__field6),
                            b"price" => _serde::__private::Ok(__Field::__field7),
                            b"reduceOnly" => _serde::__private::Ok(__Field::__field8),
                            b"side" => _serde::__private::Ok(__Field::__field9),
                            b"positionSide" => _serde::__private::Ok(__Field::__field10),
                            b"status" => _serde::__private::Ok(__Field::__field11),
                            b"stopPrice" => _serde::__private::Ok(__Field::__field12),
                            b"closePosition" => _serde::__private::Ok(__Field::__field13),
                            b"symbol" => _serde::__private::Ok(__Field::__field14),
                            b"timeInForce" => _serde::__private::Ok(__Field::__field15),
                            b"type" => _serde::__private::Ok(__Field::__field16),
                            b"origType" => _serde::__private::Ok(__Field::__field17),
                            b"activatePrice" => _serde::__private::Ok(__Field::__field18),
                            b"priceRate" => _serde::__private::Ok(__Field::__field19),
                            b"updateTime" => _serde::__private::Ok(__Field::__field20),
                            b"workingType" => _serde::__private::Ok(__Field::__field21),
                            b"priceProtect" => _serde::__private::Ok(__Field::__field22),
                            b"priceMatch" => _serde::__private::Ok(__Field::__field23),
                            b"selfTradePreventionMode" => {
                                _serde::__private::Ok(__Field::__field24)
                            }
                            b"goodTillDate" => _serde::__private::Ok(__Field::__field25),
                            _ => _serde::__private::Ok(__Field::__ignore),
                        }
                    }
                }
                impl<'de> _serde::Deserialize<'de> for __Field {
                    #[inline]
                    fn deserialize<__D>(
                        __deserializer: __D,
                    ) -> _serde::__private::Result<Self, __D::Error>
                    where
                        __D: _serde::Deserializer<'de>,
                    {
                        _serde::Deserializer::deserialize_identifier(
                            __deserializer,
                            __FieldVisitor,
                        )
                    }
                }
                #[doc(hidden)]
                struct __Visitor<'de> {
                    marker: _serde::__private::PhantomData<FuturesPositionResponse>,
                    lifetime: _serde::__private::PhantomData<&'de ()>,
                }
                impl<'de> _serde::de::Visitor<'de> for __Visitor<'de> {
                    type Value = FuturesPositionResponse;
                    fn expecting(
                        &self,
                        __formatter: &mut _serde::__private::Formatter,
                    ) -> _serde::__private::fmt::Result {
                        _serde::__private::Formatter::write_str(
                            __formatter,
                            "struct FuturesPositionResponse",
                        )
                    }
                    #[inline]
                    fn visit_seq<__A>(
                        self,
                        mut __seq: __A,
                    ) -> _serde::__private::Result<Self::Value, __A::Error>
                    where
                        __A: _serde::de::SeqAccess<'de>,
                    {
                        let __field0 = match _serde::de::SeqAccess::next_element::<
                            String,
                        >(&mut __seq)? {
                            _serde::__private::Some(__value) => __value,
                            _serde::__private::None => {
                                return _serde::__private::Err(
                                    _serde::de::Error::invalid_length(
                                        0usize,
                                        &"struct FuturesPositionResponse with 26 elements",
                                    ),
                                );
                            }
                        };
                        let __field1 = match _serde::de::SeqAccess::next_element::<
                            Option<String>,
                        >(&mut __seq)? {
                            _serde::__private::Some(__value) => __value,
                            _serde::__private::None => {
                                return _serde::__private::Err(
                                    _serde::de::Error::invalid_length(
                                        1usize,
                                        &"struct FuturesPositionResponse with 26 elements",
                                    ),
                                );
                            }
                        };
                        let __field2 = match _serde::de::SeqAccess::next_element::<
                            String,
                        >(&mut __seq)? {
                            _serde::__private::Some(__value) => __value,
                            _serde::__private::None => {
                                return _serde::__private::Err(
                                    _serde::de::Error::invalid_length(
                                        2usize,
                                        &"struct FuturesPositionResponse with 26 elements",
                                    ),
                                );
                            }
                        };
                        let __field3 = match _serde::de::SeqAccess::next_element::<
                            String,
                        >(&mut __seq)? {
                            _serde::__private::Some(__value) => __value,
                            _serde::__private::None => {
                                return _serde::__private::Err(
                                    _serde::de::Error::invalid_length(
                                        3usize,
                                        &"struct FuturesPositionResponse with 26 elements",
                                    ),
                                );
                            }
                        };
                        let __field4 = match _serde::de::SeqAccess::next_element::<
                            i64,
                        >(&mut __seq)? {
                            _serde::__private::Some(__value) => __value,
                            _serde::__private::None => {
                                return _serde::__private::Err(
                                    _serde::de::Error::invalid_length(
                                        4usize,
                                        &"struct FuturesPositionResponse with 26 elements",
                                    ),
                                );
                            }
                        };
                        let __field5 = match _serde::de::SeqAccess::next_element::<
                            Option<String>,
                        >(&mut __seq)? {
                            _serde::__private::Some(__value) => __value,
                            _serde::__private::None => {
                                return _serde::__private::Err(
                                    _serde::de::Error::invalid_length(
                                        5usize,
                                        &"struct FuturesPositionResponse with 26 elements",
                                    ),
                                );
                            }
                        };
                        let __field6 = match _serde::de::SeqAccess::next_element::<
                            String,
                        >(&mut __seq)? {
                            _serde::__private::Some(__value) => __value,
                            _serde::__private::None => {
                                return _serde::__private::Err(
                                    _serde::de::Error::invalid_length(
                                        6usize,
                                        &"struct FuturesPositionResponse with 26 elements",
                                    ),
                                );
                            }
                        };
                        let __field7 = match _serde::de::SeqAccess::next_element::<
                            String,
                        >(&mut __seq)? {
                            _serde::__private::Some(__value) => __value,
                            _serde::__private::None => {
                                return _serde::__private::Err(
                                    _serde::de::Error::invalid_length(
                                        7usize,
                                        &"struct FuturesPositionResponse with 26 elements",
                                    ),
                                );
                            }
                        };
                        let __field8 = match _serde::de::SeqAccess::next_element::<
                            bool,
                        >(&mut __seq)? {
                            _serde::__private::Some(__value) => __value,
                            _serde::__private::None => {
                                return _serde::__private::Err(
                                    _serde::de::Error::invalid_length(
                                        8usize,
                                        &"struct FuturesPositionResponse with 26 elements",
                                    ),
                                );
                            }
                        };
                        let __field9 = match _serde::de::SeqAccess::next_element::<
                            String,
                        >(&mut __seq)? {
                            _serde::__private::Some(__value) => __value,
                            _serde::__private::None => {
                                return _serde::__private::Err(
                                    _serde::de::Error::invalid_length(
                                        9usize,
                                        &"struct FuturesPositionResponse with 26 elements",
                                    ),
                                );
                            }
                        };
                        let __field10 = match _serde::de::SeqAccess::next_element::<
                            Option<String>,
                        >(&mut __seq)? {
                            _serde::__private::Some(__value) => __value,
                            _serde::__private::None => {
                                return _serde::__private::Err(
                                    _serde::de::Error::invalid_length(
                                        10usize,
                                        &"struct FuturesPositionResponse with 26 elements",
                                    ),
                                );
                            }
                        };
                        let __field11 = match _serde::de::SeqAccess::next_element::<
                            String,
                        >(&mut __seq)? {
                            _serde::__private::Some(__value) => __value,
                            _serde::__private::None => {
                                return _serde::__private::Err(
                                    _serde::de::Error::invalid_length(
                                        11usize,
                                        &"struct FuturesPositionResponse with 26 elements",
                                    ),
                                );
                            }
                        };
                        let __field12 = match _serde::de::SeqAccess::next_element::<
                            String,
                        >(&mut __seq)? {
                            _serde::__private::Some(__value) => __value,
                            _serde::__private::None => {
                                return _serde::__private::Err(
                                    _serde::de::Error::invalid_length(
                                        12usize,
                                        &"struct FuturesPositionResponse with 26 elements",
                                    ),
                                );
                            }
                        };
                        let __field13 = match _serde::de::SeqAccess::next_element::<
                            bool,
                        >(&mut __seq)? {
                            _serde::__private::Some(__value) => __value,
                            _serde::__private::None => {
                                return _serde::__private::Err(
                                    _serde::de::Error::invalid_length(
                                        13usize,
                                        &"struct FuturesPositionResponse with 26 elements",
                                    ),
                                );
                            }
                        };
                        let __field14 = match _serde::de::SeqAccess::next_element::<
                            String,
                        >(&mut __seq)? {
                            _serde::__private::Some(__value) => __value,
                            _serde::__private::None => {
                                return _serde::__private::Err(
                                    _serde::de::Error::invalid_length(
                                        14usize,
                                        &"struct FuturesPositionResponse with 26 elements",
                                    ),
                                );
                            }
                        };
                        let __field15 = match _serde::de::SeqAccess::next_element::<
                            String,
                        >(&mut __seq)? {
                            _serde::__private::Some(__value) => __value,
                            _serde::__private::None => {
                                return _serde::__private::Err(
                                    _serde::de::Error::invalid_length(
                                        15usize,
                                        &"struct FuturesPositionResponse with 26 elements",
                                    ),
                                );
                            }
                        };
                        let __field16 = match _serde::de::SeqAccess::next_element::<
                            String,
                        >(&mut __seq)? {
                            _serde::__private::Some(__value) => __value,
                            _serde::__private::None => {
                                return _serde::__private::Err(
                                    _serde::de::Error::invalid_length(
                                        16usize,
                                        &"struct FuturesPositionResponse with 26 elements",
                                    ),
                                );
                            }
                        };
                        let __field17 = match _serde::de::SeqAccess::next_element::<
                            String,
                        >(&mut __seq)? {
                            _serde::__private::Some(__value) => __value,
                            _serde::__private::None => {
                                return _serde::__private::Err(
                                    _serde::de::Error::invalid_length(
                                        17usize,
                                        &"struct FuturesPositionResponse with 26 elements",
                                    ),
                                );
                            }
                        };
                        let __field18 = match _serde::de::SeqAccess::next_element::<
                            Option<f32>,
                        >(&mut __seq)? {
                            _serde::__private::Some(__value) => __value,
                            _serde::__private::None => {
                                return _serde::__private::Err(
                                    _serde::de::Error::invalid_length(
                                        18usize,
                                        &"struct FuturesPositionResponse with 26 elements",
                                    ),
                                );
                            }
                        };
                        let __field19 = match _serde::de::SeqAccess::next_element::<
                            Option<f32>,
                        >(&mut __seq)? {
                            _serde::__private::Some(__value) => __value,
                            _serde::__private::None => {
                                return _serde::__private::Err(
                                    _serde::de::Error::invalid_length(
                                        19usize,
                                        &"struct FuturesPositionResponse with 26 elements",
                                    ),
                                );
                            }
                        };
                        let __field20 = match _serde::de::SeqAccess::next_element::<
                            i64,
                        >(&mut __seq)? {
                            _serde::__private::Some(__value) => __value,
                            _serde::__private::None => {
                                return _serde::__private::Err(
                                    _serde::de::Error::invalid_length(
                                        20usize,
                                        &"struct FuturesPositionResponse with 26 elements",
                                    ),
                                );
                            }
                        };
                        let __field21 = match _serde::de::SeqAccess::next_element::<
                            Option<String>,
                        >(&mut __seq)? {
                            _serde::__private::Some(__value) => __value,
                            _serde::__private::None => {
                                return _serde::__private::Err(
                                    _serde::de::Error::invalid_length(
                                        21usize,
                                        &"struct FuturesPositionResponse with 26 elements",
                                    ),
                                );
                            }
                        };
                        let __field22 = match _serde::de::SeqAccess::next_element::<
                            bool,
                        >(&mut __seq)? {
                            _serde::__private::Some(__value) => __value,
                            _serde::__private::None => {
                                return _serde::__private::Err(
                                    _serde::de::Error::invalid_length(
                                        22usize,
                                        &"struct FuturesPositionResponse with 26 elements",
                                    ),
                                );
                            }
                        };
                        let __field23 = match _serde::de::SeqAccess::next_element::<
                            Option<String>,
                        >(&mut __seq)? {
                            _serde::__private::Some(__value) => __value,
                            _serde::__private::None => {
                                return _serde::__private::Err(
                                    _serde::de::Error::invalid_length(
                                        23usize,
                                        &"struct FuturesPositionResponse with 26 elements",
                                    ),
                                );
                            }
                        };
                        let __field24 = match _serde::de::SeqAccess::next_element::<
                            Option<String>,
                        >(&mut __seq)? {
                            _serde::__private::Some(__value) => __value,
                            _serde::__private::None => {
                                return _serde::__private::Err(
                                    _serde::de::Error::invalid_length(
                                        24usize,
                                        &"struct FuturesPositionResponse with 26 elements",
                                    ),
                                );
                            }
                        };
                        let __field25 = match _serde::de::SeqAccess::next_element::<
                            Option<i64>,
                        >(&mut __seq)? {
                            _serde::__private::Some(__value) => __value,
                            _serde::__private::None => {
                                return _serde::__private::Err(
                                    _serde::de::Error::invalid_length(
                                        25usize,
                                        &"struct FuturesPositionResponse with 26 elements",
                                    ),
                                );
                            }
                        };
                        _serde::__private::Ok(FuturesPositionResponse {
                            clientOrderId: __field0,
                            cumQty: __field1,
                            cumQuote: __field2,
                            executedQty: __field3,
                            orderId: __field4,
                            avgPrice: __field5,
                            origQty: __field6,
                            price: __field7,
                            reduceOnly: __field8,
                            side: __field9,
                            positionSide: __field10,
                            status: __field11,
                            stopPrice: __field12,
                            closePosition: __field13,
                            symbol: __field14,
                            timeInForce: __field15,
                            r#type: __field16,
                            origType: __field17,
                            activatePrice: __field18,
                            priceRate: __field19,
                            updateTime: __field20,
                            workingType: __field21,
                            priceProtect: __field22,
                            priceMatch: __field23,
                            selfTradePreventionMode: __field24,
                            goodTillDate: __field25,
                        })
                    }
                    #[inline]
                    fn visit_map<__A>(
                        self,
                        mut __map: __A,
                    ) -> _serde::__private::Result<Self::Value, __A::Error>
                    where
                        __A: _serde::de::MapAccess<'de>,
                    {
                        let mut __field0: _serde::__private::Option<String> = _serde::__private::None;
                        let mut __field1: _serde::__private::Option<Option<String>> = _serde::__private::None;
                        let mut __field2: _serde::__private::Option<String> = _serde::__private::None;
                        let mut __field3: _serde::__private::Option<String> = _serde::__private::None;
                        let mut __field4: _serde::__private::Option<i64> = _serde::__private::None;
                        let mut __field5: _serde::__private::Option<Option<String>> = _serde::__private::None;
                        let mut __field6: _serde::__private::Option<String> = _serde::__private::None;
                        let mut __field7: _serde::__private::Option<String> = _serde::__private::None;
                        let mut __field8: _serde::__private::Option<bool> = _serde::__private::None;
                        let mut __field9: _serde::__private::Option<String> = _serde::__private::None;
                        let mut __field10: _serde::__private::Option<Option<String>> = _serde::__private::None;
                        let mut __field11: _serde::__private::Option<String> = _serde::__private::None;
                        let mut __field12: _serde::__private::Option<String> = _serde::__private::None;
                        let mut __field13: _serde::__private::Option<bool> = _serde::__private::None;
                        let mut __field14: _serde::__private::Option<String> = _serde::__private::None;
                        let mut __field15: _serde::__private::Option<String> = _serde::__private::None;
                        let mut __field16: _serde::__private::Option<String> = _serde::__private::None;
                        let mut __field17: _serde::__private::Option<String> = _serde::__private::None;
                        let mut __field18: _serde::__private::Option<Option<f32>> = _serde::__private::None;
                        let mut __field19: _serde::__private::Option<Option<f32>> = _serde::__private::None;
                        let mut __field20: _serde::__private::Option<i64> = _serde::__private::None;
                        let mut __field21: _serde::__private::Option<Option<String>> = _serde::__private::None;
                        let mut __field22: _serde::__private::Option<bool> = _serde::__private::None;
                        let mut __field23: _serde::__private::Option<Option<String>> = _serde::__private::None;
                        let mut __field24: _serde::__private::Option<Option<String>> = _serde::__private::None;
                        let mut __field25: _serde::__private::Option<Option<i64>> = _serde::__private::None;
                        while let _serde::__private::Some(__key) = _serde::de::MapAccess::next_key::<
                            __Field,
                        >(&mut __map)? {
                            match __key {
                                __Field::__field0 => {
                                    if _serde::__private::Option::is_some(&__field0) {
                                        return _serde::__private::Err(
                                            <__A::Error as _serde::de::Error>::duplicate_field(
                                                "clientOrderId",
                                            ),
                                        );
                                    }
                                    __field0 = _serde::__private::Some(
                                        _serde::de::MapAccess::next_value::<String>(&mut __map)?,
                                    );
                                }
                                __Field::__field1 => {
                                    if _serde::__private::Option::is_some(&__field1) {
                                        return _serde::__private::Err(
                                            <__A::Error as _serde::de::Error>::duplicate_field("cumQty"),
                                        );
                                    }
                                    __field1 = _serde::__private::Some(
                                        _serde::de::MapAccess::next_value::<
                                            Option<String>,
                                        >(&mut __map)?,
                                    );
                                }
                                __Field::__field2 => {
                                    if _serde::__private::Option::is_some(&__field2) {
                                        return _serde::__private::Err(
                                            <__A::Error as _serde::de::Error>::duplicate_field(
                                                "cumQuote",
                                            ),
                                        );
                                    }
                                    __field2 = _serde::__private::Some(
                                        _serde::de::MapAccess::next_value::<String>(&mut __map)?,
                                    );
                                }
                                __Field::__field3 => {
                                    if _serde::__private::Option::is_some(&__field3) {
                                        return _serde::__private::Err(
                                            <__A::Error as _serde::de::Error>::duplicate_field(
                                                "executedQty",
                                            ),
                                        );
                                    }
                                    __field3 = _serde::__private::Some(
                                        _serde::de::MapAccess::next_value::<String>(&mut __map)?,
                                    );
                                }
                                __Field::__field4 => {
                                    if _serde::__private::Option::is_some(&__field4) {
                                        return _serde::__private::Err(
                                            <__A::Error as _serde::de::Error>::duplicate_field(
                                                "orderId",
                                            ),
                                        );
                                    }
                                    __field4 = _serde::__private::Some(
                                        _serde::de::MapAccess::next_value::<i64>(&mut __map)?,
                                    );
                                }
                                __Field::__field5 => {
                                    if _serde::__private::Option::is_some(&__field5) {
                                        return _serde::__private::Err(
                                            <__A::Error as _serde::de::Error>::duplicate_field(
                                                "avgPrice",
                                            ),
                                        );
                                    }
                                    __field5 = _serde::__private::Some(
                                        _serde::de::MapAccess::next_value::<
                                            Option<String>,
                                        >(&mut __map)?,
                                    );
                                }
                                __Field::__field6 => {
                                    if _serde::__private::Option::is_some(&__field6) {
                                        return _serde::__private::Err(
                                            <__A::Error as _serde::de::Error>::duplicate_field(
                                                "origQty",
                                            ),
                                        );
                                    }
                                    __field6 = _serde::__private::Some(
                                        _serde::de::MapAccess::next_value::<String>(&mut __map)?,
                                    );
                                }
                                __Field::__field7 => {
                                    if _serde::__private::Option::is_some(&__field7) {
                                        return _serde::__private::Err(
                                            <__A::Error as _serde::de::Error>::duplicate_field("price"),
                                        );
                                    }
                                    __field7 = _serde::__private::Some(
                                        _serde::de::MapAccess::next_value::<String>(&mut __map)?,
                                    );
                                }
                                __Field::__field8 => {
                                    if _serde::__private::Option::is_some(&__field8) {
                                        return _serde::__private::Err(
                                            <__A::Error as _serde::de::Error>::duplicate_field(
                                                "reduceOnly",
                                            ),
                                        );
                                    }
                                    __field8 = _serde::__private::Some(
                                        _serde::de::MapAccess::next_value::<bool>(&mut __map)?,
                                    );
                                }
                                __Field::__field9 => {
                                    if _serde::__private::Option::is_some(&__field9) {
                                        return _serde::__private::Err(
                                            <__A::Error as _serde::de::Error>::duplicate_field("side"),
                                        );
                                    }
                                    __field9 = _serde::__private::Some(
                                        _serde::de::MapAccess::next_value::<String>(&mut __map)?,
                                    );
                                }
                                __Field::__field10 => {
                                    if _serde::__private::Option::is_some(&__field10) {
                                        return _serde::__private::Err(
                                            <__A::Error as _serde::de::Error>::duplicate_field(
                                                "positionSide",
                                            ),
                                        );
                                    }
                                    __field10 = _serde::__private::Some(
                                        _serde::de::MapAccess::next_value::<
                                            Option<String>,
                                        >(&mut __map)?,
                                    );
                                }
                                __Field::__field11 => {
                                    if _serde::__private::Option::is_some(&__field11) {
                                        return _serde::__private::Err(
                                            <__A::Error as _serde::de::Error>::duplicate_field("status"),
                                        );
                                    }
                                    __field11 = _serde::__private::Some(
                                        _serde::de::MapAccess::next_value::<String>(&mut __map)?,
                                    );
                                }
                                __Field::__field12 => {
                                    if _serde::__private::Option::is_some(&__field12) {
                                        return _serde::__private::Err(
                                            <__A::Error as _serde::de::Error>::duplicate_field(
                                                "stopPrice",
                                            ),
                                        );
                                    }
                                    __field12 = _serde::__private::Some(
                                        _serde::de::MapAccess::next_value::<String>(&mut __map)?,
                                    );
                                }
                                __Field::__field13 => {
                                    if _serde::__private::Option::is_some(&__field13) {
                                        return _serde::__private::Err(
                                            <__A::Error as _serde::de::Error>::duplicate_field(
                                                "closePosition",
                                            ),
                                        );
                                    }
                                    __field13 = _serde::__private::Some(
                                        _serde::de::MapAccess::next_value::<bool>(&mut __map)?,
                                    );
                                }
                                __Field::__field14 => {
                                    if _serde::__private::Option::is_some(&__field14) {
                                        return _serde::__private::Err(
                                            <__A::Error as _serde::de::Error>::duplicate_field("symbol"),
                                        );
                                    }
                                    __field14 = _serde::__private::Some(
                                        _serde::de::MapAccess::next_value::<String>(&mut __map)?,
                                    );
                                }
                                __Field::__field15 => {
                                    if _serde::__private::Option::is_some(&__field15) {
                                        return _serde::__private::Err(
                                            <__A::Error as _serde::de::Error>::duplicate_field(
                                                "timeInForce",
                                            ),
                                        );
                                    }
                                    __field15 = _serde::__private::Some(
                                        _serde::de::MapAccess::next_value::<String>(&mut __map)?,
                                    );
                                }
                                __Field::__field16 => {
                                    if _serde::__private::Option::is_some(&__field16) {
                                        return _serde::__private::Err(
                                            <__A::Error as _serde::de::Error>::duplicate_field("type"),
                                        );
                                    }
                                    __field16 = _serde::__private::Some(
                                        _serde::de::MapAccess::next_value::<String>(&mut __map)?,
                                    );
                                }
                                __Field::__field17 => {
                                    if _serde::__private::Option::is_some(&__field17) {
                                        return _serde::__private::Err(
                                            <__A::Error as _serde::de::Error>::duplicate_field(
                                                "origType",
                                            ),
                                        );
                                    }
                                    __field17 = _serde::__private::Some(
                                        _serde::de::MapAccess::next_value::<String>(&mut __map)?,
                                    );
                                }
                                __Field::__field18 => {
                                    if _serde::__private::Option::is_some(&__field18) {
                                        return _serde::__private::Err(
                                            <__A::Error as _serde::de::Error>::duplicate_field(
                                                "activatePrice",
                                            ),
                                        );
                                    }
                                    __field18 = _serde::__private::Some(
                                        _serde::de::MapAccess::next_value::<
                                            Option<f32>,
                                        >(&mut __map)?,
                                    );
                                }
                                __Field::__field19 => {
                                    if _serde::__private::Option::is_some(&__field19) {
                                        return _serde::__private::Err(
                                            <__A::Error as _serde::de::Error>::duplicate_field(
                                                "priceRate",
                                            ),
                                        );
                                    }
                                    __field19 = _serde::__private::Some(
                                        _serde::de::MapAccess::next_value::<
                                            Option<f32>,
                                        >(&mut __map)?,
                                    );
                                }
                                __Field::__field20 => {
                                    if _serde::__private::Option::is_some(&__field20) {
                                        return _serde::__private::Err(
                                            <__A::Error as _serde::de::Error>::duplicate_field(
                                                "updateTime",
                                            ),
                                        );
                                    }
                                    __field20 = _serde::__private::Some(
                                        _serde::de::MapAccess::next_value::<i64>(&mut __map)?,
                                    );
                                }
                                __Field::__field21 => {
                                    if _serde::__private::Option::is_some(&__field21) {
                                        return _serde::__private::Err(
                                            <__A::Error as _serde::de::Error>::duplicate_field(
                                                "workingType",
                                            ),
                                        );
                                    }
                                    __field21 = _serde::__private::Some(
                                        _serde::de::MapAccess::next_value::<
                                            Option<String>,
                                        >(&mut __map)?,
                                    );
                                }
                                __Field::__field22 => {
                                    if _serde::__private::Option::is_some(&__field22) {
                                        return _serde::__private::Err(
                                            <__A::Error as _serde::de::Error>::duplicate_field(
                                                "priceProtect",
                                            ),
                                        );
                                    }
                                    __field22 = _serde::__private::Some(
                                        _serde::de::MapAccess::next_value::<bool>(&mut __map)?,
                                    );
                                }
                                __Field::__field23 => {
                                    if _serde::__private::Option::is_some(&__field23) {
                                        return _serde::__private::Err(
                                            <__A::Error as _serde::de::Error>::duplicate_field(
                                                "priceMatch",
                                            ),
                                        );
                                    }
                                    __field23 = _serde::__private::Some(
                                        _serde::de::MapAccess::next_value::<
                                            Option<String>,
                                        >(&mut __map)?,
                                    );
                                }
                                __Field::__field24 => {
                                    if _serde::__private::Option::is_some(&__field24) {
                                        return _serde::__private::Err(
                                            <__A::Error as _serde::de::Error>::duplicate_field(
                                                "selfTradePreventionMode",
                                            ),
                                        );
                                    }
                                    __field24 = _serde::__private::Some(
                                        _serde::de::MapAccess::next_value::<
                                            Option<String>,
                                        >(&mut __map)?,
                                    );
                                }
                                __Field::__field25 => {
                                    if _serde::__private::Option::is_some(&__field25) {
                                        return _serde::__private::Err(
                                            <__A::Error as _serde::de::Error>::duplicate_field(
                                                "goodTillDate",
                                            ),
                                        );
                                    }
                                    __field25 = _serde::__private::Some(
                                        _serde::de::MapAccess::next_value::<
                                            Option<i64>,
                                        >(&mut __map)?,
                                    );
                                }
                                _ => {
                                    let _ = _serde::de::MapAccess::next_value::<
                                        _serde::de::IgnoredAny,
                                    >(&mut __map)?;
                                }
                            }
                        }
                        let __field0 = match __field0 {
                            _serde::__private::Some(__field0) => __field0,
                            _serde::__private::None => {
                                _serde::__private::de::missing_field("clientOrderId")?
                            }
                        };
                        let __field1 = match __field1 {
                            _serde::__private::Some(__field1) => __field1,
                            _serde::__private::None => {
                                _serde::__private::de::missing_field("cumQty")?
                            }
                        };
                        let __field2 = match __field2 {
                            _serde::__private::Some(__field2) => __field2,
                            _serde::__private::None => {
                                _serde::__private::de::missing_field("cumQuote")?
                            }
                        };
                        let __field3 = match __field3 {
                            _serde::__private::Some(__field3) => __field3,
                            _serde::__private::None => {
                                _serde::__private::de::missing_field("executedQty")?
                            }
                        };
                        let __field4 = match __field4 {
                            _serde::__private::Some(__field4) => __field4,
                            _serde::__private::None => {
                                _serde::__private::de::missing_field("orderId")?
                            }
                        };
                        let __field5 = match __field5 {
                            _serde::__private::Some(__field5) => __field5,
                            _serde::__private::None => {
                                _serde::__private::de::missing_field("avgPrice")?
                            }
                        };
                        let __field6 = match __field6 {
                            _serde::__private::Some(__field6) => __field6,
                            _serde::__private::None => {
                                _serde::__private::de::missing_field("origQty")?
                            }
                        };
                        let __field7 = match __field7 {
                            _serde::__private::Some(__field7) => __field7,
                            _serde::__private::None => {
                                _serde::__private::de::missing_field("price")?
                            }
                        };
                        let __field8 = match __field8 {
                            _serde::__private::Some(__field8) => __field8,
                            _serde::__private::None => {
                                _serde::__private::de::missing_field("reduceOnly")?
                            }
                        };
                        let __field9 = match __field9 {
                            _serde::__private::Some(__field9) => __field9,
                            _serde::__private::None => {
                                _serde::__private::de::missing_field("side")?
                            }
                        };
                        let __field10 = match __field10 {
                            _serde::__private::Some(__field10) => __field10,
                            _serde::__private::None => {
                                _serde::__private::de::missing_field("positionSide")?
                            }
                        };
                        let __field11 = match __field11 {
                            _serde::__private::Some(__field11) => __field11,
                            _serde::__private::None => {
                                _serde::__private::de::missing_field("status")?
                            }
                        };
                        let __field12 = match __field12 {
                            _serde::__private::Some(__field12) => __field12,
                            _serde::__private::None => {
                                _serde::__private::de::missing_field("stopPrice")?
                            }
                        };
                        let __field13 = match __field13 {
                            _serde::__private::Some(__field13) => __field13,
                            _serde::__private::None => {
                                _serde::__private::de::missing_field("closePosition")?
                            }
                        };
                        let __field14 = match __field14 {
                            _serde::__private::Some(__field14) => __field14,
                            _serde::__private::None => {
                                _serde::__private::de::missing_field("symbol")?
                            }
                        };
                        let __field15 = match __field15 {
                            _serde::__private::Some(__field15) => __field15,
                            _serde::__private::None => {
                                _serde::__private::de::missing_field("timeInForce")?
                            }
                        };
                        let __field16 = match __field16 {
                            _serde::__private::Some(__field16) => __field16,
                            _serde::__private::None => {
                                _serde::__private::de::missing_field("type")?
                            }
                        };
                        let __field17 = match __field17 {
                            _serde::__private::Some(__field17) => __field17,
                            _serde::__private::None => {
                                _serde::__private::de::missing_field("origType")?
                            }
                        };
                        let __field18 = match __field18 {
                            _serde::__private::Some(__field18) => __field18,
                            _serde::__private::None => {
                                _serde::__private::de::missing_field("activatePrice")?
                            }
                        };
                        let __field19 = match __field19 {
                            _serde::__private::Some(__field19) => __field19,
                            _serde::__private::None => {
                                _serde::__private::de::missing_field("priceRate")?
                            }
                        };
                        let __field20 = match __field20 {
                            _serde::__private::Some(__field20) => __field20,
                            _serde::__private::None => {
                                _serde::__private::de::missing_field("updateTime")?
                            }
                        };
                        let __field21 = match __field21 {
                            _serde::__private::Some(__field21) => __field21,
                            _serde::__private::None => {
                                _serde::__private::de::missing_field("workingType")?
                            }
                        };
                        let __field22 = match __field22 {
                            _serde::__private::Some(__field22) => __field22,
                            _serde::__private::None => {
                                _serde::__private::de::missing_field("priceProtect")?
                            }
                        };
                        let __field23 = match __field23 {
                            _serde::__private::Some(__field23) => __field23,
                            _serde::__private::None => {
                                _serde::__private::de::missing_field("priceMatch")?
                            }
                        };
                        let __field24 = match __field24 {
                            _serde::__private::Some(__field24) => __field24,
                            _serde::__private::None => {
                                _serde::__private::de::missing_field(
                                    "selfTradePreventionMode",
                                )?
                            }
                        };
                        let __field25 = match __field25 {
                            _serde::__private::Some(__field25) => __field25,
                            _serde::__private::None => {
                                _serde::__private::de::missing_field("goodTillDate")?
                            }
                        };
                        _serde::__private::Ok(FuturesPositionResponse {
                            clientOrderId: __field0,
                            cumQty: __field1,
                            cumQuote: __field2,
                            executedQty: __field3,
                            orderId: __field4,
                            avgPrice: __field5,
                            origQty: __field6,
                            price: __field7,
                            reduceOnly: __field8,
                            side: __field9,
                            positionSide: __field10,
                            status: __field11,
                            stopPrice: __field12,
                            closePosition: __field13,
                            symbol: __field14,
                            timeInForce: __field15,
                            r#type: __field16,
                            origType: __field17,
                            activatePrice: __field18,
                            priceRate: __field19,
                            updateTime: __field20,
                            workingType: __field21,
                            priceProtect: __field22,
                            priceMatch: __field23,
                            selfTradePreventionMode: __field24,
                            goodTillDate: __field25,
                        })
                    }
                }
                #[doc(hidden)]
                const FIELDS: &'static [&'static str] = &[
                    "clientOrderId",
                    "cumQty",
                    "cumQuote",
                    "executedQty",
                    "orderId",
                    "avgPrice",
                    "origQty",
                    "price",
                    "reduceOnly",
                    "side",
                    "positionSide",
                    "status",
                    "stopPrice",
                    "closePosition",
                    "symbol",
                    "timeInForce",
                    "type",
                    "origType",
                    "activatePrice",
                    "priceRate",
                    "updateTime",
                    "workingType",
                    "priceProtect",
                    "priceMatch",
                    "selfTradePreventionMode",
                    "goodTillDate",
                ];
                _serde::Deserializer::deserialize_struct(
                    __deserializer,
                    "FuturesPositionResponse",
                    FIELDS,
                    __Visitor {
                        marker: _serde::__private::PhantomData::<
                            FuturesPositionResponse,
                        >,
                        lifetime: _serde::__private::PhantomData,
                    },
                )
            }
        }
    };
    #[automatically_derived]
    #[allow(non_snake_case)]
    impl ::core::fmt::Debug for FuturesPositionResponse {
        #[inline]
        fn fmt(&self, f: &mut ::core::fmt::Formatter) -> ::core::fmt::Result {
            let names: &'static _ = &[
                "clientOrderId",
                "cumQty",
                "cumQuote",
                "executedQty",
                "orderId",
                "avgPrice",
                "origQty",
                "price",
                "reduceOnly",
                "side",
                "positionSide",
                "status",
                "stopPrice",
                "closePosition",
                "symbol",
                "timeInForce",
                "type",
                "origType",
                "activatePrice",
                "priceRate",
                "updateTime",
                "workingType",
                "priceProtect",
                "priceMatch",
                "selfTradePreventionMode",
                "goodTillDate",
            ];
            let values: &[&dyn ::core::fmt::Debug] = &[
                &self.clientOrderId,
                &self.cumQty,
                &self.cumQuote,
                &self.executedQty,
                &self.orderId,
                &self.avgPrice,
                &self.origQty,
                &self.price,
                &self.reduceOnly,
                &self.side,
                &self.positionSide,
                &self.status,
                &self.stopPrice,
                &self.closePosition,
                &self.symbol,
                &self.timeInForce,
                &self.r#type,
                &self.origType,
                &self.activatePrice,
                &self.priceRate,
                &self.updateTime,
                &self.workingType,
                &self.priceProtect,
                &self.priceMatch,
                &self.selfTradePreventionMode,
                &&self.goodTillDate,
            ];
            ::core::fmt::Formatter::debug_struct_fields_finish(
                f,
                "FuturesPositionResponse",
                names,
                values,
            )
        }
    }
    impl FuturesPositionResponse {
        pub fn get_url() -> Url {
            let base_url = Market::BinanceFutures.get_base_url();
            base_url.join("/fapi/v1/order").unwrap()
        }
    }
    #[allow(non_snake_case)]
    struct FuturesBalance {
        accountAlias: String,
        asset: String,
        availableBalance: String,
        balance: String,
        crossUnPnl: String,
        crossWalletBalance: String,
        marginAvailable: bool,
        maxWithdrawAmount: String,
        updateTime: Number,
    }
    #[doc(hidden)]
    #[allow(non_upper_case_globals, unused_attributes, unused_qualifications)]
    const _: () = {
        #[allow(unused_extern_crates, clippy::useless_attribute)]
        extern crate serde as _serde;
        #[automatically_derived]
        impl _serde::Serialize for FuturesBalance {
            fn serialize<__S>(
                &self,
                __serializer: __S,
            ) -> _serde::__private::Result<__S::Ok, __S::Error>
            where
                __S: _serde::Serializer,
            {
                let mut __serde_state = _serde::Serializer::serialize_struct(
                    __serializer,
                    "FuturesBalance",
                    false as usize + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1,
                )?;
                _serde::ser::SerializeStruct::serialize_field(
                    &mut __serde_state,
                    "accountAlias",
                    &self.accountAlias,
                )?;
                _serde::ser::SerializeStruct::serialize_field(
                    &mut __serde_state,
                    "asset",
                    &self.asset,
                )?;
                _serde::ser::SerializeStruct::serialize_field(
                    &mut __serde_state,
                    "availableBalance",
                    &self.availableBalance,
                )?;
                _serde::ser::SerializeStruct::serialize_field(
                    &mut __serde_state,
                    "balance",
                    &self.balance,
                )?;
                _serde::ser::SerializeStruct::serialize_field(
                    &mut __serde_state,
                    "crossUnPnl",
                    &self.crossUnPnl,
                )?;
                _serde::ser::SerializeStruct::serialize_field(
                    &mut __serde_state,
                    "crossWalletBalance",
                    &self.crossWalletBalance,
                )?;
                _serde::ser::SerializeStruct::serialize_field(
                    &mut __serde_state,
                    "marginAvailable",
                    &self.marginAvailable,
                )?;
                _serde::ser::SerializeStruct::serialize_field(
                    &mut __serde_state,
                    "maxWithdrawAmount",
                    &self.maxWithdrawAmount,
                )?;
                _serde::ser::SerializeStruct::serialize_field(
                    &mut __serde_state,
                    "updateTime",
                    &self.updateTime,
                )?;
                _serde::ser::SerializeStruct::end(__serde_state)
            }
        }
    };
    #[doc(hidden)]
    #[allow(non_upper_case_globals, unused_attributes, unused_qualifications)]
    const _: () = {
        #[allow(unused_extern_crates, clippy::useless_attribute)]
        extern crate serde as _serde;
        #[automatically_derived]
        impl<'de> _serde::Deserialize<'de> for FuturesBalance {
            fn deserialize<__D>(
                __deserializer: __D,
            ) -> _serde::__private::Result<Self, __D::Error>
            where
                __D: _serde::Deserializer<'de>,
            {
                #[allow(non_camel_case_types)]
                #[doc(hidden)]
                enum __Field {
                    __field0,
                    __field1,
                    __field2,
                    __field3,
                    __field4,
                    __field5,
                    __field6,
                    __field7,
                    __field8,
                    __ignore,
                }
                #[doc(hidden)]
                struct __FieldVisitor;
                impl<'de> _serde::de::Visitor<'de> for __FieldVisitor {
                    type Value = __Field;
                    fn expecting(
                        &self,
                        __formatter: &mut _serde::__private::Formatter,
                    ) -> _serde::__private::fmt::Result {
                        _serde::__private::Formatter::write_str(
                            __formatter,
                            "field identifier",
                        )
                    }
                    fn visit_u64<__E>(
                        self,
                        __value: u64,
                    ) -> _serde::__private::Result<Self::Value, __E>
                    where
                        __E: _serde::de::Error,
                    {
                        match __value {
                            0u64 => _serde::__private::Ok(__Field::__field0),
                            1u64 => _serde::__private::Ok(__Field::__field1),
                            2u64 => _serde::__private::Ok(__Field::__field2),
                            3u64 => _serde::__private::Ok(__Field::__field3),
                            4u64 => _serde::__private::Ok(__Field::__field4),
                            5u64 => _serde::__private::Ok(__Field::__field5),
                            6u64 => _serde::__private::Ok(__Field::__field6),
                            7u64 => _serde::__private::Ok(__Field::__field7),
                            8u64 => _serde::__private::Ok(__Field::__field8),
                            _ => _serde::__private::Ok(__Field::__ignore),
                        }
                    }
                    fn visit_str<__E>(
                        self,
                        __value: &str,
                    ) -> _serde::__private::Result<Self::Value, __E>
                    where
                        __E: _serde::de::Error,
                    {
                        match __value {
                            "accountAlias" => _serde::__private::Ok(__Field::__field0),
                            "asset" => _serde::__private::Ok(__Field::__field1),
                            "availableBalance" => {
                                _serde::__private::Ok(__Field::__field2)
                            }
                            "balance" => _serde::__private::Ok(__Field::__field3),
                            "crossUnPnl" => _serde::__private::Ok(__Field::__field4),
                            "crossWalletBalance" => {
                                _serde::__private::Ok(__Field::__field5)
                            }
                            "marginAvailable" => _serde::__private::Ok(__Field::__field6),
                            "maxWithdrawAmount" => {
                                _serde::__private::Ok(__Field::__field7)
                            }
                            "updateTime" => _serde::__private::Ok(__Field::__field8),
                            _ => _serde::__private::Ok(__Field::__ignore),
                        }
                    }
                    fn visit_bytes<__E>(
                        self,
                        __value: &[u8],
                    ) -> _serde::__private::Result<Self::Value, __E>
                    where
                        __E: _serde::de::Error,
                    {
                        match __value {
                            b"accountAlias" => _serde::__private::Ok(__Field::__field0),
                            b"asset" => _serde::__private::Ok(__Field::__field1),
                            b"availableBalance" => {
                                _serde::__private::Ok(__Field::__field2)
                            }
                            b"balance" => _serde::__private::Ok(__Field::__field3),
                            b"crossUnPnl" => _serde::__private::Ok(__Field::__field4),
                            b"crossWalletBalance" => {
                                _serde::__private::Ok(__Field::__field5)
                            }
                            b"marginAvailable" => {
                                _serde::__private::Ok(__Field::__field6)
                            }
                            b"maxWithdrawAmount" => {
                                _serde::__private::Ok(__Field::__field7)
                            }
                            b"updateTime" => _serde::__private::Ok(__Field::__field8),
                            _ => _serde::__private::Ok(__Field::__ignore),
                        }
                    }
                }
                impl<'de> _serde::Deserialize<'de> for __Field {
                    #[inline]
                    fn deserialize<__D>(
                        __deserializer: __D,
                    ) -> _serde::__private::Result<Self, __D::Error>
                    where
                        __D: _serde::Deserializer<'de>,
                    {
                        _serde::Deserializer::deserialize_identifier(
                            __deserializer,
                            __FieldVisitor,
                        )
                    }
                }
                #[doc(hidden)]
                struct __Visitor<'de> {
                    marker: _serde::__private::PhantomData<FuturesBalance>,
                    lifetime: _serde::__private::PhantomData<&'de ()>,
                }
                impl<'de> _serde::de::Visitor<'de> for __Visitor<'de> {
                    type Value = FuturesBalance;
                    fn expecting(
                        &self,
                        __formatter: &mut _serde::__private::Formatter,
                    ) -> _serde::__private::fmt::Result {
                        _serde::__private::Formatter::write_str(
                            __formatter,
                            "struct FuturesBalance",
                        )
                    }
                    #[inline]
                    fn visit_seq<__A>(
                        self,
                        mut __seq: __A,
                    ) -> _serde::__private::Result<Self::Value, __A::Error>
                    where
                        __A: _serde::de::SeqAccess<'de>,
                    {
                        let __field0 = match _serde::de::SeqAccess::next_element::<
                            String,
                        >(&mut __seq)? {
                            _serde::__private::Some(__value) => __value,
                            _serde::__private::None => {
                                return _serde::__private::Err(
                                    _serde::de::Error::invalid_length(
                                        0usize,
                                        &"struct FuturesBalance with 9 elements",
                                    ),
                                );
                            }
                        };
                        let __field1 = match _serde::de::SeqAccess::next_element::<
                            String,
                        >(&mut __seq)? {
                            _serde::__private::Some(__value) => __value,
                            _serde::__private::None => {
                                return _serde::__private::Err(
                                    _serde::de::Error::invalid_length(
                                        1usize,
                                        &"struct FuturesBalance with 9 elements",
                                    ),
                                );
                            }
                        };
                        let __field2 = match _serde::de::SeqAccess::next_element::<
                            String,
                        >(&mut __seq)? {
                            _serde::__private::Some(__value) => __value,
                            _serde::__private::None => {
                                return _serde::__private::Err(
                                    _serde::de::Error::invalid_length(
                                        2usize,
                                        &"struct FuturesBalance with 9 elements",
                                    ),
                                );
                            }
                        };
                        let __field3 = match _serde::de::SeqAccess::next_element::<
                            String,
                        >(&mut __seq)? {
                            _serde::__private::Some(__value) => __value,
                            _serde::__private::None => {
                                return _serde::__private::Err(
                                    _serde::de::Error::invalid_length(
                                        3usize,
                                        &"struct FuturesBalance with 9 elements",
                                    ),
                                );
                            }
                        };
                        let __field4 = match _serde::de::SeqAccess::next_element::<
                            String,
                        >(&mut __seq)? {
                            _serde::__private::Some(__value) => __value,
                            _serde::__private::None => {
                                return _serde::__private::Err(
                                    _serde::de::Error::invalid_length(
                                        4usize,
                                        &"struct FuturesBalance with 9 elements",
                                    ),
                                );
                            }
                        };
                        let __field5 = match _serde::de::SeqAccess::next_element::<
                            String,
                        >(&mut __seq)? {
                            _serde::__private::Some(__value) => __value,
                            _serde::__private::None => {
                                return _serde::__private::Err(
                                    _serde::de::Error::invalid_length(
                                        5usize,
                                        &"struct FuturesBalance with 9 elements",
                                    ),
                                );
                            }
                        };
                        let __field6 = match _serde::de::SeqAccess::next_element::<
                            bool,
                        >(&mut __seq)? {
                            _serde::__private::Some(__value) => __value,
                            _serde::__private::None => {
                                return _serde::__private::Err(
                                    _serde::de::Error::invalid_length(
                                        6usize,
                                        &"struct FuturesBalance with 9 elements",
                                    ),
                                );
                            }
                        };
                        let __field7 = match _serde::de::SeqAccess::next_element::<
                            String,
                        >(&mut __seq)? {
                            _serde::__private::Some(__value) => __value,
                            _serde::__private::None => {
                                return _serde::__private::Err(
                                    _serde::de::Error::invalid_length(
                                        7usize,
                                        &"struct FuturesBalance with 9 elements",
                                    ),
                                );
                            }
                        };
                        let __field8 = match _serde::de::SeqAccess::next_element::<
                            Number,
                        >(&mut __seq)? {
                            _serde::__private::Some(__value) => __value,
                            _serde::__private::None => {
                                return _serde::__private::Err(
                                    _serde::de::Error::invalid_length(
                                        8usize,
                                        &"struct FuturesBalance with 9 elements",
                                    ),
                                );
                            }
                        };
                        _serde::__private::Ok(FuturesBalance {
                            accountAlias: __field0,
                            asset: __field1,
                            availableBalance: __field2,
                            balance: __field3,
                            crossUnPnl: __field4,
                            crossWalletBalance: __field5,
                            marginAvailable: __field6,
                            maxWithdrawAmount: __field7,
                            updateTime: __field8,
                        })
                    }
                    #[inline]
                    fn visit_map<__A>(
                        self,
                        mut __map: __A,
                    ) -> _serde::__private::Result<Self::Value, __A::Error>
                    where
                        __A: _serde::de::MapAccess<'de>,
                    {
                        let mut __field0: _serde::__private::Option<String> = _serde::__private::None;
                        let mut __field1: _serde::__private::Option<String> = _serde::__private::None;
                        let mut __field2: _serde::__private::Option<String> = _serde::__private::None;
                        let mut __field3: _serde::__private::Option<String> = _serde::__private::None;
                        let mut __field4: _serde::__private::Option<String> = _serde::__private::None;
                        let mut __field5: _serde::__private::Option<String> = _serde::__private::None;
                        let mut __field6: _serde::__private::Option<bool> = _serde::__private::None;
                        let mut __field7: _serde::__private::Option<String> = _serde::__private::None;
                        let mut __field8: _serde::__private::Option<Number> = _serde::__private::None;
                        while let _serde::__private::Some(__key) = _serde::de::MapAccess::next_key::<
                            __Field,
                        >(&mut __map)? {
                            match __key {
                                __Field::__field0 => {
                                    if _serde::__private::Option::is_some(&__field0) {
                                        return _serde::__private::Err(
                                            <__A::Error as _serde::de::Error>::duplicate_field(
                                                "accountAlias",
                                            ),
                                        );
                                    }
                                    __field0 = _serde::__private::Some(
                                        _serde::de::MapAccess::next_value::<String>(&mut __map)?,
                                    );
                                }
                                __Field::__field1 => {
                                    if _serde::__private::Option::is_some(&__field1) {
                                        return _serde::__private::Err(
                                            <__A::Error as _serde::de::Error>::duplicate_field("asset"),
                                        );
                                    }
                                    __field1 = _serde::__private::Some(
                                        _serde::de::MapAccess::next_value::<String>(&mut __map)?,
                                    );
                                }
                                __Field::__field2 => {
                                    if _serde::__private::Option::is_some(&__field2) {
                                        return _serde::__private::Err(
                                            <__A::Error as _serde::de::Error>::duplicate_field(
                                                "availableBalance",
                                            ),
                                        );
                                    }
                                    __field2 = _serde::__private::Some(
                                        _serde::de::MapAccess::next_value::<String>(&mut __map)?,
                                    );
                                }
                                __Field::__field3 => {
                                    if _serde::__private::Option::is_some(&__field3) {
                                        return _serde::__private::Err(
                                            <__A::Error as _serde::de::Error>::duplicate_field(
                                                "balance",
                                            ),
                                        );
                                    }
                                    __field3 = _serde::__private::Some(
                                        _serde::de::MapAccess::next_value::<String>(&mut __map)?,
                                    );
                                }
                                __Field::__field4 => {
                                    if _serde::__private::Option::is_some(&__field4) {
                                        return _serde::__private::Err(
                                            <__A::Error as _serde::de::Error>::duplicate_field(
                                                "crossUnPnl",
                                            ),
                                        );
                                    }
                                    __field4 = _serde::__private::Some(
                                        _serde::de::MapAccess::next_value::<String>(&mut __map)?,
                                    );
                                }
                                __Field::__field5 => {
                                    if _serde::__private::Option::is_some(&__field5) {
                                        return _serde::__private::Err(
                                            <__A::Error as _serde::de::Error>::duplicate_field(
                                                "crossWalletBalance",
                                            ),
                                        );
                                    }
                                    __field5 = _serde::__private::Some(
                                        _serde::de::MapAccess::next_value::<String>(&mut __map)?,
                                    );
                                }
                                __Field::__field6 => {
                                    if _serde::__private::Option::is_some(&__field6) {
                                        return _serde::__private::Err(
                                            <__A::Error as _serde::de::Error>::duplicate_field(
                                                "marginAvailable",
                                            ),
                                        );
                                    }
                                    __field6 = _serde::__private::Some(
                                        _serde::de::MapAccess::next_value::<bool>(&mut __map)?,
                                    );
                                }
                                __Field::__field7 => {
                                    if _serde::__private::Option::is_some(&__field7) {
                                        return _serde::__private::Err(
                                            <__A::Error as _serde::de::Error>::duplicate_field(
                                                "maxWithdrawAmount",
                                            ),
                                        );
                                    }
                                    __field7 = _serde::__private::Some(
                                        _serde::de::MapAccess::next_value::<String>(&mut __map)?,
                                    );
                                }
                                __Field::__field8 => {
                                    if _serde::__private::Option::is_some(&__field8) {
                                        return _serde::__private::Err(
                                            <__A::Error as _serde::de::Error>::duplicate_field(
                                                "updateTime",
                                            ),
                                        );
                                    }
                                    __field8 = _serde::__private::Some(
                                        _serde::de::MapAccess::next_value::<Number>(&mut __map)?,
                                    );
                                }
                                _ => {
                                    let _ = _serde::de::MapAccess::next_value::<
                                        _serde::de::IgnoredAny,
                                    >(&mut __map)?;
                                }
                            }
                        }
                        let __field0 = match __field0 {
                            _serde::__private::Some(__field0) => __field0,
                            _serde::__private::None => {
                                _serde::__private::de::missing_field("accountAlias")?
                            }
                        };
                        let __field1 = match __field1 {
                            _serde::__private::Some(__field1) => __field1,
                            _serde::__private::None => {
                                _serde::__private::de::missing_field("asset")?
                            }
                        };
                        let __field2 = match __field2 {
                            _serde::__private::Some(__field2) => __field2,
                            _serde::__private::None => {
                                _serde::__private::de::missing_field("availableBalance")?
                            }
                        };
                        let __field3 = match __field3 {
                            _serde::__private::Some(__field3) => __field3,
                            _serde::__private::None => {
                                _serde::__private::de::missing_field("balance")?
                            }
                        };
                        let __field4 = match __field4 {
                            _serde::__private::Some(__field4) => __field4,
                            _serde::__private::None => {
                                _serde::__private::de::missing_field("crossUnPnl")?
                            }
                        };
                        let __field5 = match __field5 {
                            _serde::__private::Some(__field5) => __field5,
                            _serde::__private::None => {
                                _serde::__private::de::missing_field("crossWalletBalance")?
                            }
                        };
                        let __field6 = match __field6 {
                            _serde::__private::Some(__field6) => __field6,
                            _serde::__private::None => {
                                _serde::__private::de::missing_field("marginAvailable")?
                            }
                        };
                        let __field7 = match __field7 {
                            _serde::__private::Some(__field7) => __field7,
                            _serde::__private::None => {
                                _serde::__private::de::missing_field("maxWithdrawAmount")?
                            }
                        };
                        let __field8 = match __field8 {
                            _serde::__private::Some(__field8) => __field8,
                            _serde::__private::None => {
                                _serde::__private::de::missing_field("updateTime")?
                            }
                        };
                        _serde::__private::Ok(FuturesBalance {
                            accountAlias: __field0,
                            asset: __field1,
                            availableBalance: __field2,
                            balance: __field3,
                            crossUnPnl: __field4,
                            crossWalletBalance: __field5,
                            marginAvailable: __field6,
                            maxWithdrawAmount: __field7,
                            updateTime: __field8,
                        })
                    }
                }
                #[doc(hidden)]
                const FIELDS: &'static [&'static str] = &[
                    "accountAlias",
                    "asset",
                    "availableBalance",
                    "balance",
                    "crossUnPnl",
                    "crossWalletBalance",
                    "marginAvailable",
                    "maxWithdrawAmount",
                    "updateTime",
                ];
                _serde::Deserializer::deserialize_struct(
                    __deserializer,
                    "FuturesBalance",
                    FIELDS,
                    __Visitor {
                        marker: _serde::__private::PhantomData::<FuturesBalance>,
                        lifetime: _serde::__private::PhantomData,
                    },
                )
            }
        }
    };
    #[automatically_derived]
    #[allow(non_snake_case)]
    impl ::core::fmt::Debug for FuturesBalance {
        #[inline]
        fn fmt(&self, f: &mut ::core::fmt::Formatter) -> ::core::fmt::Result {
            let names: &'static _ = &[
                "accountAlias",
                "asset",
                "availableBalance",
                "balance",
                "crossUnPnl",
                "crossWalletBalance",
                "marginAvailable",
                "maxWithdrawAmount",
                "updateTime",
            ];
            let values: &[&dyn ::core::fmt::Debug] = &[
                &self.accountAlias,
                &self.asset,
                &self.availableBalance,
                &self.balance,
                &self.crossUnPnl,
                &self.crossWalletBalance,
                &self.marginAvailable,
                &self.maxWithdrawAmount,
                &&self.updateTime,
            ];
            ::core::fmt::Formatter::debug_struct_fields_finish(
                f,
                "FuturesBalance",
                names,
                values,
            )
        }
    }
    #[allow(non_snake_case)]
    struct SpotAccountDetails {
        makerCommission: i32,
        takerCommission: i32,
        buyerCommission: i32,
        sellerCommission: i32,
        commissionRates: CommissionRates,
        canTrade: bool,
        canWithdraw: bool,
        canDeposit: bool,
        brokered: bool,
        requireSelfTradePrevention: bool,
        preventSor: bool,
        updateTime: u64,
        accountType: String,
        balances: Vec<SpotBalance>,
        permissions: Vec<String>,
        uid: u64,
    }
    #[doc(hidden)]
    #[allow(non_upper_case_globals, unused_attributes, unused_qualifications)]
    const _: () = {
        #[allow(unused_extern_crates, clippy::useless_attribute)]
        extern crate serde as _serde;
        #[automatically_derived]
        impl _serde::Serialize for SpotAccountDetails {
            fn serialize<__S>(
                &self,
                __serializer: __S,
            ) -> _serde::__private::Result<__S::Ok, __S::Error>
            where
                __S: _serde::Serializer,
            {
                let mut __serde_state = _serde::Serializer::serialize_struct(
                    __serializer,
                    "SpotAccountDetails",
                    false as usize + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1
                        + 1 + 1 + 1,
                )?;
                _serde::ser::SerializeStruct::serialize_field(
                    &mut __serde_state,
                    "makerCommission",
                    &self.makerCommission,
                )?;
                _serde::ser::SerializeStruct::serialize_field(
                    &mut __serde_state,
                    "takerCommission",
                    &self.takerCommission,
                )?;
                _serde::ser::SerializeStruct::serialize_field(
                    &mut __serde_state,
                    "buyerCommission",
                    &self.buyerCommission,
                )?;
                _serde::ser::SerializeStruct::serialize_field(
                    &mut __serde_state,
                    "sellerCommission",
                    &self.sellerCommission,
                )?;
                _serde::ser::SerializeStruct::serialize_field(
                    &mut __serde_state,
                    "commissionRates",
                    &self.commissionRates,
                )?;
                _serde::ser::SerializeStruct::serialize_field(
                    &mut __serde_state,
                    "canTrade",
                    &self.canTrade,
                )?;
                _serde::ser::SerializeStruct::serialize_field(
                    &mut __serde_state,
                    "canWithdraw",
                    &self.canWithdraw,
                )?;
                _serde::ser::SerializeStruct::serialize_field(
                    &mut __serde_state,
                    "canDeposit",
                    &self.canDeposit,
                )?;
                _serde::ser::SerializeStruct::serialize_field(
                    &mut __serde_state,
                    "brokered",
                    &self.brokered,
                )?;
                _serde::ser::SerializeStruct::serialize_field(
                    &mut __serde_state,
                    "requireSelfTradePrevention",
                    &self.requireSelfTradePrevention,
                )?;
                _serde::ser::SerializeStruct::serialize_field(
                    &mut __serde_state,
                    "preventSor",
                    &self.preventSor,
                )?;
                _serde::ser::SerializeStruct::serialize_field(
                    &mut __serde_state,
                    "updateTime",
                    &self.updateTime,
                )?;
                _serde::ser::SerializeStruct::serialize_field(
                    &mut __serde_state,
                    "accountType",
                    &self.accountType,
                )?;
                _serde::ser::SerializeStruct::serialize_field(
                    &mut __serde_state,
                    "balances",
                    &self.balances,
                )?;
                _serde::ser::SerializeStruct::serialize_field(
                    &mut __serde_state,
                    "permissions",
                    &self.permissions,
                )?;
                _serde::ser::SerializeStruct::serialize_field(
                    &mut __serde_state,
                    "uid",
                    &self.uid,
                )?;
                _serde::ser::SerializeStruct::end(__serde_state)
            }
        }
    };
    #[doc(hidden)]
    #[allow(non_upper_case_globals, unused_attributes, unused_qualifications)]
    const _: () = {
        #[allow(unused_extern_crates, clippy::useless_attribute)]
        extern crate serde as _serde;
        #[automatically_derived]
        impl<'de> _serde::Deserialize<'de> for SpotAccountDetails {
            fn deserialize<__D>(
                __deserializer: __D,
            ) -> _serde::__private::Result<Self, __D::Error>
            where
                __D: _serde::Deserializer<'de>,
            {
                #[allow(non_camel_case_types)]
                #[doc(hidden)]
                enum __Field {
                    __field0,
                    __field1,
                    __field2,
                    __field3,
                    __field4,
                    __field5,
                    __field6,
                    __field7,
                    __field8,
                    __field9,
                    __field10,
                    __field11,
                    __field12,
                    __field13,
                    __field14,
                    __field15,
                    __ignore,
                }
                #[doc(hidden)]
                struct __FieldVisitor;
                impl<'de> _serde::de::Visitor<'de> for __FieldVisitor {
                    type Value = __Field;
                    fn expecting(
                        &self,
                        __formatter: &mut _serde::__private::Formatter,
                    ) -> _serde::__private::fmt::Result {
                        _serde::__private::Formatter::write_str(
                            __formatter,
                            "field identifier",
                        )
                    }
                    fn visit_u64<__E>(
                        self,
                        __value: u64,
                    ) -> _serde::__private::Result<Self::Value, __E>
                    where
                        __E: _serde::de::Error,
                    {
                        match __value {
                            0u64 => _serde::__private::Ok(__Field::__field0),
                            1u64 => _serde::__private::Ok(__Field::__field1),
                            2u64 => _serde::__private::Ok(__Field::__field2),
                            3u64 => _serde::__private::Ok(__Field::__field3),
                            4u64 => _serde::__private::Ok(__Field::__field4),
                            5u64 => _serde::__private::Ok(__Field::__field5),
                            6u64 => _serde::__private::Ok(__Field::__field6),
                            7u64 => _serde::__private::Ok(__Field::__field7),
                            8u64 => _serde::__private::Ok(__Field::__field8),
                            9u64 => _serde::__private::Ok(__Field::__field9),
                            10u64 => _serde::__private::Ok(__Field::__field10),
                            11u64 => _serde::__private::Ok(__Field::__field11),
                            12u64 => _serde::__private::Ok(__Field::__field12),
                            13u64 => _serde::__private::Ok(__Field::__field13),
                            14u64 => _serde::__private::Ok(__Field::__field14),
                            15u64 => _serde::__private::Ok(__Field::__field15),
                            _ => _serde::__private::Ok(__Field::__ignore),
                        }
                    }
                    fn visit_str<__E>(
                        self,
                        __value: &str,
                    ) -> _serde::__private::Result<Self::Value, __E>
                    where
                        __E: _serde::de::Error,
                    {
                        match __value {
                            "makerCommission" => _serde::__private::Ok(__Field::__field0),
                            "takerCommission" => _serde::__private::Ok(__Field::__field1),
                            "buyerCommission" => _serde::__private::Ok(__Field::__field2),
                            "sellerCommission" => {
                                _serde::__private::Ok(__Field::__field3)
                            }
                            "commissionRates" => _serde::__private::Ok(__Field::__field4),
                            "canTrade" => _serde::__private::Ok(__Field::__field5),
                            "canWithdraw" => _serde::__private::Ok(__Field::__field6),
                            "canDeposit" => _serde::__private::Ok(__Field::__field7),
                            "brokered" => _serde::__private::Ok(__Field::__field8),
                            "requireSelfTradePrevention" => {
                                _serde::__private::Ok(__Field::__field9)
                            }
                            "preventSor" => _serde::__private::Ok(__Field::__field10),
                            "updateTime" => _serde::__private::Ok(__Field::__field11),
                            "accountType" => _serde::__private::Ok(__Field::__field12),
                            "balances" => _serde::__private::Ok(__Field::__field13),
                            "permissions" => _serde::__private::Ok(__Field::__field14),
                            "uid" => _serde::__private::Ok(__Field::__field15),
                            _ => _serde::__private::Ok(__Field::__ignore),
                        }
                    }
                    fn visit_bytes<__E>(
                        self,
                        __value: &[u8],
                    ) -> _serde::__private::Result<Self::Value, __E>
                    where
                        __E: _serde::de::Error,
                    {
                        match __value {
                            b"makerCommission" => {
                                _serde::__private::Ok(__Field::__field0)
                            }
                            b"takerCommission" => {
                                _serde::__private::Ok(__Field::__field1)
                            }
                            b"buyerCommission" => {
                                _serde::__private::Ok(__Field::__field2)
                            }
                            b"sellerCommission" => {
                                _serde::__private::Ok(__Field::__field3)
                            }
                            b"commissionRates" => {
                                _serde::__private::Ok(__Field::__field4)
                            }
                            b"canTrade" => _serde::__private::Ok(__Field::__field5),
                            b"canWithdraw" => _serde::__private::Ok(__Field::__field6),
                            b"canDeposit" => _serde::__private::Ok(__Field::__field7),
                            b"brokered" => _serde::__private::Ok(__Field::__field8),
                            b"requireSelfTradePrevention" => {
                                _serde::__private::Ok(__Field::__field9)
                            }
                            b"preventSor" => _serde::__private::Ok(__Field::__field10),
                            b"updateTime" => _serde::__private::Ok(__Field::__field11),
                            b"accountType" => _serde::__private::Ok(__Field::__field12),
                            b"balances" => _serde::__private::Ok(__Field::__field13),
                            b"permissions" => _serde::__private::Ok(__Field::__field14),
                            b"uid" => _serde::__private::Ok(__Field::__field15),
                            _ => _serde::__private::Ok(__Field::__ignore),
                        }
                    }
                }
                impl<'de> _serde::Deserialize<'de> for __Field {
                    #[inline]
                    fn deserialize<__D>(
                        __deserializer: __D,
                    ) -> _serde::__private::Result<Self, __D::Error>
                    where
                        __D: _serde::Deserializer<'de>,
                    {
                        _serde::Deserializer::deserialize_identifier(
                            __deserializer,
                            __FieldVisitor,
                        )
                    }
                }
                #[doc(hidden)]
                struct __Visitor<'de> {
                    marker: _serde::__private::PhantomData<SpotAccountDetails>,
                    lifetime: _serde::__private::PhantomData<&'de ()>,
                }
                impl<'de> _serde::de::Visitor<'de> for __Visitor<'de> {
                    type Value = SpotAccountDetails;
                    fn expecting(
                        &self,
                        __formatter: &mut _serde::__private::Formatter,
                    ) -> _serde::__private::fmt::Result {
                        _serde::__private::Formatter::write_str(
                            __formatter,
                            "struct SpotAccountDetails",
                        )
                    }
                    #[inline]
                    fn visit_seq<__A>(
                        self,
                        mut __seq: __A,
                    ) -> _serde::__private::Result<Self::Value, __A::Error>
                    where
                        __A: _serde::de::SeqAccess<'de>,
                    {
                        let __field0 = match _serde::de::SeqAccess::next_element::<
                            i32,
                        >(&mut __seq)? {
                            _serde::__private::Some(__value) => __value,
                            _serde::__private::None => {
                                return _serde::__private::Err(
                                    _serde::de::Error::invalid_length(
                                        0usize,
                                        &"struct SpotAccountDetails with 16 elements",
                                    ),
                                );
                            }
                        };
                        let __field1 = match _serde::de::SeqAccess::next_element::<
                            i32,
                        >(&mut __seq)? {
                            _serde::__private::Some(__value) => __value,
                            _serde::__private::None => {
                                return _serde::__private::Err(
                                    _serde::de::Error::invalid_length(
                                        1usize,
                                        &"struct SpotAccountDetails with 16 elements",
                                    ),
                                );
                            }
                        };
                        let __field2 = match _serde::de::SeqAccess::next_element::<
                            i32,
                        >(&mut __seq)? {
                            _serde::__private::Some(__value) => __value,
                            _serde::__private::None => {
                                return _serde::__private::Err(
                                    _serde::de::Error::invalid_length(
                                        2usize,
                                        &"struct SpotAccountDetails with 16 elements",
                                    ),
                                );
                            }
                        };
                        let __field3 = match _serde::de::SeqAccess::next_element::<
                            i32,
                        >(&mut __seq)? {
                            _serde::__private::Some(__value) => __value,
                            _serde::__private::None => {
                                return _serde::__private::Err(
                                    _serde::de::Error::invalid_length(
                                        3usize,
                                        &"struct SpotAccountDetails with 16 elements",
                                    ),
                                );
                            }
                        };
                        let __field4 = match _serde::de::SeqAccess::next_element::<
                            CommissionRates,
                        >(&mut __seq)? {
                            _serde::__private::Some(__value) => __value,
                            _serde::__private::None => {
                                return _serde::__private::Err(
                                    _serde::de::Error::invalid_length(
                                        4usize,
                                        &"struct SpotAccountDetails with 16 elements",
                                    ),
                                );
                            }
                        };
                        let __field5 = match _serde::de::SeqAccess::next_element::<
                            bool,
                        >(&mut __seq)? {
                            _serde::__private::Some(__value) => __value,
                            _serde::__private::None => {
                                return _serde::__private::Err(
                                    _serde::de::Error::invalid_length(
                                        5usize,
                                        &"struct SpotAccountDetails with 16 elements",
                                    ),
                                );
                            }
                        };
                        let __field6 = match _serde::de::SeqAccess::next_element::<
                            bool,
                        >(&mut __seq)? {
                            _serde::__private::Some(__value) => __value,
                            _serde::__private::None => {
                                return _serde::__private::Err(
                                    _serde::de::Error::invalid_length(
                                        6usize,
                                        &"struct SpotAccountDetails with 16 elements",
                                    ),
                                );
                            }
                        };
                        let __field7 = match _serde::de::SeqAccess::next_element::<
                            bool,
                        >(&mut __seq)? {
                            _serde::__private::Some(__value) => __value,
                            _serde::__private::None => {
                                return _serde::__private::Err(
                                    _serde::de::Error::invalid_length(
                                        7usize,
                                        &"struct SpotAccountDetails with 16 elements",
                                    ),
                                );
                            }
                        };
                        let __field8 = match _serde::de::SeqAccess::next_element::<
                            bool,
                        >(&mut __seq)? {
                            _serde::__private::Some(__value) => __value,
                            _serde::__private::None => {
                                return _serde::__private::Err(
                                    _serde::de::Error::invalid_length(
                                        8usize,
                                        &"struct SpotAccountDetails with 16 elements",
                                    ),
                                );
                            }
                        };
                        let __field9 = match _serde::de::SeqAccess::next_element::<
                            bool,
                        >(&mut __seq)? {
                            _serde::__private::Some(__value) => __value,
                            _serde::__private::None => {
                                return _serde::__private::Err(
                                    _serde::de::Error::invalid_length(
                                        9usize,
                                        &"struct SpotAccountDetails with 16 elements",
                                    ),
                                );
                            }
                        };
                        let __field10 = match _serde::de::SeqAccess::next_element::<
                            bool,
                        >(&mut __seq)? {
                            _serde::__private::Some(__value) => __value,
                            _serde::__private::None => {
                                return _serde::__private::Err(
                                    _serde::de::Error::invalid_length(
                                        10usize,
                                        &"struct SpotAccountDetails with 16 elements",
                                    ),
                                );
                            }
                        };
                        let __field11 = match _serde::de::SeqAccess::next_element::<
                            u64,
                        >(&mut __seq)? {
                            _serde::__private::Some(__value) => __value,
                            _serde::__private::None => {
                                return _serde::__private::Err(
                                    _serde::de::Error::invalid_length(
                                        11usize,
                                        &"struct SpotAccountDetails with 16 elements",
                                    ),
                                );
                            }
                        };
                        let __field12 = match _serde::de::SeqAccess::next_element::<
                            String,
                        >(&mut __seq)? {
                            _serde::__private::Some(__value) => __value,
                            _serde::__private::None => {
                                return _serde::__private::Err(
                                    _serde::de::Error::invalid_length(
                                        12usize,
                                        &"struct SpotAccountDetails with 16 elements",
                                    ),
                                );
                            }
                        };
                        let __field13 = match _serde::de::SeqAccess::next_element::<
                            Vec<SpotBalance>,
                        >(&mut __seq)? {
                            _serde::__private::Some(__value) => __value,
                            _serde::__private::None => {
                                return _serde::__private::Err(
                                    _serde::de::Error::invalid_length(
                                        13usize,
                                        &"struct SpotAccountDetails with 16 elements",
                                    ),
                                );
                            }
                        };
                        let __field14 = match _serde::de::SeqAccess::next_element::<
                            Vec<String>,
                        >(&mut __seq)? {
                            _serde::__private::Some(__value) => __value,
                            _serde::__private::None => {
                                return _serde::__private::Err(
                                    _serde::de::Error::invalid_length(
                                        14usize,
                                        &"struct SpotAccountDetails with 16 elements",
                                    ),
                                );
                            }
                        };
                        let __field15 = match _serde::de::SeqAccess::next_element::<
                            u64,
                        >(&mut __seq)? {
                            _serde::__private::Some(__value) => __value,
                            _serde::__private::None => {
                                return _serde::__private::Err(
                                    _serde::de::Error::invalid_length(
                                        15usize,
                                        &"struct SpotAccountDetails with 16 elements",
                                    ),
                                );
                            }
                        };
                        _serde::__private::Ok(SpotAccountDetails {
                            makerCommission: __field0,
                            takerCommission: __field1,
                            buyerCommission: __field2,
                            sellerCommission: __field3,
                            commissionRates: __field4,
                            canTrade: __field5,
                            canWithdraw: __field6,
                            canDeposit: __field7,
                            brokered: __field8,
                            requireSelfTradePrevention: __field9,
                            preventSor: __field10,
                            updateTime: __field11,
                            accountType: __field12,
                            balances: __field13,
                            permissions: __field14,
                            uid: __field15,
                        })
                    }
                    #[inline]
                    fn visit_map<__A>(
                        self,
                        mut __map: __A,
                    ) -> _serde::__private::Result<Self::Value, __A::Error>
                    where
                        __A: _serde::de::MapAccess<'de>,
                    {
                        let mut __field0: _serde::__private::Option<i32> = _serde::__private::None;
                        let mut __field1: _serde::__private::Option<i32> = _serde::__private::None;
                        let mut __field2: _serde::__private::Option<i32> = _serde::__private::None;
                        let mut __field3: _serde::__private::Option<i32> = _serde::__private::None;
                        let mut __field4: _serde::__private::Option<CommissionRates> = _serde::__private::None;
                        let mut __field5: _serde::__private::Option<bool> = _serde::__private::None;
                        let mut __field6: _serde::__private::Option<bool> = _serde::__private::None;
                        let mut __field7: _serde::__private::Option<bool> = _serde::__private::None;
                        let mut __field8: _serde::__private::Option<bool> = _serde::__private::None;
                        let mut __field9: _serde::__private::Option<bool> = _serde::__private::None;
                        let mut __field10: _serde::__private::Option<bool> = _serde::__private::None;
                        let mut __field11: _serde::__private::Option<u64> = _serde::__private::None;
                        let mut __field12: _serde::__private::Option<String> = _serde::__private::None;
                        let mut __field13: _serde::__private::Option<Vec<SpotBalance>> = _serde::__private::None;
                        let mut __field14: _serde::__private::Option<Vec<String>> = _serde::__private::None;
                        let mut __field15: _serde::__private::Option<u64> = _serde::__private::None;
                        while let _serde::__private::Some(__key) = _serde::de::MapAccess::next_key::<
                            __Field,
                        >(&mut __map)? {
                            match __key {
                                __Field::__field0 => {
                                    if _serde::__private::Option::is_some(&__field0) {
                                        return _serde::__private::Err(
                                            <__A::Error as _serde::de::Error>::duplicate_field(
                                                "makerCommission",
                                            ),
                                        );
                                    }
                                    __field0 = _serde::__private::Some(
                                        _serde::de::MapAccess::next_value::<i32>(&mut __map)?,
                                    );
                                }
                                __Field::__field1 => {
                                    if _serde::__private::Option::is_some(&__field1) {
                                        return _serde::__private::Err(
                                            <__A::Error as _serde::de::Error>::duplicate_field(
                                                "takerCommission",
                                            ),
                                        );
                                    }
                                    __field1 = _serde::__private::Some(
                                        _serde::de::MapAccess::next_value::<i32>(&mut __map)?,
                                    );
                                }
                                __Field::__field2 => {
                                    if _serde::__private::Option::is_some(&__field2) {
                                        return _serde::__private::Err(
                                            <__A::Error as _serde::de::Error>::duplicate_field(
                                                "buyerCommission",
                                            ),
                                        );
                                    }
                                    __field2 = _serde::__private::Some(
                                        _serde::de::MapAccess::next_value::<i32>(&mut __map)?,
                                    );
                                }
                                __Field::__field3 => {
                                    if _serde::__private::Option::is_some(&__field3) {
                                        return _serde::__private::Err(
                                            <__A::Error as _serde::de::Error>::duplicate_field(
                                                "sellerCommission",
                                            ),
                                        );
                                    }
                                    __field3 = _serde::__private::Some(
                                        _serde::de::MapAccess::next_value::<i32>(&mut __map)?,
                                    );
                                }
                                __Field::__field4 => {
                                    if _serde::__private::Option::is_some(&__field4) {
                                        return _serde::__private::Err(
                                            <__A::Error as _serde::de::Error>::duplicate_field(
                                                "commissionRates",
                                            ),
                                        );
                                    }
                                    __field4 = _serde::__private::Some(
                                        _serde::de::MapAccess::next_value::<
                                            CommissionRates,
                                        >(&mut __map)?,
                                    );
                                }
                                __Field::__field5 => {
                                    if _serde::__private::Option::is_some(&__field5) {
                                        return _serde::__private::Err(
                                            <__A::Error as _serde::de::Error>::duplicate_field(
                                                "canTrade",
                                            ),
                                        );
                                    }
                                    __field5 = _serde::__private::Some(
                                        _serde::de::MapAccess::next_value::<bool>(&mut __map)?,
                                    );
                                }
                                __Field::__field6 => {
                                    if _serde::__private::Option::is_some(&__field6) {
                                        return _serde::__private::Err(
                                            <__A::Error as _serde::de::Error>::duplicate_field(
                                                "canWithdraw",
                                            ),
                                        );
                                    }
                                    __field6 = _serde::__private::Some(
                                        _serde::de::MapAccess::next_value::<bool>(&mut __map)?,
                                    );
                                }
                                __Field::__field7 => {
                                    if _serde::__private::Option::is_some(&__field7) {
                                        return _serde::__private::Err(
                                            <__A::Error as _serde::de::Error>::duplicate_field(
                                                "canDeposit",
                                            ),
                                        );
                                    }
                                    __field7 = _serde::__private::Some(
                                        _serde::de::MapAccess::next_value::<bool>(&mut __map)?,
                                    );
                                }
                                __Field::__field8 => {
                                    if _serde::__private::Option::is_some(&__field8) {
                                        return _serde::__private::Err(
                                            <__A::Error as _serde::de::Error>::duplicate_field(
                                                "brokered",
                                            ),
                                        );
                                    }
                                    __field8 = _serde::__private::Some(
                                        _serde::de::MapAccess::next_value::<bool>(&mut __map)?,
                                    );
                                }
                                __Field::__field9 => {
                                    if _serde::__private::Option::is_some(&__field9) {
                                        return _serde::__private::Err(
                                            <__A::Error as _serde::de::Error>::duplicate_field(
                                                "requireSelfTradePrevention",
                                            ),
                                        );
                                    }
                                    __field9 = _serde::__private::Some(
                                        _serde::de::MapAccess::next_value::<bool>(&mut __map)?,
                                    );
                                }
                                __Field::__field10 => {
                                    if _serde::__private::Option::is_some(&__field10) {
                                        return _serde::__private::Err(
                                            <__A::Error as _serde::de::Error>::duplicate_field(
                                                "preventSor",
                                            ),
                                        );
                                    }
                                    __field10 = _serde::__private::Some(
                                        _serde::de::MapAccess::next_value::<bool>(&mut __map)?,
                                    );
                                }
                                __Field::__field11 => {
                                    if _serde::__private::Option::is_some(&__field11) {
                                        return _serde::__private::Err(
                                            <__A::Error as _serde::de::Error>::duplicate_field(
                                                "updateTime",
                                            ),
                                        );
                                    }
                                    __field11 = _serde::__private::Some(
                                        _serde::de::MapAccess::next_value::<u64>(&mut __map)?,
                                    );
                                }
                                __Field::__field12 => {
                                    if _serde::__private::Option::is_some(&__field12) {
                                        return _serde::__private::Err(
                                            <__A::Error as _serde::de::Error>::duplicate_field(
                                                "accountType",
                                            ),
                                        );
                                    }
                                    __field12 = _serde::__private::Some(
                                        _serde::de::MapAccess::next_value::<String>(&mut __map)?,
                                    );
                                }
                                __Field::__field13 => {
                                    if _serde::__private::Option::is_some(&__field13) {
                                        return _serde::__private::Err(
                                            <__A::Error as _serde::de::Error>::duplicate_field(
                                                "balances",
                                            ),
                                        );
                                    }
                                    __field13 = _serde::__private::Some(
                                        _serde::de::MapAccess::next_value::<
                                            Vec<SpotBalance>,
                                        >(&mut __map)?,
                                    );
                                }
                                __Field::__field14 => {
                                    if _serde::__private::Option::is_some(&__field14) {
                                        return _serde::__private::Err(
                                            <__A::Error as _serde::de::Error>::duplicate_field(
                                                "permissions",
                                            ),
                                        );
                                    }
                                    __field14 = _serde::__private::Some(
                                        _serde::de::MapAccess::next_value::<
                                            Vec<String>,
                                        >(&mut __map)?,
                                    );
                                }
                                __Field::__field15 => {
                                    if _serde::__private::Option::is_some(&__field15) {
                                        return _serde::__private::Err(
                                            <__A::Error as _serde::de::Error>::duplicate_field("uid"),
                                        );
                                    }
                                    __field15 = _serde::__private::Some(
                                        _serde::de::MapAccess::next_value::<u64>(&mut __map)?,
                                    );
                                }
                                _ => {
                                    let _ = _serde::de::MapAccess::next_value::<
                                        _serde::de::IgnoredAny,
                                    >(&mut __map)?;
                                }
                            }
                        }
                        let __field0 = match __field0 {
                            _serde::__private::Some(__field0) => __field0,
                            _serde::__private::None => {
                                _serde::__private::de::missing_field("makerCommission")?
                            }
                        };
                        let __field1 = match __field1 {
                            _serde::__private::Some(__field1) => __field1,
                            _serde::__private::None => {
                                _serde::__private::de::missing_field("takerCommission")?
                            }
                        };
                        let __field2 = match __field2 {
                            _serde::__private::Some(__field2) => __field2,
                            _serde::__private::None => {
                                _serde::__private::de::missing_field("buyerCommission")?
                            }
                        };
                        let __field3 = match __field3 {
                            _serde::__private::Some(__field3) => __field3,
                            _serde::__private::None => {
                                _serde::__private::de::missing_field("sellerCommission")?
                            }
                        };
                        let __field4 = match __field4 {
                            _serde::__private::Some(__field4) => __field4,
                            _serde::__private::None => {
                                _serde::__private::de::missing_field("commissionRates")?
                            }
                        };
                        let __field5 = match __field5 {
                            _serde::__private::Some(__field5) => __field5,
                            _serde::__private::None => {
                                _serde::__private::de::missing_field("canTrade")?
                            }
                        };
                        let __field6 = match __field6 {
                            _serde::__private::Some(__field6) => __field6,
                            _serde::__private::None => {
                                _serde::__private::de::missing_field("canWithdraw")?
                            }
                        };
                        let __field7 = match __field7 {
                            _serde::__private::Some(__field7) => __field7,
                            _serde::__private::None => {
                                _serde::__private::de::missing_field("canDeposit")?
                            }
                        };
                        let __field8 = match __field8 {
                            _serde::__private::Some(__field8) => __field8,
                            _serde::__private::None => {
                                _serde::__private::de::missing_field("brokered")?
                            }
                        };
                        let __field9 = match __field9 {
                            _serde::__private::Some(__field9) => __field9,
                            _serde::__private::None => {
                                _serde::__private::de::missing_field(
                                    "requireSelfTradePrevention",
                                )?
                            }
                        };
                        let __field10 = match __field10 {
                            _serde::__private::Some(__field10) => __field10,
                            _serde::__private::None => {
                                _serde::__private::de::missing_field("preventSor")?
                            }
                        };
                        let __field11 = match __field11 {
                            _serde::__private::Some(__field11) => __field11,
                            _serde::__private::None => {
                                _serde::__private::de::missing_field("updateTime")?
                            }
                        };
                        let __field12 = match __field12 {
                            _serde::__private::Some(__field12) => __field12,
                            _serde::__private::None => {
                                _serde::__private::de::missing_field("accountType")?
                            }
                        };
                        let __field13 = match __field13 {
                            _serde::__private::Some(__field13) => __field13,
                            _serde::__private::None => {
                                _serde::__private::de::missing_field("balances")?
                            }
                        };
                        let __field14 = match __field14 {
                            _serde::__private::Some(__field14) => __field14,
                            _serde::__private::None => {
                                _serde::__private::de::missing_field("permissions")?
                            }
                        };
                        let __field15 = match __field15 {
                            _serde::__private::Some(__field15) => __field15,
                            _serde::__private::None => {
                                _serde::__private::de::missing_field("uid")?
                            }
                        };
                        _serde::__private::Ok(SpotAccountDetails {
                            makerCommission: __field0,
                            takerCommission: __field1,
                            buyerCommission: __field2,
                            sellerCommission: __field3,
                            commissionRates: __field4,
                            canTrade: __field5,
                            canWithdraw: __field6,
                            canDeposit: __field7,
                            brokered: __field8,
                            requireSelfTradePrevention: __field9,
                            preventSor: __field10,
                            updateTime: __field11,
                            accountType: __field12,
                            balances: __field13,
                            permissions: __field14,
                            uid: __field15,
                        })
                    }
                }
                #[doc(hidden)]
                const FIELDS: &'static [&'static str] = &[
                    "makerCommission",
                    "takerCommission",
                    "buyerCommission",
                    "sellerCommission",
                    "commissionRates",
                    "canTrade",
                    "canWithdraw",
                    "canDeposit",
                    "brokered",
                    "requireSelfTradePrevention",
                    "preventSor",
                    "updateTime",
                    "accountType",
                    "balances",
                    "permissions",
                    "uid",
                ];
                _serde::Deserializer::deserialize_struct(
                    __deserializer,
                    "SpotAccountDetails",
                    FIELDS,
                    __Visitor {
                        marker: _serde::__private::PhantomData::<SpotAccountDetails>,
                        lifetime: _serde::__private::PhantomData,
                    },
                )
            }
        }
    };
    #[automatically_derived]
    #[allow(non_snake_case)]
    impl ::core::fmt::Debug for SpotAccountDetails {
        #[inline]
        fn fmt(&self, f: &mut ::core::fmt::Formatter) -> ::core::fmt::Result {
            let names: &'static _ = &[
                "makerCommission",
                "takerCommission",
                "buyerCommission",
                "sellerCommission",
                "commissionRates",
                "canTrade",
                "canWithdraw",
                "canDeposit",
                "brokered",
                "requireSelfTradePrevention",
                "preventSor",
                "updateTime",
                "accountType",
                "balances",
                "permissions",
                "uid",
            ];
            let values: &[&dyn ::core::fmt::Debug] = &[
                &self.makerCommission,
                &self.takerCommission,
                &self.buyerCommission,
                &self.sellerCommission,
                &self.commissionRates,
                &self.canTrade,
                &self.canWithdraw,
                &self.canDeposit,
                &self.brokered,
                &self.requireSelfTradePrevention,
                &self.preventSor,
                &self.updateTime,
                &self.accountType,
                &self.balances,
                &self.permissions,
                &&self.uid,
            ];
            ::core::fmt::Formatter::debug_struct_fields_finish(
                f,
                "SpotAccountDetails",
                names,
                values,
            )
        }
    }
    struct CommissionRates {
        maker: String,
        taker: String,
        buyer: String,
        seller: String,
    }
    #[doc(hidden)]
    #[allow(non_upper_case_globals, unused_attributes, unused_qualifications)]
    const _: () = {
        #[allow(unused_extern_crates, clippy::useless_attribute)]
        extern crate serde as _serde;
        #[automatically_derived]
        impl _serde::Serialize for CommissionRates {
            fn serialize<__S>(
                &self,
                __serializer: __S,
            ) -> _serde::__private::Result<__S::Ok, __S::Error>
            where
                __S: _serde::Serializer,
            {
                let mut __serde_state = _serde::Serializer::serialize_struct(
                    __serializer,
                    "CommissionRates",
                    false as usize + 1 + 1 + 1 + 1,
                )?;
                _serde::ser::SerializeStruct::serialize_field(
                    &mut __serde_state,
                    "maker",
                    &self.maker,
                )?;
                _serde::ser::SerializeStruct::serialize_field(
                    &mut __serde_state,
                    "taker",
                    &self.taker,
                )?;
                _serde::ser::SerializeStruct::serialize_field(
                    &mut __serde_state,
                    "buyer",
                    &self.buyer,
                )?;
                _serde::ser::SerializeStruct::serialize_field(
                    &mut __serde_state,
                    "seller",
                    &self.seller,
                )?;
                _serde::ser::SerializeStruct::end(__serde_state)
            }
        }
    };
    #[doc(hidden)]
    #[allow(non_upper_case_globals, unused_attributes, unused_qualifications)]
    const _: () = {
        #[allow(unused_extern_crates, clippy::useless_attribute)]
        extern crate serde as _serde;
        #[automatically_derived]
        impl<'de> _serde::Deserialize<'de> for CommissionRates {
            fn deserialize<__D>(
                __deserializer: __D,
            ) -> _serde::__private::Result<Self, __D::Error>
            where
                __D: _serde::Deserializer<'de>,
            {
                #[allow(non_camel_case_types)]
                #[doc(hidden)]
                enum __Field {
                    __field0,
                    __field1,
                    __field2,
                    __field3,
                    __ignore,
                }
                #[doc(hidden)]
                struct __FieldVisitor;
                impl<'de> _serde::de::Visitor<'de> for __FieldVisitor {
                    type Value = __Field;
                    fn expecting(
                        &self,
                        __formatter: &mut _serde::__private::Formatter,
                    ) -> _serde::__private::fmt::Result {
                        _serde::__private::Formatter::write_str(
                            __formatter,
                            "field identifier",
                        )
                    }
                    fn visit_u64<__E>(
                        self,
                        __value: u64,
                    ) -> _serde::__private::Result<Self::Value, __E>
                    where
                        __E: _serde::de::Error,
                    {
                        match __value {
                            0u64 => _serde::__private::Ok(__Field::__field0),
                            1u64 => _serde::__private::Ok(__Field::__field1),
                            2u64 => _serde::__private::Ok(__Field::__field2),
                            3u64 => _serde::__private::Ok(__Field::__field3),
                            _ => _serde::__private::Ok(__Field::__ignore),
                        }
                    }
                    fn visit_str<__E>(
                        self,
                        __value: &str,
                    ) -> _serde::__private::Result<Self::Value, __E>
                    where
                        __E: _serde::de::Error,
                    {
                        match __value {
                            "maker" => _serde::__private::Ok(__Field::__field0),
                            "taker" => _serde::__private::Ok(__Field::__field1),
                            "buyer" => _serde::__private::Ok(__Field::__field2),
                            "seller" => _serde::__private::Ok(__Field::__field3),
                            _ => _serde::__private::Ok(__Field::__ignore),
                        }
                    }
                    fn visit_bytes<__E>(
                        self,
                        __value: &[u8],
                    ) -> _serde::__private::Result<Self::Value, __E>
                    where
                        __E: _serde::de::Error,
                    {
                        match __value {
                            b"maker" => _serde::__private::Ok(__Field::__field0),
                            b"taker" => _serde::__private::Ok(__Field::__field1),
                            b"buyer" => _serde::__private::Ok(__Field::__field2),
                            b"seller" => _serde::__private::Ok(__Field::__field3),
                            _ => _serde::__private::Ok(__Field::__ignore),
                        }
                    }
                }
                impl<'de> _serde::Deserialize<'de> for __Field {
                    #[inline]
                    fn deserialize<__D>(
                        __deserializer: __D,
                    ) -> _serde::__private::Result<Self, __D::Error>
                    where
                        __D: _serde::Deserializer<'de>,
                    {
                        _serde::Deserializer::deserialize_identifier(
                            __deserializer,
                            __FieldVisitor,
                        )
                    }
                }
                #[doc(hidden)]
                struct __Visitor<'de> {
                    marker: _serde::__private::PhantomData<CommissionRates>,
                    lifetime: _serde::__private::PhantomData<&'de ()>,
                }
                impl<'de> _serde::de::Visitor<'de> for __Visitor<'de> {
                    type Value = CommissionRates;
                    fn expecting(
                        &self,
                        __formatter: &mut _serde::__private::Formatter,
                    ) -> _serde::__private::fmt::Result {
                        _serde::__private::Formatter::write_str(
                            __formatter,
                            "struct CommissionRates",
                        )
                    }
                    #[inline]
                    fn visit_seq<__A>(
                        self,
                        mut __seq: __A,
                    ) -> _serde::__private::Result<Self::Value, __A::Error>
                    where
                        __A: _serde::de::SeqAccess<'de>,
                    {
                        let __field0 = match _serde::de::SeqAccess::next_element::<
                            String,
                        >(&mut __seq)? {
                            _serde::__private::Some(__value) => __value,
                            _serde::__private::None => {
                                return _serde::__private::Err(
                                    _serde::de::Error::invalid_length(
                                        0usize,
                                        &"struct CommissionRates with 4 elements",
                                    ),
                                );
                            }
                        };
                        let __field1 = match _serde::de::SeqAccess::next_element::<
                            String,
                        >(&mut __seq)? {
                            _serde::__private::Some(__value) => __value,
                            _serde::__private::None => {
                                return _serde::__private::Err(
                                    _serde::de::Error::invalid_length(
                                        1usize,
                                        &"struct CommissionRates with 4 elements",
                                    ),
                                );
                            }
                        };
                        let __field2 = match _serde::de::SeqAccess::next_element::<
                            String,
                        >(&mut __seq)? {
                            _serde::__private::Some(__value) => __value,
                            _serde::__private::None => {
                                return _serde::__private::Err(
                                    _serde::de::Error::invalid_length(
                                        2usize,
                                        &"struct CommissionRates with 4 elements",
                                    ),
                                );
                            }
                        };
                        let __field3 = match _serde::de::SeqAccess::next_element::<
                            String,
                        >(&mut __seq)? {
                            _serde::__private::Some(__value) => __value,
                            _serde::__private::None => {
                                return _serde::__private::Err(
                                    _serde::de::Error::invalid_length(
                                        3usize,
                                        &"struct CommissionRates with 4 elements",
                                    ),
                                );
                            }
                        };
                        _serde::__private::Ok(CommissionRates {
                            maker: __field0,
                            taker: __field1,
                            buyer: __field2,
                            seller: __field3,
                        })
                    }
                    #[inline]
                    fn visit_map<__A>(
                        self,
                        mut __map: __A,
                    ) -> _serde::__private::Result<Self::Value, __A::Error>
                    where
                        __A: _serde::de::MapAccess<'de>,
                    {
                        let mut __field0: _serde::__private::Option<String> = _serde::__private::None;
                        let mut __field1: _serde::__private::Option<String> = _serde::__private::None;
                        let mut __field2: _serde::__private::Option<String> = _serde::__private::None;
                        let mut __field3: _serde::__private::Option<String> = _serde::__private::None;
                        while let _serde::__private::Some(__key) = _serde::de::MapAccess::next_key::<
                            __Field,
                        >(&mut __map)? {
                            match __key {
                                __Field::__field0 => {
                                    if _serde::__private::Option::is_some(&__field0) {
                                        return _serde::__private::Err(
                                            <__A::Error as _serde::de::Error>::duplicate_field("maker"),
                                        );
                                    }
                                    __field0 = _serde::__private::Some(
                                        _serde::de::MapAccess::next_value::<String>(&mut __map)?,
                                    );
                                }
                                __Field::__field1 => {
                                    if _serde::__private::Option::is_some(&__field1) {
                                        return _serde::__private::Err(
                                            <__A::Error as _serde::de::Error>::duplicate_field("taker"),
                                        );
                                    }
                                    __field1 = _serde::__private::Some(
                                        _serde::de::MapAccess::next_value::<String>(&mut __map)?,
                                    );
                                }
                                __Field::__field2 => {
                                    if _serde::__private::Option::is_some(&__field2) {
                                        return _serde::__private::Err(
                                            <__A::Error as _serde::de::Error>::duplicate_field("buyer"),
                                        );
                                    }
                                    __field2 = _serde::__private::Some(
                                        _serde::de::MapAccess::next_value::<String>(&mut __map)?,
                                    );
                                }
                                __Field::__field3 => {
                                    if _serde::__private::Option::is_some(&__field3) {
                                        return _serde::__private::Err(
                                            <__A::Error as _serde::de::Error>::duplicate_field("seller"),
                                        );
                                    }
                                    __field3 = _serde::__private::Some(
                                        _serde::de::MapAccess::next_value::<String>(&mut __map)?,
                                    );
                                }
                                _ => {
                                    let _ = _serde::de::MapAccess::next_value::<
                                        _serde::de::IgnoredAny,
                                    >(&mut __map)?;
                                }
                            }
                        }
                        let __field0 = match __field0 {
                            _serde::__private::Some(__field0) => __field0,
                            _serde::__private::None => {
                                _serde::__private::de::missing_field("maker")?
                            }
                        };
                        let __field1 = match __field1 {
                            _serde::__private::Some(__field1) => __field1,
                            _serde::__private::None => {
                                _serde::__private::de::missing_field("taker")?
                            }
                        };
                        let __field2 = match __field2 {
                            _serde::__private::Some(__field2) => __field2,
                            _serde::__private::None => {
                                _serde::__private::de::missing_field("buyer")?
                            }
                        };
                        let __field3 = match __field3 {
                            _serde::__private::Some(__field3) => __field3,
                            _serde::__private::None => {
                                _serde::__private::de::missing_field("seller")?
                            }
                        };
                        _serde::__private::Ok(CommissionRates {
                            maker: __field0,
                            taker: __field1,
                            buyer: __field2,
                            seller: __field3,
                        })
                    }
                }
                #[doc(hidden)]
                const FIELDS: &'static [&'static str] = &[
                    "maker",
                    "taker",
                    "buyer",
                    "seller",
                ];
                _serde::Deserializer::deserialize_struct(
                    __deserializer,
                    "CommissionRates",
                    FIELDS,
                    __Visitor {
                        marker: _serde::__private::PhantomData::<CommissionRates>,
                        lifetime: _serde::__private::PhantomData,
                    },
                )
            }
        }
    };
    #[automatically_derived]
    impl ::core::fmt::Debug for CommissionRates {
        #[inline]
        fn fmt(&self, f: &mut ::core::fmt::Formatter) -> ::core::fmt::Result {
            ::core::fmt::Formatter::debug_struct_field4_finish(
                f,
                "CommissionRates",
                "maker",
                &self.maker,
                "taker",
                &self.taker,
                "buyer",
                &self.buyer,
                "seller",
                &&self.seller,
            )
        }
    }
    struct SpotBalance {
        asset: String,
        free: String,
        locked: String,
    }
    #[doc(hidden)]
    #[allow(non_upper_case_globals, unused_attributes, unused_qualifications)]
    const _: () = {
        #[allow(unused_extern_crates, clippy::useless_attribute)]
        extern crate serde as _serde;
        #[automatically_derived]
        impl _serde::Serialize for SpotBalance {
            fn serialize<__S>(
                &self,
                __serializer: __S,
            ) -> _serde::__private::Result<__S::Ok, __S::Error>
            where
                __S: _serde::Serializer,
            {
                let mut __serde_state = _serde::Serializer::serialize_struct(
                    __serializer,
                    "SpotBalance",
                    false as usize + 1 + 1 + 1,
                )?;
                _serde::ser::SerializeStruct::serialize_field(
                    &mut __serde_state,
                    "asset",
                    &self.asset,
                )?;
                _serde::ser::SerializeStruct::serialize_field(
                    &mut __serde_state,
                    "free",
                    &self.free,
                )?;
                _serde::ser::SerializeStruct::serialize_field(
                    &mut __serde_state,
                    "locked",
                    &self.locked,
                )?;
                _serde::ser::SerializeStruct::end(__serde_state)
            }
        }
    };
    #[doc(hidden)]
    #[allow(non_upper_case_globals, unused_attributes, unused_qualifications)]
    const _: () = {
        #[allow(unused_extern_crates, clippy::useless_attribute)]
        extern crate serde as _serde;
        #[automatically_derived]
        impl<'de> _serde::Deserialize<'de> for SpotBalance {
            fn deserialize<__D>(
                __deserializer: __D,
            ) -> _serde::__private::Result<Self, __D::Error>
            where
                __D: _serde::Deserializer<'de>,
            {
                #[allow(non_camel_case_types)]
                #[doc(hidden)]
                enum __Field {
                    __field0,
                    __field1,
                    __field2,
                    __ignore,
                }
                #[doc(hidden)]
                struct __FieldVisitor;
                impl<'de> _serde::de::Visitor<'de> for __FieldVisitor {
                    type Value = __Field;
                    fn expecting(
                        &self,
                        __formatter: &mut _serde::__private::Formatter,
                    ) -> _serde::__private::fmt::Result {
                        _serde::__private::Formatter::write_str(
                            __formatter,
                            "field identifier",
                        )
                    }
                    fn visit_u64<__E>(
                        self,
                        __value: u64,
                    ) -> _serde::__private::Result<Self::Value, __E>
                    where
                        __E: _serde::de::Error,
                    {
                        match __value {
                            0u64 => _serde::__private::Ok(__Field::__field0),
                            1u64 => _serde::__private::Ok(__Field::__field1),
                            2u64 => _serde::__private::Ok(__Field::__field2),
                            _ => _serde::__private::Ok(__Field::__ignore),
                        }
                    }
                    fn visit_str<__E>(
                        self,
                        __value: &str,
                    ) -> _serde::__private::Result<Self::Value, __E>
                    where
                        __E: _serde::de::Error,
                    {
                        match __value {
                            "asset" => _serde::__private::Ok(__Field::__field0),
                            "free" => _serde::__private::Ok(__Field::__field1),
                            "locked" => _serde::__private::Ok(__Field::__field2),
                            _ => _serde::__private::Ok(__Field::__ignore),
                        }
                    }
                    fn visit_bytes<__E>(
                        self,
                        __value: &[u8],
                    ) -> _serde::__private::Result<Self::Value, __E>
                    where
                        __E: _serde::de::Error,
                    {
                        match __value {
                            b"asset" => _serde::__private::Ok(__Field::__field0),
                            b"free" => _serde::__private::Ok(__Field::__field1),
                            b"locked" => _serde::__private::Ok(__Field::__field2),
                            _ => _serde::__private::Ok(__Field::__ignore),
                        }
                    }
                }
                impl<'de> _serde::Deserialize<'de> for __Field {
                    #[inline]
                    fn deserialize<__D>(
                        __deserializer: __D,
                    ) -> _serde::__private::Result<Self, __D::Error>
                    where
                        __D: _serde::Deserializer<'de>,
                    {
                        _serde::Deserializer::deserialize_identifier(
                            __deserializer,
                            __FieldVisitor,
                        )
                    }
                }
                #[doc(hidden)]
                struct __Visitor<'de> {
                    marker: _serde::__private::PhantomData<SpotBalance>,
                    lifetime: _serde::__private::PhantomData<&'de ()>,
                }
                impl<'de> _serde::de::Visitor<'de> for __Visitor<'de> {
                    type Value = SpotBalance;
                    fn expecting(
                        &self,
                        __formatter: &mut _serde::__private::Formatter,
                    ) -> _serde::__private::fmt::Result {
                        _serde::__private::Formatter::write_str(
                            __formatter,
                            "struct SpotBalance",
                        )
                    }
                    #[inline]
                    fn visit_seq<__A>(
                        self,
                        mut __seq: __A,
                    ) -> _serde::__private::Result<Self::Value, __A::Error>
                    where
                        __A: _serde::de::SeqAccess<'de>,
                    {
                        let __field0 = match _serde::de::SeqAccess::next_element::<
                            String,
                        >(&mut __seq)? {
                            _serde::__private::Some(__value) => __value,
                            _serde::__private::None => {
                                return _serde::__private::Err(
                                    _serde::de::Error::invalid_length(
                                        0usize,
                                        &"struct SpotBalance with 3 elements",
                                    ),
                                );
                            }
                        };
                        let __field1 = match _serde::de::SeqAccess::next_element::<
                            String,
                        >(&mut __seq)? {
                            _serde::__private::Some(__value) => __value,
                            _serde::__private::None => {
                                return _serde::__private::Err(
                                    _serde::de::Error::invalid_length(
                                        1usize,
                                        &"struct SpotBalance with 3 elements",
                                    ),
                                );
                            }
                        };
                        let __field2 = match _serde::de::SeqAccess::next_element::<
                            String,
                        >(&mut __seq)? {
                            _serde::__private::Some(__value) => __value,
                            _serde::__private::None => {
                                return _serde::__private::Err(
                                    _serde::de::Error::invalid_length(
                                        2usize,
                                        &"struct SpotBalance with 3 elements",
                                    ),
                                );
                            }
                        };
                        _serde::__private::Ok(SpotBalance {
                            asset: __field0,
                            free: __field1,
                            locked: __field2,
                        })
                    }
                    #[inline]
                    fn visit_map<__A>(
                        self,
                        mut __map: __A,
                    ) -> _serde::__private::Result<Self::Value, __A::Error>
                    where
                        __A: _serde::de::MapAccess<'de>,
                    {
                        let mut __field0: _serde::__private::Option<String> = _serde::__private::None;
                        let mut __field1: _serde::__private::Option<String> = _serde::__private::None;
                        let mut __field2: _serde::__private::Option<String> = _serde::__private::None;
                        while let _serde::__private::Some(__key) = _serde::de::MapAccess::next_key::<
                            __Field,
                        >(&mut __map)? {
                            match __key {
                                __Field::__field0 => {
                                    if _serde::__private::Option::is_some(&__field0) {
                                        return _serde::__private::Err(
                                            <__A::Error as _serde::de::Error>::duplicate_field("asset"),
                                        );
                                    }
                                    __field0 = _serde::__private::Some(
                                        _serde::de::MapAccess::next_value::<String>(&mut __map)?,
                                    );
                                }
                                __Field::__field1 => {
                                    if _serde::__private::Option::is_some(&__field1) {
                                        return _serde::__private::Err(
                                            <__A::Error as _serde::de::Error>::duplicate_field("free"),
                                        );
                                    }
                                    __field1 = _serde::__private::Some(
                                        _serde::de::MapAccess::next_value::<String>(&mut __map)?,
                                    );
                                }
                                __Field::__field2 => {
                                    if _serde::__private::Option::is_some(&__field2) {
                                        return _serde::__private::Err(
                                            <__A::Error as _serde::de::Error>::duplicate_field("locked"),
                                        );
                                    }
                                    __field2 = _serde::__private::Some(
                                        _serde::de::MapAccess::next_value::<String>(&mut __map)?,
                                    );
                                }
                                _ => {
                                    let _ = _serde::de::MapAccess::next_value::<
                                        _serde::de::IgnoredAny,
                                    >(&mut __map)?;
                                }
                            }
                        }
                        let __field0 = match __field0 {
                            _serde::__private::Some(__field0) => __field0,
                            _serde::__private::None => {
                                _serde::__private::de::missing_field("asset")?
                            }
                        };
                        let __field1 = match __field1 {
                            _serde::__private::Some(__field1) => __field1,
                            _serde::__private::None => {
                                _serde::__private::de::missing_field("free")?
                            }
                        };
                        let __field2 = match __field2 {
                            _serde::__private::Some(__field2) => __field2,
                            _serde::__private::None => {
                                _serde::__private::de::missing_field("locked")?
                            }
                        };
                        _serde::__private::Ok(SpotBalance {
                            asset: __field0,
                            free: __field1,
                            locked: __field2,
                        })
                    }
                }
                #[doc(hidden)]
                const FIELDS: &'static [&'static str] = &["asset", "free", "locked"];
                _serde::Deserializer::deserialize_struct(
                    __deserializer,
                    "SpotBalance",
                    FIELDS,
                    __Visitor {
                        marker: _serde::__private::PhantomData::<SpotBalance>,
                        lifetime: _serde::__private::PhantomData,
                    },
                )
            }
        }
    };
    #[automatically_derived]
    impl ::core::fmt::Debug for SpotBalance {
        #[inline]
        fn fmt(&self, f: &mut ::core::fmt::Formatter) -> ::core::fmt::Result {
            ::core::fmt::Formatter::debug_struct_field3_finish(
                f,
                "SpotBalance",
                "asset",
                &self.asset,
                "free",
                &self.free,
                "locked",
                &&self.locked,
            )
        }
    }
    #[allow(non_snake_case)]
    struct MarginAccountDetails {
        borrowEnabled: bool,
        marginLevel: String,
        CollateralMarginLevel: String,
        totalAssetOfBtc: String,
        totalLiabilityOfBtc: String,
        totalNetAssetOfBtc: String,
        TotalCollateralValueInUSDT: String,
        tradeEnabled: bool,
        transferEnabled: bool,
        accountType: String,
        userAssets: Vec<MarginUserAsset>,
    }
    #[doc(hidden)]
    #[allow(non_upper_case_globals, unused_attributes, unused_qualifications)]
    const _: () = {
        #[allow(unused_extern_crates, clippy::useless_attribute)]
        extern crate serde as _serde;
        #[automatically_derived]
        impl _serde::Serialize for MarginAccountDetails {
            fn serialize<__S>(
                &self,
                __serializer: __S,
            ) -> _serde::__private::Result<__S::Ok, __S::Error>
            where
                __S: _serde::Serializer,
            {
                let mut __serde_state = _serde::Serializer::serialize_struct(
                    __serializer,
                    "MarginAccountDetails",
                    false as usize + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1,
                )?;
                _serde::ser::SerializeStruct::serialize_field(
                    &mut __serde_state,
                    "borrowEnabled",
                    &self.borrowEnabled,
                )?;
                _serde::ser::SerializeStruct::serialize_field(
                    &mut __serde_state,
                    "marginLevel",
                    &self.marginLevel,
                )?;
                _serde::ser::SerializeStruct::serialize_field(
                    &mut __serde_state,
                    "CollateralMarginLevel",
                    &self.CollateralMarginLevel,
                )?;
                _serde::ser::SerializeStruct::serialize_field(
                    &mut __serde_state,
                    "totalAssetOfBtc",
                    &self.totalAssetOfBtc,
                )?;
                _serde::ser::SerializeStruct::serialize_field(
                    &mut __serde_state,
                    "totalLiabilityOfBtc",
                    &self.totalLiabilityOfBtc,
                )?;
                _serde::ser::SerializeStruct::serialize_field(
                    &mut __serde_state,
                    "totalNetAssetOfBtc",
                    &self.totalNetAssetOfBtc,
                )?;
                _serde::ser::SerializeStruct::serialize_field(
                    &mut __serde_state,
                    "TotalCollateralValueInUSDT",
                    &self.TotalCollateralValueInUSDT,
                )?;
                _serde::ser::SerializeStruct::serialize_field(
                    &mut __serde_state,
                    "tradeEnabled",
                    &self.tradeEnabled,
                )?;
                _serde::ser::SerializeStruct::serialize_field(
                    &mut __serde_state,
                    "transferEnabled",
                    &self.transferEnabled,
                )?;
                _serde::ser::SerializeStruct::serialize_field(
                    &mut __serde_state,
                    "accountType",
                    &self.accountType,
                )?;
                _serde::ser::SerializeStruct::serialize_field(
                    &mut __serde_state,
                    "userAssets",
                    &self.userAssets,
                )?;
                _serde::ser::SerializeStruct::end(__serde_state)
            }
        }
    };
    #[doc(hidden)]
    #[allow(non_upper_case_globals, unused_attributes, unused_qualifications)]
    const _: () = {
        #[allow(unused_extern_crates, clippy::useless_attribute)]
        extern crate serde as _serde;
        #[automatically_derived]
        impl<'de> _serde::Deserialize<'de> for MarginAccountDetails {
            fn deserialize<__D>(
                __deserializer: __D,
            ) -> _serde::__private::Result<Self, __D::Error>
            where
                __D: _serde::Deserializer<'de>,
            {
                #[allow(non_camel_case_types)]
                #[doc(hidden)]
                enum __Field {
                    __field0,
                    __field1,
                    __field2,
                    __field3,
                    __field4,
                    __field5,
                    __field6,
                    __field7,
                    __field8,
                    __field9,
                    __field10,
                    __ignore,
                }
                #[doc(hidden)]
                struct __FieldVisitor;
                impl<'de> _serde::de::Visitor<'de> for __FieldVisitor {
                    type Value = __Field;
                    fn expecting(
                        &self,
                        __formatter: &mut _serde::__private::Formatter,
                    ) -> _serde::__private::fmt::Result {
                        _serde::__private::Formatter::write_str(
                            __formatter,
                            "field identifier",
                        )
                    }
                    fn visit_u64<__E>(
                        self,
                        __value: u64,
                    ) -> _serde::__private::Result<Self::Value, __E>
                    where
                        __E: _serde::de::Error,
                    {
                        match __value {
                            0u64 => _serde::__private::Ok(__Field::__field0),
                            1u64 => _serde::__private::Ok(__Field::__field1),
                            2u64 => _serde::__private::Ok(__Field::__field2),
                            3u64 => _serde::__private::Ok(__Field::__field3),
                            4u64 => _serde::__private::Ok(__Field::__field4),
                            5u64 => _serde::__private::Ok(__Field::__field5),
                            6u64 => _serde::__private::Ok(__Field::__field6),
                            7u64 => _serde::__private::Ok(__Field::__field7),
                            8u64 => _serde::__private::Ok(__Field::__field8),
                            9u64 => _serde::__private::Ok(__Field::__field9),
                            10u64 => _serde::__private::Ok(__Field::__field10),
                            _ => _serde::__private::Ok(__Field::__ignore),
                        }
                    }
                    fn visit_str<__E>(
                        self,
                        __value: &str,
                    ) -> _serde::__private::Result<Self::Value, __E>
                    where
                        __E: _serde::de::Error,
                    {
                        match __value {
                            "borrowEnabled" => _serde::__private::Ok(__Field::__field0),
                            "marginLevel" => _serde::__private::Ok(__Field::__field1),
                            "CollateralMarginLevel" => {
                                _serde::__private::Ok(__Field::__field2)
                            }
                            "totalAssetOfBtc" => _serde::__private::Ok(__Field::__field3),
                            "totalLiabilityOfBtc" => {
                                _serde::__private::Ok(__Field::__field4)
                            }
                            "totalNetAssetOfBtc" => {
                                _serde::__private::Ok(__Field::__field5)
                            }
                            "TotalCollateralValueInUSDT" => {
                                _serde::__private::Ok(__Field::__field6)
                            }
                            "tradeEnabled" => _serde::__private::Ok(__Field::__field7),
                            "transferEnabled" => _serde::__private::Ok(__Field::__field8),
                            "accountType" => _serde::__private::Ok(__Field::__field9),
                            "userAssets" => _serde::__private::Ok(__Field::__field10),
                            _ => _serde::__private::Ok(__Field::__ignore),
                        }
                    }
                    fn visit_bytes<__E>(
                        self,
                        __value: &[u8],
                    ) -> _serde::__private::Result<Self::Value, __E>
                    where
                        __E: _serde::de::Error,
                    {
                        match __value {
                            b"borrowEnabled" => _serde::__private::Ok(__Field::__field0),
                            b"marginLevel" => _serde::__private::Ok(__Field::__field1),
                            b"CollateralMarginLevel" => {
                                _serde::__private::Ok(__Field::__field2)
                            }
                            b"totalAssetOfBtc" => {
                                _serde::__private::Ok(__Field::__field3)
                            }
                            b"totalLiabilityOfBtc" => {
                                _serde::__private::Ok(__Field::__field4)
                            }
                            b"totalNetAssetOfBtc" => {
                                _serde::__private::Ok(__Field::__field5)
                            }
                            b"TotalCollateralValueInUSDT" => {
                                _serde::__private::Ok(__Field::__field6)
                            }
                            b"tradeEnabled" => _serde::__private::Ok(__Field::__field7),
                            b"transferEnabled" => {
                                _serde::__private::Ok(__Field::__field8)
                            }
                            b"accountType" => _serde::__private::Ok(__Field::__field9),
                            b"userAssets" => _serde::__private::Ok(__Field::__field10),
                            _ => _serde::__private::Ok(__Field::__ignore),
                        }
                    }
                }
                impl<'de> _serde::Deserialize<'de> for __Field {
                    #[inline]
                    fn deserialize<__D>(
                        __deserializer: __D,
                    ) -> _serde::__private::Result<Self, __D::Error>
                    where
                        __D: _serde::Deserializer<'de>,
                    {
                        _serde::Deserializer::deserialize_identifier(
                            __deserializer,
                            __FieldVisitor,
                        )
                    }
                }
                #[doc(hidden)]
                struct __Visitor<'de> {
                    marker: _serde::__private::PhantomData<MarginAccountDetails>,
                    lifetime: _serde::__private::PhantomData<&'de ()>,
                }
                impl<'de> _serde::de::Visitor<'de> for __Visitor<'de> {
                    type Value = MarginAccountDetails;
                    fn expecting(
                        &self,
                        __formatter: &mut _serde::__private::Formatter,
                    ) -> _serde::__private::fmt::Result {
                        _serde::__private::Formatter::write_str(
                            __formatter,
                            "struct MarginAccountDetails",
                        )
                    }
                    #[inline]
                    fn visit_seq<__A>(
                        self,
                        mut __seq: __A,
                    ) -> _serde::__private::Result<Self::Value, __A::Error>
                    where
                        __A: _serde::de::SeqAccess<'de>,
                    {
                        let __field0 = match _serde::de::SeqAccess::next_element::<
                            bool,
                        >(&mut __seq)? {
                            _serde::__private::Some(__value) => __value,
                            _serde::__private::None => {
                                return _serde::__private::Err(
                                    _serde::de::Error::invalid_length(
                                        0usize,
                                        &"struct MarginAccountDetails with 11 elements",
                                    ),
                                );
                            }
                        };
                        let __field1 = match _serde::de::SeqAccess::next_element::<
                            String,
                        >(&mut __seq)? {
                            _serde::__private::Some(__value) => __value,
                            _serde::__private::None => {
                                return _serde::__private::Err(
                                    _serde::de::Error::invalid_length(
                                        1usize,
                                        &"struct MarginAccountDetails with 11 elements",
                                    ),
                                );
                            }
                        };
                        let __field2 = match _serde::de::SeqAccess::next_element::<
                            String,
                        >(&mut __seq)? {
                            _serde::__private::Some(__value) => __value,
                            _serde::__private::None => {
                                return _serde::__private::Err(
                                    _serde::de::Error::invalid_length(
                                        2usize,
                                        &"struct MarginAccountDetails with 11 elements",
                                    ),
                                );
                            }
                        };
                        let __field3 = match _serde::de::SeqAccess::next_element::<
                            String,
                        >(&mut __seq)? {
                            _serde::__private::Some(__value) => __value,
                            _serde::__private::None => {
                                return _serde::__private::Err(
                                    _serde::de::Error::invalid_length(
                                        3usize,
                                        &"struct MarginAccountDetails with 11 elements",
                                    ),
                                );
                            }
                        };
                        let __field4 = match _serde::de::SeqAccess::next_element::<
                            String,
                        >(&mut __seq)? {
                            _serde::__private::Some(__value) => __value,
                            _serde::__private::None => {
                                return _serde::__private::Err(
                                    _serde::de::Error::invalid_length(
                                        4usize,
                                        &"struct MarginAccountDetails with 11 elements",
                                    ),
                                );
                            }
                        };
                        let __field5 = match _serde::de::SeqAccess::next_element::<
                            String,
                        >(&mut __seq)? {
                            _serde::__private::Some(__value) => __value,
                            _serde::__private::None => {
                                return _serde::__private::Err(
                                    _serde::de::Error::invalid_length(
                                        5usize,
                                        &"struct MarginAccountDetails with 11 elements",
                                    ),
                                );
                            }
                        };
                        let __field6 = match _serde::de::SeqAccess::next_element::<
                            String,
                        >(&mut __seq)? {
                            _serde::__private::Some(__value) => __value,
                            _serde::__private::None => {
                                return _serde::__private::Err(
                                    _serde::de::Error::invalid_length(
                                        6usize,
                                        &"struct MarginAccountDetails with 11 elements",
                                    ),
                                );
                            }
                        };
                        let __field7 = match _serde::de::SeqAccess::next_element::<
                            bool,
                        >(&mut __seq)? {
                            _serde::__private::Some(__value) => __value,
                            _serde::__private::None => {
                                return _serde::__private::Err(
                                    _serde::de::Error::invalid_length(
                                        7usize,
                                        &"struct MarginAccountDetails with 11 elements",
                                    ),
                                );
                            }
                        };
                        let __field8 = match _serde::de::SeqAccess::next_element::<
                            bool,
                        >(&mut __seq)? {
                            _serde::__private::Some(__value) => __value,
                            _serde::__private::None => {
                                return _serde::__private::Err(
                                    _serde::de::Error::invalid_length(
                                        8usize,
                                        &"struct MarginAccountDetails with 11 elements",
                                    ),
                                );
                            }
                        };
                        let __field9 = match _serde::de::SeqAccess::next_element::<
                            String,
                        >(&mut __seq)? {
                            _serde::__private::Some(__value) => __value,
                            _serde::__private::None => {
                                return _serde::__private::Err(
                                    _serde::de::Error::invalid_length(
                                        9usize,
                                        &"struct MarginAccountDetails with 11 elements",
                                    ),
                                );
                            }
                        };
                        let __field10 = match _serde::de::SeqAccess::next_element::<
                            Vec<MarginUserAsset>,
                        >(&mut __seq)? {
                            _serde::__private::Some(__value) => __value,
                            _serde::__private::None => {
                                return _serde::__private::Err(
                                    _serde::de::Error::invalid_length(
                                        10usize,
                                        &"struct MarginAccountDetails with 11 elements",
                                    ),
                                );
                            }
                        };
                        _serde::__private::Ok(MarginAccountDetails {
                            borrowEnabled: __field0,
                            marginLevel: __field1,
                            CollateralMarginLevel: __field2,
                            totalAssetOfBtc: __field3,
                            totalLiabilityOfBtc: __field4,
                            totalNetAssetOfBtc: __field5,
                            TotalCollateralValueInUSDT: __field6,
                            tradeEnabled: __field7,
                            transferEnabled: __field8,
                            accountType: __field9,
                            userAssets: __field10,
                        })
                    }
                    #[inline]
                    fn visit_map<__A>(
                        self,
                        mut __map: __A,
                    ) -> _serde::__private::Result<Self::Value, __A::Error>
                    where
                        __A: _serde::de::MapAccess<'de>,
                    {
                        let mut __field0: _serde::__private::Option<bool> = _serde::__private::None;
                        let mut __field1: _serde::__private::Option<String> = _serde::__private::None;
                        let mut __field2: _serde::__private::Option<String> = _serde::__private::None;
                        let mut __field3: _serde::__private::Option<String> = _serde::__private::None;
                        let mut __field4: _serde::__private::Option<String> = _serde::__private::None;
                        let mut __field5: _serde::__private::Option<String> = _serde::__private::None;
                        let mut __field6: _serde::__private::Option<String> = _serde::__private::None;
                        let mut __field7: _serde::__private::Option<bool> = _serde::__private::None;
                        let mut __field8: _serde::__private::Option<bool> = _serde::__private::None;
                        let mut __field9: _serde::__private::Option<String> = _serde::__private::None;
                        let mut __field10: _serde::__private::Option<
                            Vec<MarginUserAsset>,
                        > = _serde::__private::None;
                        while let _serde::__private::Some(__key) = _serde::de::MapAccess::next_key::<
                            __Field,
                        >(&mut __map)? {
                            match __key {
                                __Field::__field0 => {
                                    if _serde::__private::Option::is_some(&__field0) {
                                        return _serde::__private::Err(
                                            <__A::Error as _serde::de::Error>::duplicate_field(
                                                "borrowEnabled",
                                            ),
                                        );
                                    }
                                    __field0 = _serde::__private::Some(
                                        _serde::de::MapAccess::next_value::<bool>(&mut __map)?,
                                    );
                                }
                                __Field::__field1 => {
                                    if _serde::__private::Option::is_some(&__field1) {
                                        return _serde::__private::Err(
                                            <__A::Error as _serde::de::Error>::duplicate_field(
                                                "marginLevel",
                                            ),
                                        );
                                    }
                                    __field1 = _serde::__private::Some(
                                        _serde::de::MapAccess::next_value::<String>(&mut __map)?,
                                    );
                                }
                                __Field::__field2 => {
                                    if _serde::__private::Option::is_some(&__field2) {
                                        return _serde::__private::Err(
                                            <__A::Error as _serde::de::Error>::duplicate_field(
                                                "CollateralMarginLevel",
                                            ),
                                        );
                                    }
                                    __field2 = _serde::__private::Some(
                                        _serde::de::MapAccess::next_value::<String>(&mut __map)?,
                                    );
                                }
                                __Field::__field3 => {
                                    if _serde::__private::Option::is_some(&__field3) {
                                        return _serde::__private::Err(
                                            <__A::Error as _serde::de::Error>::duplicate_field(
                                                "totalAssetOfBtc",
                                            ),
                                        );
                                    }
                                    __field3 = _serde::__private::Some(
                                        _serde::de::MapAccess::next_value::<String>(&mut __map)?,
                                    );
                                }
                                __Field::__field4 => {
                                    if _serde::__private::Option::is_some(&__field4) {
                                        return _serde::__private::Err(
                                            <__A::Error as _serde::de::Error>::duplicate_field(
                                                "totalLiabilityOfBtc",
                                            ),
                                        );
                                    }
                                    __field4 = _serde::__private::Some(
                                        _serde::de::MapAccess::next_value::<String>(&mut __map)?,
                                    );
                                }
                                __Field::__field5 => {
                                    if _serde::__private::Option::is_some(&__field5) {
                                        return _serde::__private::Err(
                                            <__A::Error as _serde::de::Error>::duplicate_field(
                                                "totalNetAssetOfBtc",
                                            ),
                                        );
                                    }
                                    __field5 = _serde::__private::Some(
                                        _serde::de::MapAccess::next_value::<String>(&mut __map)?,
                                    );
                                }
                                __Field::__field6 => {
                                    if _serde::__private::Option::is_some(&__field6) {
                                        return _serde::__private::Err(
                                            <__A::Error as _serde::de::Error>::duplicate_field(
                                                "TotalCollateralValueInUSDT",
                                            ),
                                        );
                                    }
                                    __field6 = _serde::__private::Some(
                                        _serde::de::MapAccess::next_value::<String>(&mut __map)?,
                                    );
                                }
                                __Field::__field7 => {
                                    if _serde::__private::Option::is_some(&__field7) {
                                        return _serde::__private::Err(
                                            <__A::Error as _serde::de::Error>::duplicate_field(
                                                "tradeEnabled",
                                            ),
                                        );
                                    }
                                    __field7 = _serde::__private::Some(
                                        _serde::de::MapAccess::next_value::<bool>(&mut __map)?,
                                    );
                                }
                                __Field::__field8 => {
                                    if _serde::__private::Option::is_some(&__field8) {
                                        return _serde::__private::Err(
                                            <__A::Error as _serde::de::Error>::duplicate_field(
                                                "transferEnabled",
                                            ),
                                        );
                                    }
                                    __field8 = _serde::__private::Some(
                                        _serde::de::MapAccess::next_value::<bool>(&mut __map)?,
                                    );
                                }
                                __Field::__field9 => {
                                    if _serde::__private::Option::is_some(&__field9) {
                                        return _serde::__private::Err(
                                            <__A::Error as _serde::de::Error>::duplicate_field(
                                                "accountType",
                                            ),
                                        );
                                    }
                                    __field9 = _serde::__private::Some(
                                        _serde::de::MapAccess::next_value::<String>(&mut __map)?,
                                    );
                                }
                                __Field::__field10 => {
                                    if _serde::__private::Option::is_some(&__field10) {
                                        return _serde::__private::Err(
                                            <__A::Error as _serde::de::Error>::duplicate_field(
                                                "userAssets",
                                            ),
                                        );
                                    }
                                    __field10 = _serde::__private::Some(
                                        _serde::de::MapAccess::next_value::<
                                            Vec<MarginUserAsset>,
                                        >(&mut __map)?,
                                    );
                                }
                                _ => {
                                    let _ = _serde::de::MapAccess::next_value::<
                                        _serde::de::IgnoredAny,
                                    >(&mut __map)?;
                                }
                            }
                        }
                        let __field0 = match __field0 {
                            _serde::__private::Some(__field0) => __field0,
                            _serde::__private::None => {
                                _serde::__private::de::missing_field("borrowEnabled")?
                            }
                        };
                        let __field1 = match __field1 {
                            _serde::__private::Some(__field1) => __field1,
                            _serde::__private::None => {
                                _serde::__private::de::missing_field("marginLevel")?
                            }
                        };
                        let __field2 = match __field2 {
                            _serde::__private::Some(__field2) => __field2,
                            _serde::__private::None => {
                                _serde::__private::de::missing_field(
                                    "CollateralMarginLevel",
                                )?
                            }
                        };
                        let __field3 = match __field3 {
                            _serde::__private::Some(__field3) => __field3,
                            _serde::__private::None => {
                                _serde::__private::de::missing_field("totalAssetOfBtc")?
                            }
                        };
                        let __field4 = match __field4 {
                            _serde::__private::Some(__field4) => __field4,
                            _serde::__private::None => {
                                _serde::__private::de::missing_field("totalLiabilityOfBtc")?
                            }
                        };
                        let __field5 = match __field5 {
                            _serde::__private::Some(__field5) => __field5,
                            _serde::__private::None => {
                                _serde::__private::de::missing_field("totalNetAssetOfBtc")?
                            }
                        };
                        let __field6 = match __field6 {
                            _serde::__private::Some(__field6) => __field6,
                            _serde::__private::None => {
                                _serde::__private::de::missing_field(
                                    "TotalCollateralValueInUSDT",
                                )?
                            }
                        };
                        let __field7 = match __field7 {
                            _serde::__private::Some(__field7) => __field7,
                            _serde::__private::None => {
                                _serde::__private::de::missing_field("tradeEnabled")?
                            }
                        };
                        let __field8 = match __field8 {
                            _serde::__private::Some(__field8) => __field8,
                            _serde::__private::None => {
                                _serde::__private::de::missing_field("transferEnabled")?
                            }
                        };
                        let __field9 = match __field9 {
                            _serde::__private::Some(__field9) => __field9,
                            _serde::__private::None => {
                                _serde::__private::de::missing_field("accountType")?
                            }
                        };
                        let __field10 = match __field10 {
                            _serde::__private::Some(__field10) => __field10,
                            _serde::__private::None => {
                                _serde::__private::de::missing_field("userAssets")?
                            }
                        };
                        _serde::__private::Ok(MarginAccountDetails {
                            borrowEnabled: __field0,
                            marginLevel: __field1,
                            CollateralMarginLevel: __field2,
                            totalAssetOfBtc: __field3,
                            totalLiabilityOfBtc: __field4,
                            totalNetAssetOfBtc: __field5,
                            TotalCollateralValueInUSDT: __field6,
                            tradeEnabled: __field7,
                            transferEnabled: __field8,
                            accountType: __field9,
                            userAssets: __field10,
                        })
                    }
                }
                #[doc(hidden)]
                const FIELDS: &'static [&'static str] = &[
                    "borrowEnabled",
                    "marginLevel",
                    "CollateralMarginLevel",
                    "totalAssetOfBtc",
                    "totalLiabilityOfBtc",
                    "totalNetAssetOfBtc",
                    "TotalCollateralValueInUSDT",
                    "tradeEnabled",
                    "transferEnabled",
                    "accountType",
                    "userAssets",
                ];
                _serde::Deserializer::deserialize_struct(
                    __deserializer,
                    "MarginAccountDetails",
                    FIELDS,
                    __Visitor {
                        marker: _serde::__private::PhantomData::<MarginAccountDetails>,
                        lifetime: _serde::__private::PhantomData,
                    },
                )
            }
        }
    };
    #[automatically_derived]
    #[allow(non_snake_case)]
    impl ::core::fmt::Debug for MarginAccountDetails {
        #[inline]
        fn fmt(&self, f: &mut ::core::fmt::Formatter) -> ::core::fmt::Result {
            let names: &'static _ = &[
                "borrowEnabled",
                "marginLevel",
                "CollateralMarginLevel",
                "totalAssetOfBtc",
                "totalLiabilityOfBtc",
                "totalNetAssetOfBtc",
                "TotalCollateralValueInUSDT",
                "tradeEnabled",
                "transferEnabled",
                "accountType",
                "userAssets",
            ];
            let values: &[&dyn ::core::fmt::Debug] = &[
                &self.borrowEnabled,
                &self.marginLevel,
                &self.CollateralMarginLevel,
                &self.totalAssetOfBtc,
                &self.totalLiabilityOfBtc,
                &self.totalNetAssetOfBtc,
                &self.TotalCollateralValueInUSDT,
                &self.tradeEnabled,
                &self.transferEnabled,
                &self.accountType,
                &&self.userAssets,
            ];
            ::core::fmt::Formatter::debug_struct_fields_finish(
                f,
                "MarginAccountDetails",
                names,
                values,
            )
        }
    }
    #[allow(non_snake_case)]
    struct MarginUserAsset {
        asset: String,
        borrowed: String,
        free: String,
        interest: String,
        locked: String,
        netAsset: String,
    }
    #[doc(hidden)]
    #[allow(non_upper_case_globals, unused_attributes, unused_qualifications)]
    const _: () = {
        #[allow(unused_extern_crates, clippy::useless_attribute)]
        extern crate serde as _serde;
        #[automatically_derived]
        impl _serde::Serialize for MarginUserAsset {
            fn serialize<__S>(
                &self,
                __serializer: __S,
            ) -> _serde::__private::Result<__S::Ok, __S::Error>
            where
                __S: _serde::Serializer,
            {
                let mut __serde_state = _serde::Serializer::serialize_struct(
                    __serializer,
                    "MarginUserAsset",
                    false as usize + 1 + 1 + 1 + 1 + 1 + 1,
                )?;
                _serde::ser::SerializeStruct::serialize_field(
                    &mut __serde_state,
                    "asset",
                    &self.asset,
                )?;
                _serde::ser::SerializeStruct::serialize_field(
                    &mut __serde_state,
                    "borrowed",
                    &self.borrowed,
                )?;
                _serde::ser::SerializeStruct::serialize_field(
                    &mut __serde_state,
                    "free",
                    &self.free,
                )?;
                _serde::ser::SerializeStruct::serialize_field(
                    &mut __serde_state,
                    "interest",
                    &self.interest,
                )?;
                _serde::ser::SerializeStruct::serialize_field(
                    &mut __serde_state,
                    "locked",
                    &self.locked,
                )?;
                _serde::ser::SerializeStruct::serialize_field(
                    &mut __serde_state,
                    "netAsset",
                    &self.netAsset,
                )?;
                _serde::ser::SerializeStruct::end(__serde_state)
            }
        }
    };
    #[doc(hidden)]
    #[allow(non_upper_case_globals, unused_attributes, unused_qualifications)]
    const _: () = {
        #[allow(unused_extern_crates, clippy::useless_attribute)]
        extern crate serde as _serde;
        #[automatically_derived]
        impl<'de> _serde::Deserialize<'de> for MarginUserAsset {
            fn deserialize<__D>(
                __deserializer: __D,
            ) -> _serde::__private::Result<Self, __D::Error>
            where
                __D: _serde::Deserializer<'de>,
            {
                #[allow(non_camel_case_types)]
                #[doc(hidden)]
                enum __Field {
                    __field0,
                    __field1,
                    __field2,
                    __field3,
                    __field4,
                    __field5,
                    __ignore,
                }
                #[doc(hidden)]
                struct __FieldVisitor;
                impl<'de> _serde::de::Visitor<'de> for __FieldVisitor {
                    type Value = __Field;
                    fn expecting(
                        &self,
                        __formatter: &mut _serde::__private::Formatter,
                    ) -> _serde::__private::fmt::Result {
                        _serde::__private::Formatter::write_str(
                            __formatter,
                            "field identifier",
                        )
                    }
                    fn visit_u64<__E>(
                        self,
                        __value: u64,
                    ) -> _serde::__private::Result<Self::Value, __E>
                    where
                        __E: _serde::de::Error,
                    {
                        match __value {
                            0u64 => _serde::__private::Ok(__Field::__field0),
                            1u64 => _serde::__private::Ok(__Field::__field1),
                            2u64 => _serde::__private::Ok(__Field::__field2),
                            3u64 => _serde::__private::Ok(__Field::__field3),
                            4u64 => _serde::__private::Ok(__Field::__field4),
                            5u64 => _serde::__private::Ok(__Field::__field5),
                            _ => _serde::__private::Ok(__Field::__ignore),
                        }
                    }
                    fn visit_str<__E>(
                        self,
                        __value: &str,
                    ) -> _serde::__private::Result<Self::Value, __E>
                    where
                        __E: _serde::de::Error,
                    {
                        match __value {
                            "asset" => _serde::__private::Ok(__Field::__field0),
                            "borrowed" => _serde::__private::Ok(__Field::__field1),
                            "free" => _serde::__private::Ok(__Field::__field2),
                            "interest" => _serde::__private::Ok(__Field::__field3),
                            "locked" => _serde::__private::Ok(__Field::__field4),
                            "netAsset" => _serde::__private::Ok(__Field::__field5),
                            _ => _serde::__private::Ok(__Field::__ignore),
                        }
                    }
                    fn visit_bytes<__E>(
                        self,
                        __value: &[u8],
                    ) -> _serde::__private::Result<Self::Value, __E>
                    where
                        __E: _serde::de::Error,
                    {
                        match __value {
                            b"asset" => _serde::__private::Ok(__Field::__field0),
                            b"borrowed" => _serde::__private::Ok(__Field::__field1),
                            b"free" => _serde::__private::Ok(__Field::__field2),
                            b"interest" => _serde::__private::Ok(__Field::__field3),
                            b"locked" => _serde::__private::Ok(__Field::__field4),
                            b"netAsset" => _serde::__private::Ok(__Field::__field5),
                            _ => _serde::__private::Ok(__Field::__ignore),
                        }
                    }
                }
                impl<'de> _serde::Deserialize<'de> for __Field {
                    #[inline]
                    fn deserialize<__D>(
                        __deserializer: __D,
                    ) -> _serde::__private::Result<Self, __D::Error>
                    where
                        __D: _serde::Deserializer<'de>,
                    {
                        _serde::Deserializer::deserialize_identifier(
                            __deserializer,
                            __FieldVisitor,
                        )
                    }
                }
                #[doc(hidden)]
                struct __Visitor<'de> {
                    marker: _serde::__private::PhantomData<MarginUserAsset>,
                    lifetime: _serde::__private::PhantomData<&'de ()>,
                }
                impl<'de> _serde::de::Visitor<'de> for __Visitor<'de> {
                    type Value = MarginUserAsset;
                    fn expecting(
                        &self,
                        __formatter: &mut _serde::__private::Formatter,
                    ) -> _serde::__private::fmt::Result {
                        _serde::__private::Formatter::write_str(
                            __formatter,
                            "struct MarginUserAsset",
                        )
                    }
                    #[inline]
                    fn visit_seq<__A>(
                        self,
                        mut __seq: __A,
                    ) -> _serde::__private::Result<Self::Value, __A::Error>
                    where
                        __A: _serde::de::SeqAccess<'de>,
                    {
                        let __field0 = match _serde::de::SeqAccess::next_element::<
                            String,
                        >(&mut __seq)? {
                            _serde::__private::Some(__value) => __value,
                            _serde::__private::None => {
                                return _serde::__private::Err(
                                    _serde::de::Error::invalid_length(
                                        0usize,
                                        &"struct MarginUserAsset with 6 elements",
                                    ),
                                );
                            }
                        };
                        let __field1 = match _serde::de::SeqAccess::next_element::<
                            String,
                        >(&mut __seq)? {
                            _serde::__private::Some(__value) => __value,
                            _serde::__private::None => {
                                return _serde::__private::Err(
                                    _serde::de::Error::invalid_length(
                                        1usize,
                                        &"struct MarginUserAsset with 6 elements",
                                    ),
                                );
                            }
                        };
                        let __field2 = match _serde::de::SeqAccess::next_element::<
                            String,
                        >(&mut __seq)? {
                            _serde::__private::Some(__value) => __value,
                            _serde::__private::None => {
                                return _serde::__private::Err(
                                    _serde::de::Error::invalid_length(
                                        2usize,
                                        &"struct MarginUserAsset with 6 elements",
                                    ),
                                );
                            }
                        };
                        let __field3 = match _serde::de::SeqAccess::next_element::<
                            String,
                        >(&mut __seq)? {
                            _serde::__private::Some(__value) => __value,
                            _serde::__private::None => {
                                return _serde::__private::Err(
                                    _serde::de::Error::invalid_length(
                                        3usize,
                                        &"struct MarginUserAsset with 6 elements",
                                    ),
                                );
                            }
                        };
                        let __field4 = match _serde::de::SeqAccess::next_element::<
                            String,
                        >(&mut __seq)? {
                            _serde::__private::Some(__value) => __value,
                            _serde::__private::None => {
                                return _serde::__private::Err(
                                    _serde::de::Error::invalid_length(
                                        4usize,
                                        &"struct MarginUserAsset with 6 elements",
                                    ),
                                );
                            }
                        };
                        let __field5 = match _serde::de::SeqAccess::next_element::<
                            String,
                        >(&mut __seq)? {
                            _serde::__private::Some(__value) => __value,
                            _serde::__private::None => {
                                return _serde::__private::Err(
                                    _serde::de::Error::invalid_length(
                                        5usize,
                                        &"struct MarginUserAsset with 6 elements",
                                    ),
                                );
                            }
                        };
                        _serde::__private::Ok(MarginUserAsset {
                            asset: __field0,
                            borrowed: __field1,
                            free: __field2,
                            interest: __field3,
                            locked: __field4,
                            netAsset: __field5,
                        })
                    }
                    #[inline]
                    fn visit_map<__A>(
                        self,
                        mut __map: __A,
                    ) -> _serde::__private::Result<Self::Value, __A::Error>
                    where
                        __A: _serde::de::MapAccess<'de>,
                    {
                        let mut __field0: _serde::__private::Option<String> = _serde::__private::None;
                        let mut __field1: _serde::__private::Option<String> = _serde::__private::None;
                        let mut __field2: _serde::__private::Option<String> = _serde::__private::None;
                        let mut __field3: _serde::__private::Option<String> = _serde::__private::None;
                        let mut __field4: _serde::__private::Option<String> = _serde::__private::None;
                        let mut __field5: _serde::__private::Option<String> = _serde::__private::None;
                        while let _serde::__private::Some(__key) = _serde::de::MapAccess::next_key::<
                            __Field,
                        >(&mut __map)? {
                            match __key {
                                __Field::__field0 => {
                                    if _serde::__private::Option::is_some(&__field0) {
                                        return _serde::__private::Err(
                                            <__A::Error as _serde::de::Error>::duplicate_field("asset"),
                                        );
                                    }
                                    __field0 = _serde::__private::Some(
                                        _serde::de::MapAccess::next_value::<String>(&mut __map)?,
                                    );
                                }
                                __Field::__field1 => {
                                    if _serde::__private::Option::is_some(&__field1) {
                                        return _serde::__private::Err(
                                            <__A::Error as _serde::de::Error>::duplicate_field(
                                                "borrowed",
                                            ),
                                        );
                                    }
                                    __field1 = _serde::__private::Some(
                                        _serde::de::MapAccess::next_value::<String>(&mut __map)?,
                                    );
                                }
                                __Field::__field2 => {
                                    if _serde::__private::Option::is_some(&__field2) {
                                        return _serde::__private::Err(
                                            <__A::Error as _serde::de::Error>::duplicate_field("free"),
                                        );
                                    }
                                    __field2 = _serde::__private::Some(
                                        _serde::de::MapAccess::next_value::<String>(&mut __map)?,
                                    );
                                }
                                __Field::__field3 => {
                                    if _serde::__private::Option::is_some(&__field3) {
                                        return _serde::__private::Err(
                                            <__A::Error as _serde::de::Error>::duplicate_field(
                                                "interest",
                                            ),
                                        );
                                    }
                                    __field3 = _serde::__private::Some(
                                        _serde::de::MapAccess::next_value::<String>(&mut __map)?,
                                    );
                                }
                                __Field::__field4 => {
                                    if _serde::__private::Option::is_some(&__field4) {
                                        return _serde::__private::Err(
                                            <__A::Error as _serde::de::Error>::duplicate_field("locked"),
                                        );
                                    }
                                    __field4 = _serde::__private::Some(
                                        _serde::de::MapAccess::next_value::<String>(&mut __map)?,
                                    );
                                }
                                __Field::__field5 => {
                                    if _serde::__private::Option::is_some(&__field5) {
                                        return _serde::__private::Err(
                                            <__A::Error as _serde::de::Error>::duplicate_field(
                                                "netAsset",
                                            ),
                                        );
                                    }
                                    __field5 = _serde::__private::Some(
                                        _serde::de::MapAccess::next_value::<String>(&mut __map)?,
                                    );
                                }
                                _ => {
                                    let _ = _serde::de::MapAccess::next_value::<
                                        _serde::de::IgnoredAny,
                                    >(&mut __map)?;
                                }
                            }
                        }
                        let __field0 = match __field0 {
                            _serde::__private::Some(__field0) => __field0,
                            _serde::__private::None => {
                                _serde::__private::de::missing_field("asset")?
                            }
                        };
                        let __field1 = match __field1 {
                            _serde::__private::Some(__field1) => __field1,
                            _serde::__private::None => {
                                _serde::__private::de::missing_field("borrowed")?
                            }
                        };
                        let __field2 = match __field2 {
                            _serde::__private::Some(__field2) => __field2,
                            _serde::__private::None => {
                                _serde::__private::de::missing_field("free")?
                            }
                        };
                        let __field3 = match __field3 {
                            _serde::__private::Some(__field3) => __field3,
                            _serde::__private::None => {
                                _serde::__private::de::missing_field("interest")?
                            }
                        };
                        let __field4 = match __field4 {
                            _serde::__private::Some(__field4) => __field4,
                            _serde::__private::None => {
                                _serde::__private::de::missing_field("locked")?
                            }
                        };
                        let __field5 = match __field5 {
                            _serde::__private::Some(__field5) => __field5,
                            _serde::__private::None => {
                                _serde::__private::de::missing_field("netAsset")?
                            }
                        };
                        _serde::__private::Ok(MarginUserAsset {
                            asset: __field0,
                            borrowed: __field1,
                            free: __field2,
                            interest: __field3,
                            locked: __field4,
                            netAsset: __field5,
                        })
                    }
                }
                #[doc(hidden)]
                const FIELDS: &'static [&'static str] = &[
                    "asset",
                    "borrowed",
                    "free",
                    "interest",
                    "locked",
                    "netAsset",
                ];
                _serde::Deserializer::deserialize_struct(
                    __deserializer,
                    "MarginUserAsset",
                    FIELDS,
                    __Visitor {
                        marker: _serde::__private::PhantomData::<MarginUserAsset>,
                        lifetime: _serde::__private::PhantomData,
                    },
                )
            }
        }
    };
    #[automatically_derived]
    #[allow(non_snake_case)]
    impl ::core::fmt::Debug for MarginUserAsset {
        #[inline]
        fn fmt(&self, f: &mut ::core::fmt::Formatter) -> ::core::fmt::Result {
            let names: &'static _ = &[
                "asset",
                "borrowed",
                "free",
                "interest",
                "locked",
                "netAsset",
            ];
            let values: &[&dyn ::core::fmt::Debug] = &[
                &self.asset,
                &self.borrowed,
                &self.free,
                &self.interest,
                &self.locked,
                &&self.netAsset,
            ];
            ::core::fmt::Formatter::debug_struct_fields_finish(
                f,
                "MarginUserAsset",
                names,
                values,
            )
        }
    }
    #[allow(non_snake_case)]
    struct FuturesExchangeInfo {
        exchangeFilters: Vec<String>,
        rateLimits: Vec<RateLimit>,
        serverTime: i64,
        assets: Vec<Value>,
        symbols: Vec<FuturesSymbol>,
        timezone: String,
    }
    #[automatically_derived]
    #[allow(non_snake_case)]
    impl ::core::fmt::Debug for FuturesExchangeInfo {
        #[inline]
        fn fmt(&self, f: &mut ::core::fmt::Formatter) -> ::core::fmt::Result {
            let names: &'static _ = &[
                "exchangeFilters",
                "rateLimits",
                "serverTime",
                "assets",
                "symbols",
                "timezone",
            ];
            let values: &[&dyn ::core::fmt::Debug] = &[
                &self.exchangeFilters,
                &self.rateLimits,
                &self.serverTime,
                &self.assets,
                &self.symbols,
                &&self.timezone,
            ];
            ::core::fmt::Formatter::debug_struct_fields_finish(
                f,
                "FuturesExchangeInfo",
                names,
                values,
            )
        }
    }
    #[doc(hidden)]
    #[allow(non_upper_case_globals, unused_attributes, unused_qualifications)]
    const _: () = {
        #[allow(unused_extern_crates, clippy::useless_attribute)]
        extern crate serde as _serde;
        #[automatically_derived]
        impl<'de> _serde::Deserialize<'de> for FuturesExchangeInfo {
            fn deserialize<__D>(
                __deserializer: __D,
            ) -> _serde::__private::Result<Self, __D::Error>
            where
                __D: _serde::Deserializer<'de>,
            {
                #[allow(non_camel_case_types)]
                #[doc(hidden)]
                enum __Field {
                    __field0,
                    __field1,
                    __field2,
                    __field3,
                    __field4,
                    __field5,
                    __ignore,
                }
                #[doc(hidden)]
                struct __FieldVisitor;
                impl<'de> _serde::de::Visitor<'de> for __FieldVisitor {
                    type Value = __Field;
                    fn expecting(
                        &self,
                        __formatter: &mut _serde::__private::Formatter,
                    ) -> _serde::__private::fmt::Result {
                        _serde::__private::Formatter::write_str(
                            __formatter,
                            "field identifier",
                        )
                    }
                    fn visit_u64<__E>(
                        self,
                        __value: u64,
                    ) -> _serde::__private::Result<Self::Value, __E>
                    where
                        __E: _serde::de::Error,
                    {
                        match __value {
                            0u64 => _serde::__private::Ok(__Field::__field0),
                            1u64 => _serde::__private::Ok(__Field::__field1),
                            2u64 => _serde::__private::Ok(__Field::__field2),
                            3u64 => _serde::__private::Ok(__Field::__field3),
                            4u64 => _serde::__private::Ok(__Field::__field4),
                            5u64 => _serde::__private::Ok(__Field::__field5),
                            _ => _serde::__private::Ok(__Field::__ignore),
                        }
                    }
                    fn visit_str<__E>(
                        self,
                        __value: &str,
                    ) -> _serde::__private::Result<Self::Value, __E>
                    where
                        __E: _serde::de::Error,
                    {
                        match __value {
                            "exchangeFilters" => _serde::__private::Ok(__Field::__field0),
                            "rateLimits" => _serde::__private::Ok(__Field::__field1),
                            "serverTime" => _serde::__private::Ok(__Field::__field2),
                            "assets" => _serde::__private::Ok(__Field::__field3),
                            "symbols" => _serde::__private::Ok(__Field::__field4),
                            "timezone" => _serde::__private::Ok(__Field::__field5),
                            _ => _serde::__private::Ok(__Field::__ignore),
                        }
                    }
                    fn visit_bytes<__E>(
                        self,
                        __value: &[u8],
                    ) -> _serde::__private::Result<Self::Value, __E>
                    where
                        __E: _serde::de::Error,
                    {
                        match __value {
                            b"exchangeFilters" => {
                                _serde::__private::Ok(__Field::__field0)
                            }
                            b"rateLimits" => _serde::__private::Ok(__Field::__field1),
                            b"serverTime" => _serde::__private::Ok(__Field::__field2),
                            b"assets" => _serde::__private::Ok(__Field::__field3),
                            b"symbols" => _serde::__private::Ok(__Field::__field4),
                            b"timezone" => _serde::__private::Ok(__Field::__field5),
                            _ => _serde::__private::Ok(__Field::__ignore),
                        }
                    }
                }
                impl<'de> _serde::Deserialize<'de> for __Field {
                    #[inline]
                    fn deserialize<__D>(
                        __deserializer: __D,
                    ) -> _serde::__private::Result<Self, __D::Error>
                    where
                        __D: _serde::Deserializer<'de>,
                    {
                        _serde::Deserializer::deserialize_identifier(
                            __deserializer,
                            __FieldVisitor,
                        )
                    }
                }
                #[doc(hidden)]
                struct __Visitor<'de> {
                    marker: _serde::__private::PhantomData<FuturesExchangeInfo>,
                    lifetime: _serde::__private::PhantomData<&'de ()>,
                }
                impl<'de> _serde::de::Visitor<'de> for __Visitor<'de> {
                    type Value = FuturesExchangeInfo;
                    fn expecting(
                        &self,
                        __formatter: &mut _serde::__private::Formatter,
                    ) -> _serde::__private::fmt::Result {
                        _serde::__private::Formatter::write_str(
                            __formatter,
                            "struct FuturesExchangeInfo",
                        )
                    }
                    #[inline]
                    fn visit_seq<__A>(
                        self,
                        mut __seq: __A,
                    ) -> _serde::__private::Result<Self::Value, __A::Error>
                    where
                        __A: _serde::de::SeqAccess<'de>,
                    {
                        let __field0 = match _serde::de::SeqAccess::next_element::<
                            Vec<String>,
                        >(&mut __seq)? {
                            _serde::__private::Some(__value) => __value,
                            _serde::__private::None => {
                                return _serde::__private::Err(
                                    _serde::de::Error::invalid_length(
                                        0usize,
                                        &"struct FuturesExchangeInfo with 6 elements",
                                    ),
                                );
                            }
                        };
                        let __field1 = match _serde::de::SeqAccess::next_element::<
                            Vec<RateLimit>,
                        >(&mut __seq)? {
                            _serde::__private::Some(__value) => __value,
                            _serde::__private::None => {
                                return _serde::__private::Err(
                                    _serde::de::Error::invalid_length(
                                        1usize,
                                        &"struct FuturesExchangeInfo with 6 elements",
                                    ),
                                );
                            }
                        };
                        let __field2 = match _serde::de::SeqAccess::next_element::<
                            i64,
                        >(&mut __seq)? {
                            _serde::__private::Some(__value) => __value,
                            _serde::__private::None => {
                                return _serde::__private::Err(
                                    _serde::de::Error::invalid_length(
                                        2usize,
                                        &"struct FuturesExchangeInfo with 6 elements",
                                    ),
                                );
                            }
                        };
                        let __field3 = match _serde::de::SeqAccess::next_element::<
                            Vec<Value>,
                        >(&mut __seq)? {
                            _serde::__private::Some(__value) => __value,
                            _serde::__private::None => {
                                return _serde::__private::Err(
                                    _serde::de::Error::invalid_length(
                                        3usize,
                                        &"struct FuturesExchangeInfo with 6 elements",
                                    ),
                                );
                            }
                        };
                        let __field4 = match _serde::de::SeqAccess::next_element::<
                            Vec<FuturesSymbol>,
                        >(&mut __seq)? {
                            _serde::__private::Some(__value) => __value,
                            _serde::__private::None => {
                                return _serde::__private::Err(
                                    _serde::de::Error::invalid_length(
                                        4usize,
                                        &"struct FuturesExchangeInfo with 6 elements",
                                    ),
                                );
                            }
                        };
                        let __field5 = match _serde::de::SeqAccess::next_element::<
                            String,
                        >(&mut __seq)? {
                            _serde::__private::Some(__value) => __value,
                            _serde::__private::None => {
                                return _serde::__private::Err(
                                    _serde::de::Error::invalid_length(
                                        5usize,
                                        &"struct FuturesExchangeInfo with 6 elements",
                                    ),
                                );
                            }
                        };
                        _serde::__private::Ok(FuturesExchangeInfo {
                            exchangeFilters: __field0,
                            rateLimits: __field1,
                            serverTime: __field2,
                            assets: __field3,
                            symbols: __field4,
                            timezone: __field5,
                        })
                    }
                    #[inline]
                    fn visit_map<__A>(
                        self,
                        mut __map: __A,
                    ) -> _serde::__private::Result<Self::Value, __A::Error>
                    where
                        __A: _serde::de::MapAccess<'de>,
                    {
                        let mut __field0: _serde::__private::Option<Vec<String>> = _serde::__private::None;
                        let mut __field1: _serde::__private::Option<Vec<RateLimit>> = _serde::__private::None;
                        let mut __field2: _serde::__private::Option<i64> = _serde::__private::None;
                        let mut __field3: _serde::__private::Option<Vec<Value>> = _serde::__private::None;
                        let mut __field4: _serde::__private::Option<
                            Vec<FuturesSymbol>,
                        > = _serde::__private::None;
                        let mut __field5: _serde::__private::Option<String> = _serde::__private::None;
                        while let _serde::__private::Some(__key) = _serde::de::MapAccess::next_key::<
                            __Field,
                        >(&mut __map)? {
                            match __key {
                                __Field::__field0 => {
                                    if _serde::__private::Option::is_some(&__field0) {
                                        return _serde::__private::Err(
                                            <__A::Error as _serde::de::Error>::duplicate_field(
                                                "exchangeFilters",
                                            ),
                                        );
                                    }
                                    __field0 = _serde::__private::Some(
                                        _serde::de::MapAccess::next_value::<
                                            Vec<String>,
                                        >(&mut __map)?,
                                    );
                                }
                                __Field::__field1 => {
                                    if _serde::__private::Option::is_some(&__field1) {
                                        return _serde::__private::Err(
                                            <__A::Error as _serde::de::Error>::duplicate_field(
                                                "rateLimits",
                                            ),
                                        );
                                    }
                                    __field1 = _serde::__private::Some(
                                        _serde::de::MapAccess::next_value::<
                                            Vec<RateLimit>,
                                        >(&mut __map)?,
                                    );
                                }
                                __Field::__field2 => {
                                    if _serde::__private::Option::is_some(&__field2) {
                                        return _serde::__private::Err(
                                            <__A::Error as _serde::de::Error>::duplicate_field(
                                                "serverTime",
                                            ),
                                        );
                                    }
                                    __field2 = _serde::__private::Some(
                                        _serde::de::MapAccess::next_value::<i64>(&mut __map)?,
                                    );
                                }
                                __Field::__field3 => {
                                    if _serde::__private::Option::is_some(&__field3) {
                                        return _serde::__private::Err(
                                            <__A::Error as _serde::de::Error>::duplicate_field("assets"),
                                        );
                                    }
                                    __field3 = _serde::__private::Some(
                                        _serde::de::MapAccess::next_value::<Vec<Value>>(&mut __map)?,
                                    );
                                }
                                __Field::__field4 => {
                                    if _serde::__private::Option::is_some(&__field4) {
                                        return _serde::__private::Err(
                                            <__A::Error as _serde::de::Error>::duplicate_field(
                                                "symbols",
                                            ),
                                        );
                                    }
                                    __field4 = _serde::__private::Some(
                                        _serde::de::MapAccess::next_value::<
                                            Vec<FuturesSymbol>,
                                        >(&mut __map)?,
                                    );
                                }
                                __Field::__field5 => {
                                    if _serde::__private::Option::is_some(&__field5) {
                                        return _serde::__private::Err(
                                            <__A::Error as _serde::de::Error>::duplicate_field(
                                                "timezone",
                                            ),
                                        );
                                    }
                                    __field5 = _serde::__private::Some(
                                        _serde::de::MapAccess::next_value::<String>(&mut __map)?,
                                    );
                                }
                                _ => {
                                    let _ = _serde::de::MapAccess::next_value::<
                                        _serde::de::IgnoredAny,
                                    >(&mut __map)?;
                                }
                            }
                        }
                        let __field0 = match __field0 {
                            _serde::__private::Some(__field0) => __field0,
                            _serde::__private::None => {
                                _serde::__private::de::missing_field("exchangeFilters")?
                            }
                        };
                        let __field1 = match __field1 {
                            _serde::__private::Some(__field1) => __field1,
                            _serde::__private::None => {
                                _serde::__private::de::missing_field("rateLimits")?
                            }
                        };
                        let __field2 = match __field2 {
                            _serde::__private::Some(__field2) => __field2,
                            _serde::__private::None => {
                                _serde::__private::de::missing_field("serverTime")?
                            }
                        };
                        let __field3 = match __field3 {
                            _serde::__private::Some(__field3) => __field3,
                            _serde::__private::None => {
                                _serde::__private::de::missing_field("assets")?
                            }
                        };
                        let __field4 = match __field4 {
                            _serde::__private::Some(__field4) => __field4,
                            _serde::__private::None => {
                                _serde::__private::de::missing_field("symbols")?
                            }
                        };
                        let __field5 = match __field5 {
                            _serde::__private::Some(__field5) => __field5,
                            _serde::__private::None => {
                                _serde::__private::de::missing_field("timezone")?
                            }
                        };
                        _serde::__private::Ok(FuturesExchangeInfo {
                            exchangeFilters: __field0,
                            rateLimits: __field1,
                            serverTime: __field2,
                            assets: __field3,
                            symbols: __field4,
                            timezone: __field5,
                        })
                    }
                }
                #[doc(hidden)]
                const FIELDS: &'static [&'static str] = &[
                    "exchangeFilters",
                    "rateLimits",
                    "serverTime",
                    "assets",
                    "symbols",
                    "timezone",
                ];
                _serde::Deserializer::deserialize_struct(
                    __deserializer,
                    "FuturesExchangeInfo",
                    FIELDS,
                    __Visitor {
                        marker: _serde::__private::PhantomData::<FuturesExchangeInfo>,
                        lifetime: _serde::__private::PhantomData,
                    },
                )
            }
        }
    };
    #[doc(hidden)]
    #[allow(non_upper_case_globals, unused_attributes, unused_qualifications)]
    const _: () = {
        #[allow(unused_extern_crates, clippy::useless_attribute)]
        extern crate serde as _serde;
        #[automatically_derived]
        impl _serde::Serialize for FuturesExchangeInfo {
            fn serialize<__S>(
                &self,
                __serializer: __S,
            ) -> _serde::__private::Result<__S::Ok, __S::Error>
            where
                __S: _serde::Serializer,
            {
                let mut __serde_state = _serde::Serializer::serialize_struct(
                    __serializer,
                    "FuturesExchangeInfo",
                    false as usize + 1 + 1 + 1 + 1 + 1 + 1,
                )?;
                _serde::ser::SerializeStruct::serialize_field(
                    &mut __serde_state,
                    "exchangeFilters",
                    &self.exchangeFilters,
                )?;
                _serde::ser::SerializeStruct::serialize_field(
                    &mut __serde_state,
                    "rateLimits",
                    &self.rateLimits,
                )?;
                _serde::ser::SerializeStruct::serialize_field(
                    &mut __serde_state,
                    "serverTime",
                    &self.serverTime,
                )?;
                _serde::ser::SerializeStruct::serialize_field(
                    &mut __serde_state,
                    "assets",
                    &self.assets,
                )?;
                _serde::ser::SerializeStruct::serialize_field(
                    &mut __serde_state,
                    "symbols",
                    &self.symbols,
                )?;
                _serde::ser::SerializeStruct::serialize_field(
                    &mut __serde_state,
                    "timezone",
                    &self.timezone,
                )?;
                _serde::ser::SerializeStruct::end(__serde_state)
            }
        }
    };
    #[allow(non_snake_case)]
    struct RateLimit {
        interval: String,
        intervalNum: u32,
        limit: u32,
        rateLimitType: String,
    }
    #[automatically_derived]
    #[allow(non_snake_case)]
    impl ::core::fmt::Debug for RateLimit {
        #[inline]
        fn fmt(&self, f: &mut ::core::fmt::Formatter) -> ::core::fmt::Result {
            ::core::fmt::Formatter::debug_struct_field4_finish(
                f,
                "RateLimit",
                "interval",
                &self.interval,
                "intervalNum",
                &self.intervalNum,
                "limit",
                &self.limit,
                "rateLimitType",
                &&self.rateLimitType,
            )
        }
    }
    #[doc(hidden)]
    #[allow(non_upper_case_globals, unused_attributes, unused_qualifications)]
    const _: () = {
        #[allow(unused_extern_crates, clippy::useless_attribute)]
        extern crate serde as _serde;
        #[automatically_derived]
        impl<'de> _serde::Deserialize<'de> for RateLimit {
            fn deserialize<__D>(
                __deserializer: __D,
            ) -> _serde::__private::Result<Self, __D::Error>
            where
                __D: _serde::Deserializer<'de>,
            {
                #[allow(non_camel_case_types)]
                #[doc(hidden)]
                enum __Field {
                    __field0,
                    __field1,
                    __field2,
                    __field3,
                    __ignore,
                }
                #[doc(hidden)]
                struct __FieldVisitor;
                impl<'de> _serde::de::Visitor<'de> for __FieldVisitor {
                    type Value = __Field;
                    fn expecting(
                        &self,
                        __formatter: &mut _serde::__private::Formatter,
                    ) -> _serde::__private::fmt::Result {
                        _serde::__private::Formatter::write_str(
                            __formatter,
                            "field identifier",
                        )
                    }
                    fn visit_u64<__E>(
                        self,
                        __value: u64,
                    ) -> _serde::__private::Result<Self::Value, __E>
                    where
                        __E: _serde::de::Error,
                    {
                        match __value {
                            0u64 => _serde::__private::Ok(__Field::__field0),
                            1u64 => _serde::__private::Ok(__Field::__field1),
                            2u64 => _serde::__private::Ok(__Field::__field2),
                            3u64 => _serde::__private::Ok(__Field::__field3),
                            _ => _serde::__private::Ok(__Field::__ignore),
                        }
                    }
                    fn visit_str<__E>(
                        self,
                        __value: &str,
                    ) -> _serde::__private::Result<Self::Value, __E>
                    where
                        __E: _serde::de::Error,
                    {
                        match __value {
                            "interval" => _serde::__private::Ok(__Field::__field0),
                            "intervalNum" => _serde::__private::Ok(__Field::__field1),
                            "limit" => _serde::__private::Ok(__Field::__field2),
                            "rateLimitType" => _serde::__private::Ok(__Field::__field3),
                            _ => _serde::__private::Ok(__Field::__ignore),
                        }
                    }
                    fn visit_bytes<__E>(
                        self,
                        __value: &[u8],
                    ) -> _serde::__private::Result<Self::Value, __E>
                    where
                        __E: _serde::de::Error,
                    {
                        match __value {
                            b"interval" => _serde::__private::Ok(__Field::__field0),
                            b"intervalNum" => _serde::__private::Ok(__Field::__field1),
                            b"limit" => _serde::__private::Ok(__Field::__field2),
                            b"rateLimitType" => _serde::__private::Ok(__Field::__field3),
                            _ => _serde::__private::Ok(__Field::__ignore),
                        }
                    }
                }
                impl<'de> _serde::Deserialize<'de> for __Field {
                    #[inline]
                    fn deserialize<__D>(
                        __deserializer: __D,
                    ) -> _serde::__private::Result<Self, __D::Error>
                    where
                        __D: _serde::Deserializer<'de>,
                    {
                        _serde::Deserializer::deserialize_identifier(
                            __deserializer,
                            __FieldVisitor,
                        )
                    }
                }
                #[doc(hidden)]
                struct __Visitor<'de> {
                    marker: _serde::__private::PhantomData<RateLimit>,
                    lifetime: _serde::__private::PhantomData<&'de ()>,
                }
                impl<'de> _serde::de::Visitor<'de> for __Visitor<'de> {
                    type Value = RateLimit;
                    fn expecting(
                        &self,
                        __formatter: &mut _serde::__private::Formatter,
                    ) -> _serde::__private::fmt::Result {
                        _serde::__private::Formatter::write_str(
                            __formatter,
                            "struct RateLimit",
                        )
                    }
                    #[inline]
                    fn visit_seq<__A>(
                        self,
                        mut __seq: __A,
                    ) -> _serde::__private::Result<Self::Value, __A::Error>
                    where
                        __A: _serde::de::SeqAccess<'de>,
                    {
                        let __field0 = match _serde::de::SeqAccess::next_element::<
                            String,
                        >(&mut __seq)? {
                            _serde::__private::Some(__value) => __value,
                            _serde::__private::None => {
                                return _serde::__private::Err(
                                    _serde::de::Error::invalid_length(
                                        0usize,
                                        &"struct RateLimit with 4 elements",
                                    ),
                                );
                            }
                        };
                        let __field1 = match _serde::de::SeqAccess::next_element::<
                            u32,
                        >(&mut __seq)? {
                            _serde::__private::Some(__value) => __value,
                            _serde::__private::None => {
                                return _serde::__private::Err(
                                    _serde::de::Error::invalid_length(
                                        1usize,
                                        &"struct RateLimit with 4 elements",
                                    ),
                                );
                            }
                        };
                        let __field2 = match _serde::de::SeqAccess::next_element::<
                            u32,
                        >(&mut __seq)? {
                            _serde::__private::Some(__value) => __value,
                            _serde::__private::None => {
                                return _serde::__private::Err(
                                    _serde::de::Error::invalid_length(
                                        2usize,
                                        &"struct RateLimit with 4 elements",
                                    ),
                                );
                            }
                        };
                        let __field3 = match _serde::de::SeqAccess::next_element::<
                            String,
                        >(&mut __seq)? {
                            _serde::__private::Some(__value) => __value,
                            _serde::__private::None => {
                                return _serde::__private::Err(
                                    _serde::de::Error::invalid_length(
                                        3usize,
                                        &"struct RateLimit with 4 elements",
                                    ),
                                );
                            }
                        };
                        _serde::__private::Ok(RateLimit {
                            interval: __field0,
                            intervalNum: __field1,
                            limit: __field2,
                            rateLimitType: __field3,
                        })
                    }
                    #[inline]
                    fn visit_map<__A>(
                        self,
                        mut __map: __A,
                    ) -> _serde::__private::Result<Self::Value, __A::Error>
                    where
                        __A: _serde::de::MapAccess<'de>,
                    {
                        let mut __field0: _serde::__private::Option<String> = _serde::__private::None;
                        let mut __field1: _serde::__private::Option<u32> = _serde::__private::None;
                        let mut __field2: _serde::__private::Option<u32> = _serde::__private::None;
                        let mut __field3: _serde::__private::Option<String> = _serde::__private::None;
                        while let _serde::__private::Some(__key) = _serde::de::MapAccess::next_key::<
                            __Field,
                        >(&mut __map)? {
                            match __key {
                                __Field::__field0 => {
                                    if _serde::__private::Option::is_some(&__field0) {
                                        return _serde::__private::Err(
                                            <__A::Error as _serde::de::Error>::duplicate_field(
                                                "interval",
                                            ),
                                        );
                                    }
                                    __field0 = _serde::__private::Some(
                                        _serde::de::MapAccess::next_value::<String>(&mut __map)?,
                                    );
                                }
                                __Field::__field1 => {
                                    if _serde::__private::Option::is_some(&__field1) {
                                        return _serde::__private::Err(
                                            <__A::Error as _serde::de::Error>::duplicate_field(
                                                "intervalNum",
                                            ),
                                        );
                                    }
                                    __field1 = _serde::__private::Some(
                                        _serde::de::MapAccess::next_value::<u32>(&mut __map)?,
                                    );
                                }
                                __Field::__field2 => {
                                    if _serde::__private::Option::is_some(&__field2) {
                                        return _serde::__private::Err(
                                            <__A::Error as _serde::de::Error>::duplicate_field("limit"),
                                        );
                                    }
                                    __field2 = _serde::__private::Some(
                                        _serde::de::MapAccess::next_value::<u32>(&mut __map)?,
                                    );
                                }
                                __Field::__field3 => {
                                    if _serde::__private::Option::is_some(&__field3) {
                                        return _serde::__private::Err(
                                            <__A::Error as _serde::de::Error>::duplicate_field(
                                                "rateLimitType",
                                            ),
                                        );
                                    }
                                    __field3 = _serde::__private::Some(
                                        _serde::de::MapAccess::next_value::<String>(&mut __map)?,
                                    );
                                }
                                _ => {
                                    let _ = _serde::de::MapAccess::next_value::<
                                        _serde::de::IgnoredAny,
                                    >(&mut __map)?;
                                }
                            }
                        }
                        let __field0 = match __field0 {
                            _serde::__private::Some(__field0) => __field0,
                            _serde::__private::None => {
                                _serde::__private::de::missing_field("interval")?
                            }
                        };
                        let __field1 = match __field1 {
                            _serde::__private::Some(__field1) => __field1,
                            _serde::__private::None => {
                                _serde::__private::de::missing_field("intervalNum")?
                            }
                        };
                        let __field2 = match __field2 {
                            _serde::__private::Some(__field2) => __field2,
                            _serde::__private::None => {
                                _serde::__private::de::missing_field("limit")?
                            }
                        };
                        let __field3 = match __field3 {
                            _serde::__private::Some(__field3) => __field3,
                            _serde::__private::None => {
                                _serde::__private::de::missing_field("rateLimitType")?
                            }
                        };
                        _serde::__private::Ok(RateLimit {
                            interval: __field0,
                            intervalNum: __field1,
                            limit: __field2,
                            rateLimitType: __field3,
                        })
                    }
                }
                #[doc(hidden)]
                const FIELDS: &'static [&'static str] = &[
                    "interval",
                    "intervalNum",
                    "limit",
                    "rateLimitType",
                ];
                _serde::Deserializer::deserialize_struct(
                    __deserializer,
                    "RateLimit",
                    FIELDS,
                    __Visitor {
                        marker: _serde::__private::PhantomData::<RateLimit>,
                        lifetime: _serde::__private::PhantomData,
                    },
                )
            }
        }
    };
    #[doc(hidden)]
    #[allow(non_upper_case_globals, unused_attributes, unused_qualifications)]
    const _: () = {
        #[allow(unused_extern_crates, clippy::useless_attribute)]
        extern crate serde as _serde;
        #[automatically_derived]
        impl _serde::Serialize for RateLimit {
            fn serialize<__S>(
                &self,
                __serializer: __S,
            ) -> _serde::__private::Result<__S::Ok, __S::Error>
            where
                __S: _serde::Serializer,
            {
                let mut __serde_state = _serde::Serializer::serialize_struct(
                    __serializer,
                    "RateLimit",
                    false as usize + 1 + 1 + 1 + 1,
                )?;
                _serde::ser::SerializeStruct::serialize_field(
                    &mut __serde_state,
                    "interval",
                    &self.interval,
                )?;
                _serde::ser::SerializeStruct::serialize_field(
                    &mut __serde_state,
                    "intervalNum",
                    &self.intervalNum,
                )?;
                _serde::ser::SerializeStruct::serialize_field(
                    &mut __serde_state,
                    "limit",
                    &self.limit,
                )?;
                _serde::ser::SerializeStruct::serialize_field(
                    &mut __serde_state,
                    "rateLimitType",
                    &self.rateLimitType,
                )?;
                _serde::ser::SerializeStruct::end(__serde_state)
            }
        }
    };
    #[allow(non_snake_case)]
    struct FuturesSymbol {
        symbol: String,
        pair: String,
        contractType: String,
        deliveryDate: i64,
        onboardDate: i64,
        status: String,
        maintMarginPercent: String,
        requiredMarginPercent: String,
        baseAsset: String,
        quoteAsset: String,
        marginAsset: String,
        pricePrecision: u32,
        quantityPrecision: usize,
        baseAssetPrecision: u32,
        quotePrecision: u32,
        underlyingType: String,
        underlyingSubType: Vec<String>,
        settlePlan: u32,
        triggerProtect: String,
        filters: Vec<Value>,
        OrderType: Option<Vec<String>>,
        timeInForce: Vec<String>,
        liquidationFee: String,
        marketTakeBound: String,
    }
    #[automatically_derived]
    #[allow(non_snake_case)]
    impl ::core::fmt::Debug for FuturesSymbol {
        #[inline]
        fn fmt(&self, f: &mut ::core::fmt::Formatter) -> ::core::fmt::Result {
            let names: &'static _ = &[
                "symbol",
                "pair",
                "contractType",
                "deliveryDate",
                "onboardDate",
                "status",
                "maintMarginPercent",
                "requiredMarginPercent",
                "baseAsset",
                "quoteAsset",
                "marginAsset",
                "pricePrecision",
                "quantityPrecision",
                "baseAssetPrecision",
                "quotePrecision",
                "underlyingType",
                "underlyingSubType",
                "settlePlan",
                "triggerProtect",
                "filters",
                "OrderType",
                "timeInForce",
                "liquidationFee",
                "marketTakeBound",
            ];
            let values: &[&dyn ::core::fmt::Debug] = &[
                &self.symbol,
                &self.pair,
                &self.contractType,
                &self.deliveryDate,
                &self.onboardDate,
                &self.status,
                &self.maintMarginPercent,
                &self.requiredMarginPercent,
                &self.baseAsset,
                &self.quoteAsset,
                &self.marginAsset,
                &self.pricePrecision,
                &self.quantityPrecision,
                &self.baseAssetPrecision,
                &self.quotePrecision,
                &self.underlyingType,
                &self.underlyingSubType,
                &self.settlePlan,
                &self.triggerProtect,
                &self.filters,
                &self.OrderType,
                &self.timeInForce,
                &self.liquidationFee,
                &&self.marketTakeBound,
            ];
            ::core::fmt::Formatter::debug_struct_fields_finish(
                f,
                "FuturesSymbol",
                names,
                values,
            )
        }
    }
    #[doc(hidden)]
    #[allow(non_upper_case_globals, unused_attributes, unused_qualifications)]
    const _: () = {
        #[allow(unused_extern_crates, clippy::useless_attribute)]
        extern crate serde as _serde;
        #[automatically_derived]
        impl<'de> _serde::Deserialize<'de> for FuturesSymbol {
            fn deserialize<__D>(
                __deserializer: __D,
            ) -> _serde::__private::Result<Self, __D::Error>
            where
                __D: _serde::Deserializer<'de>,
            {
                #[allow(non_camel_case_types)]
                #[doc(hidden)]
                enum __Field {
                    __field0,
                    __field1,
                    __field2,
                    __field3,
                    __field4,
                    __field5,
                    __field6,
                    __field7,
                    __field8,
                    __field9,
                    __field10,
                    __field11,
                    __field12,
                    __field13,
                    __field14,
                    __field15,
                    __field16,
                    __field17,
                    __field18,
                    __field19,
                    __field20,
                    __field21,
                    __field22,
                    __field23,
                    __ignore,
                }
                #[doc(hidden)]
                struct __FieldVisitor;
                impl<'de> _serde::de::Visitor<'de> for __FieldVisitor {
                    type Value = __Field;
                    fn expecting(
                        &self,
                        __formatter: &mut _serde::__private::Formatter,
                    ) -> _serde::__private::fmt::Result {
                        _serde::__private::Formatter::write_str(
                            __formatter,
                            "field identifier",
                        )
                    }
                    fn visit_u64<__E>(
                        self,
                        __value: u64,
                    ) -> _serde::__private::Result<Self::Value, __E>
                    where
                        __E: _serde::de::Error,
                    {
                        match __value {
                            0u64 => _serde::__private::Ok(__Field::__field0),
                            1u64 => _serde::__private::Ok(__Field::__field1),
                            2u64 => _serde::__private::Ok(__Field::__field2),
                            3u64 => _serde::__private::Ok(__Field::__field3),
                            4u64 => _serde::__private::Ok(__Field::__field4),
                            5u64 => _serde::__private::Ok(__Field::__field5),
                            6u64 => _serde::__private::Ok(__Field::__field6),
                            7u64 => _serde::__private::Ok(__Field::__field7),
                            8u64 => _serde::__private::Ok(__Field::__field8),
                            9u64 => _serde::__private::Ok(__Field::__field9),
                            10u64 => _serde::__private::Ok(__Field::__field10),
                            11u64 => _serde::__private::Ok(__Field::__field11),
                            12u64 => _serde::__private::Ok(__Field::__field12),
                            13u64 => _serde::__private::Ok(__Field::__field13),
                            14u64 => _serde::__private::Ok(__Field::__field14),
                            15u64 => _serde::__private::Ok(__Field::__field15),
                            16u64 => _serde::__private::Ok(__Field::__field16),
                            17u64 => _serde::__private::Ok(__Field::__field17),
                            18u64 => _serde::__private::Ok(__Field::__field18),
                            19u64 => _serde::__private::Ok(__Field::__field19),
                            20u64 => _serde::__private::Ok(__Field::__field20),
                            21u64 => _serde::__private::Ok(__Field::__field21),
                            22u64 => _serde::__private::Ok(__Field::__field22),
                            23u64 => _serde::__private::Ok(__Field::__field23),
                            _ => _serde::__private::Ok(__Field::__ignore),
                        }
                    }
                    fn visit_str<__E>(
                        self,
                        __value: &str,
                    ) -> _serde::__private::Result<Self::Value, __E>
                    where
                        __E: _serde::de::Error,
                    {
                        match __value {
                            "symbol" => _serde::__private::Ok(__Field::__field0),
                            "pair" => _serde::__private::Ok(__Field::__field1),
                            "contractType" => _serde::__private::Ok(__Field::__field2),
                            "deliveryDate" => _serde::__private::Ok(__Field::__field3),
                            "onboardDate" => _serde::__private::Ok(__Field::__field4),
                            "status" => _serde::__private::Ok(__Field::__field5),
                            "maintMarginPercent" => {
                                _serde::__private::Ok(__Field::__field6)
                            }
                            "requiredMarginPercent" => {
                                _serde::__private::Ok(__Field::__field7)
                            }
                            "baseAsset" => _serde::__private::Ok(__Field::__field8),
                            "quoteAsset" => _serde::__private::Ok(__Field::__field9),
                            "marginAsset" => _serde::__private::Ok(__Field::__field10),
                            "pricePrecision" => _serde::__private::Ok(__Field::__field11),
                            "quantityPrecision" => {
                                _serde::__private::Ok(__Field::__field12)
                            }
                            "baseAssetPrecision" => {
                                _serde::__private::Ok(__Field::__field13)
                            }
                            "quotePrecision" => _serde::__private::Ok(__Field::__field14),
                            "underlyingType" => _serde::__private::Ok(__Field::__field15),
                            "underlyingSubType" => {
                                _serde::__private::Ok(__Field::__field16)
                            }
                            "settlePlan" => _serde::__private::Ok(__Field::__field17),
                            "triggerProtect" => _serde::__private::Ok(__Field::__field18),
                            "filters" => _serde::__private::Ok(__Field::__field19),
                            "OrderType" => _serde::__private::Ok(__Field::__field20),
                            "timeInForce" => _serde::__private::Ok(__Field::__field21),
                            "liquidationFee" => _serde::__private::Ok(__Field::__field22),
                            "marketTakeBound" => {
                                _serde::__private::Ok(__Field::__field23)
                            }
                            _ => _serde::__private::Ok(__Field::__ignore),
                        }
                    }
                    fn visit_bytes<__E>(
                        self,
                        __value: &[u8],
                    ) -> _serde::__private::Result<Self::Value, __E>
                    where
                        __E: _serde::de::Error,
                    {
                        match __value {
                            b"symbol" => _serde::__private::Ok(__Field::__field0),
                            b"pair" => _serde::__private::Ok(__Field::__field1),
                            b"contractType" => _serde::__private::Ok(__Field::__field2),
                            b"deliveryDate" => _serde::__private::Ok(__Field::__field3),
                            b"onboardDate" => _serde::__private::Ok(__Field::__field4),
                            b"status" => _serde::__private::Ok(__Field::__field5),
                            b"maintMarginPercent" => {
                                _serde::__private::Ok(__Field::__field6)
                            }
                            b"requiredMarginPercent" => {
                                _serde::__private::Ok(__Field::__field7)
                            }
                            b"baseAsset" => _serde::__private::Ok(__Field::__field8),
                            b"quoteAsset" => _serde::__private::Ok(__Field::__field9),
                            b"marginAsset" => _serde::__private::Ok(__Field::__field10),
                            b"pricePrecision" => {
                                _serde::__private::Ok(__Field::__field11)
                            }
                            b"quantityPrecision" => {
                                _serde::__private::Ok(__Field::__field12)
                            }
                            b"baseAssetPrecision" => {
                                _serde::__private::Ok(__Field::__field13)
                            }
                            b"quotePrecision" => {
                                _serde::__private::Ok(__Field::__field14)
                            }
                            b"underlyingType" => {
                                _serde::__private::Ok(__Field::__field15)
                            }
                            b"underlyingSubType" => {
                                _serde::__private::Ok(__Field::__field16)
                            }
                            b"settlePlan" => _serde::__private::Ok(__Field::__field17),
                            b"triggerProtect" => {
                                _serde::__private::Ok(__Field::__field18)
                            }
                            b"filters" => _serde::__private::Ok(__Field::__field19),
                            b"OrderType" => _serde::__private::Ok(__Field::__field20),
                            b"timeInForce" => _serde::__private::Ok(__Field::__field21),
                            b"liquidationFee" => {
                                _serde::__private::Ok(__Field::__field22)
                            }
                            b"marketTakeBound" => {
                                _serde::__private::Ok(__Field::__field23)
                            }
                            _ => _serde::__private::Ok(__Field::__ignore),
                        }
                    }
                }
                impl<'de> _serde::Deserialize<'de> for __Field {
                    #[inline]
                    fn deserialize<__D>(
                        __deserializer: __D,
                    ) -> _serde::__private::Result<Self, __D::Error>
                    where
                        __D: _serde::Deserializer<'de>,
                    {
                        _serde::Deserializer::deserialize_identifier(
                            __deserializer,
                            __FieldVisitor,
                        )
                    }
                }
                #[doc(hidden)]
                struct __Visitor<'de> {
                    marker: _serde::__private::PhantomData<FuturesSymbol>,
                    lifetime: _serde::__private::PhantomData<&'de ()>,
                }
                impl<'de> _serde::de::Visitor<'de> for __Visitor<'de> {
                    type Value = FuturesSymbol;
                    fn expecting(
                        &self,
                        __formatter: &mut _serde::__private::Formatter,
                    ) -> _serde::__private::fmt::Result {
                        _serde::__private::Formatter::write_str(
                            __formatter,
                            "struct FuturesSymbol",
                        )
                    }
                    #[inline]
                    fn visit_seq<__A>(
                        self,
                        mut __seq: __A,
                    ) -> _serde::__private::Result<Self::Value, __A::Error>
                    where
                        __A: _serde::de::SeqAccess<'de>,
                    {
                        let __field0 = match _serde::de::SeqAccess::next_element::<
                            String,
                        >(&mut __seq)? {
                            _serde::__private::Some(__value) => __value,
                            _serde::__private::None => {
                                return _serde::__private::Err(
                                    _serde::de::Error::invalid_length(
                                        0usize,
                                        &"struct FuturesSymbol with 24 elements",
                                    ),
                                );
                            }
                        };
                        let __field1 = match _serde::de::SeqAccess::next_element::<
                            String,
                        >(&mut __seq)? {
                            _serde::__private::Some(__value) => __value,
                            _serde::__private::None => {
                                return _serde::__private::Err(
                                    _serde::de::Error::invalid_length(
                                        1usize,
                                        &"struct FuturesSymbol with 24 elements",
                                    ),
                                );
                            }
                        };
                        let __field2 = match _serde::de::SeqAccess::next_element::<
                            String,
                        >(&mut __seq)? {
                            _serde::__private::Some(__value) => __value,
                            _serde::__private::None => {
                                return _serde::__private::Err(
                                    _serde::de::Error::invalid_length(
                                        2usize,
                                        &"struct FuturesSymbol with 24 elements",
                                    ),
                                );
                            }
                        };
                        let __field3 = match _serde::de::SeqAccess::next_element::<
                            i64,
                        >(&mut __seq)? {
                            _serde::__private::Some(__value) => __value,
                            _serde::__private::None => {
                                return _serde::__private::Err(
                                    _serde::de::Error::invalid_length(
                                        3usize,
                                        &"struct FuturesSymbol with 24 elements",
                                    ),
                                );
                            }
                        };
                        let __field4 = match _serde::de::SeqAccess::next_element::<
                            i64,
                        >(&mut __seq)? {
                            _serde::__private::Some(__value) => __value,
                            _serde::__private::None => {
                                return _serde::__private::Err(
                                    _serde::de::Error::invalid_length(
                                        4usize,
                                        &"struct FuturesSymbol with 24 elements",
                                    ),
                                );
                            }
                        };
                        let __field5 = match _serde::de::SeqAccess::next_element::<
                            String,
                        >(&mut __seq)? {
                            _serde::__private::Some(__value) => __value,
                            _serde::__private::None => {
                                return _serde::__private::Err(
                                    _serde::de::Error::invalid_length(
                                        5usize,
                                        &"struct FuturesSymbol with 24 elements",
                                    ),
                                );
                            }
                        };
                        let __field6 = match _serde::de::SeqAccess::next_element::<
                            String,
                        >(&mut __seq)? {
                            _serde::__private::Some(__value) => __value,
                            _serde::__private::None => {
                                return _serde::__private::Err(
                                    _serde::de::Error::invalid_length(
                                        6usize,
                                        &"struct FuturesSymbol with 24 elements",
                                    ),
                                );
                            }
                        };
                        let __field7 = match _serde::de::SeqAccess::next_element::<
                            String,
                        >(&mut __seq)? {
                            _serde::__private::Some(__value) => __value,
                            _serde::__private::None => {
                                return _serde::__private::Err(
                                    _serde::de::Error::invalid_length(
                                        7usize,
                                        &"struct FuturesSymbol with 24 elements",
                                    ),
                                );
                            }
                        };
                        let __field8 = match _serde::de::SeqAccess::next_element::<
                            String,
                        >(&mut __seq)? {
                            _serde::__private::Some(__value) => __value,
                            _serde::__private::None => {
                                return _serde::__private::Err(
                                    _serde::de::Error::invalid_length(
                                        8usize,
                                        &"struct FuturesSymbol with 24 elements",
                                    ),
                                );
                            }
                        };
                        let __field9 = match _serde::de::SeqAccess::next_element::<
                            String,
                        >(&mut __seq)? {
                            _serde::__private::Some(__value) => __value,
                            _serde::__private::None => {
                                return _serde::__private::Err(
                                    _serde::de::Error::invalid_length(
                                        9usize,
                                        &"struct FuturesSymbol with 24 elements",
                                    ),
                                );
                            }
                        };
                        let __field10 = match _serde::de::SeqAccess::next_element::<
                            String,
                        >(&mut __seq)? {
                            _serde::__private::Some(__value) => __value,
                            _serde::__private::None => {
                                return _serde::__private::Err(
                                    _serde::de::Error::invalid_length(
                                        10usize,
                                        &"struct FuturesSymbol with 24 elements",
                                    ),
                                );
                            }
                        };
                        let __field11 = match _serde::de::SeqAccess::next_element::<
                            u32,
                        >(&mut __seq)? {
                            _serde::__private::Some(__value) => __value,
                            _serde::__private::None => {
                                return _serde::__private::Err(
                                    _serde::de::Error::invalid_length(
                                        11usize,
                                        &"struct FuturesSymbol with 24 elements",
                                    ),
                                );
                            }
                        };
                        let __field12 = match _serde::de::SeqAccess::next_element::<
                            usize,
                        >(&mut __seq)? {
                            _serde::__private::Some(__value) => __value,
                            _serde::__private::None => {
                                return _serde::__private::Err(
                                    _serde::de::Error::invalid_length(
                                        12usize,
                                        &"struct FuturesSymbol with 24 elements",
                                    ),
                                );
                            }
                        };
                        let __field13 = match _serde::de::SeqAccess::next_element::<
                            u32,
                        >(&mut __seq)? {
                            _serde::__private::Some(__value) => __value,
                            _serde::__private::None => {
                                return _serde::__private::Err(
                                    _serde::de::Error::invalid_length(
                                        13usize,
                                        &"struct FuturesSymbol with 24 elements",
                                    ),
                                );
                            }
                        };
                        let __field14 = match _serde::de::SeqAccess::next_element::<
                            u32,
                        >(&mut __seq)? {
                            _serde::__private::Some(__value) => __value,
                            _serde::__private::None => {
                                return _serde::__private::Err(
                                    _serde::de::Error::invalid_length(
                                        14usize,
                                        &"struct FuturesSymbol with 24 elements",
                                    ),
                                );
                            }
                        };
                        let __field15 = match _serde::de::SeqAccess::next_element::<
                            String,
                        >(&mut __seq)? {
                            _serde::__private::Some(__value) => __value,
                            _serde::__private::None => {
                                return _serde::__private::Err(
                                    _serde::de::Error::invalid_length(
                                        15usize,
                                        &"struct FuturesSymbol with 24 elements",
                                    ),
                                );
                            }
                        };
                        let __field16 = match _serde::de::SeqAccess::next_element::<
                            Vec<String>,
                        >(&mut __seq)? {
                            _serde::__private::Some(__value) => __value,
                            _serde::__private::None => {
                                return _serde::__private::Err(
                                    _serde::de::Error::invalid_length(
                                        16usize,
                                        &"struct FuturesSymbol with 24 elements",
                                    ),
                                );
                            }
                        };
                        let __field17 = match _serde::de::SeqAccess::next_element::<
                            u32,
                        >(&mut __seq)? {
                            _serde::__private::Some(__value) => __value,
                            _serde::__private::None => {
                                return _serde::__private::Err(
                                    _serde::de::Error::invalid_length(
                                        17usize,
                                        &"struct FuturesSymbol with 24 elements",
                                    ),
                                );
                            }
                        };
                        let __field18 = match _serde::de::SeqAccess::next_element::<
                            String,
                        >(&mut __seq)? {
                            _serde::__private::Some(__value) => __value,
                            _serde::__private::None => {
                                return _serde::__private::Err(
                                    _serde::de::Error::invalid_length(
                                        18usize,
                                        &"struct FuturesSymbol with 24 elements",
                                    ),
                                );
                            }
                        };
                        let __field19 = match _serde::de::SeqAccess::next_element::<
                            Vec<Value>,
                        >(&mut __seq)? {
                            _serde::__private::Some(__value) => __value,
                            _serde::__private::None => {
                                return _serde::__private::Err(
                                    _serde::de::Error::invalid_length(
                                        19usize,
                                        &"struct FuturesSymbol with 24 elements",
                                    ),
                                );
                            }
                        };
                        let __field20 = match _serde::de::SeqAccess::next_element::<
                            Option<Vec<String>>,
                        >(&mut __seq)? {
                            _serde::__private::Some(__value) => __value,
                            _serde::__private::None => {
                                return _serde::__private::Err(
                                    _serde::de::Error::invalid_length(
                                        20usize,
                                        &"struct FuturesSymbol with 24 elements",
                                    ),
                                );
                            }
                        };
                        let __field21 = match _serde::de::SeqAccess::next_element::<
                            Vec<String>,
                        >(&mut __seq)? {
                            _serde::__private::Some(__value) => __value,
                            _serde::__private::None => {
                                return _serde::__private::Err(
                                    _serde::de::Error::invalid_length(
                                        21usize,
                                        &"struct FuturesSymbol with 24 elements",
                                    ),
                                );
                            }
                        };
                        let __field22 = match _serde::de::SeqAccess::next_element::<
                            String,
                        >(&mut __seq)? {
                            _serde::__private::Some(__value) => __value,
                            _serde::__private::None => {
                                return _serde::__private::Err(
                                    _serde::de::Error::invalid_length(
                                        22usize,
                                        &"struct FuturesSymbol with 24 elements",
                                    ),
                                );
                            }
                        };
                        let __field23 = match _serde::de::SeqAccess::next_element::<
                            String,
                        >(&mut __seq)? {
                            _serde::__private::Some(__value) => __value,
                            _serde::__private::None => {
                                return _serde::__private::Err(
                                    _serde::de::Error::invalid_length(
                                        23usize,
                                        &"struct FuturesSymbol with 24 elements",
                                    ),
                                );
                            }
                        };
                        _serde::__private::Ok(FuturesSymbol {
                            symbol: __field0,
                            pair: __field1,
                            contractType: __field2,
                            deliveryDate: __field3,
                            onboardDate: __field4,
                            status: __field5,
                            maintMarginPercent: __field6,
                            requiredMarginPercent: __field7,
                            baseAsset: __field8,
                            quoteAsset: __field9,
                            marginAsset: __field10,
                            pricePrecision: __field11,
                            quantityPrecision: __field12,
                            baseAssetPrecision: __field13,
                            quotePrecision: __field14,
                            underlyingType: __field15,
                            underlyingSubType: __field16,
                            settlePlan: __field17,
                            triggerProtect: __field18,
                            filters: __field19,
                            OrderType: __field20,
                            timeInForce: __field21,
                            liquidationFee: __field22,
                            marketTakeBound: __field23,
                        })
                    }
                    #[inline]
                    fn visit_map<__A>(
                        self,
                        mut __map: __A,
                    ) -> _serde::__private::Result<Self::Value, __A::Error>
                    where
                        __A: _serde::de::MapAccess<'de>,
                    {
                        let mut __field0: _serde::__private::Option<String> = _serde::__private::None;
                        let mut __field1: _serde::__private::Option<String> = _serde::__private::None;
                        let mut __field2: _serde::__private::Option<String> = _serde::__private::None;
                        let mut __field3: _serde::__private::Option<i64> = _serde::__private::None;
                        let mut __field4: _serde::__private::Option<i64> = _serde::__private::None;
                        let mut __field5: _serde::__private::Option<String> = _serde::__private::None;
                        let mut __field6: _serde::__private::Option<String> = _serde::__private::None;
                        let mut __field7: _serde::__private::Option<String> = _serde::__private::None;
                        let mut __field8: _serde::__private::Option<String> = _serde::__private::None;
                        let mut __field9: _serde::__private::Option<String> = _serde::__private::None;
                        let mut __field10: _serde::__private::Option<String> = _serde::__private::None;
                        let mut __field11: _serde::__private::Option<u32> = _serde::__private::None;
                        let mut __field12: _serde::__private::Option<usize> = _serde::__private::None;
                        let mut __field13: _serde::__private::Option<u32> = _serde::__private::None;
                        let mut __field14: _serde::__private::Option<u32> = _serde::__private::None;
                        let mut __field15: _serde::__private::Option<String> = _serde::__private::None;
                        let mut __field16: _serde::__private::Option<Vec<String>> = _serde::__private::None;
                        let mut __field17: _serde::__private::Option<u32> = _serde::__private::None;
                        let mut __field18: _serde::__private::Option<String> = _serde::__private::None;
                        let mut __field19: _serde::__private::Option<Vec<Value>> = _serde::__private::None;
                        let mut __field20: _serde::__private::Option<
                            Option<Vec<String>>,
                        > = _serde::__private::None;
                        let mut __field21: _serde::__private::Option<Vec<String>> = _serde::__private::None;
                        let mut __field22: _serde::__private::Option<String> = _serde::__private::None;
                        let mut __field23: _serde::__private::Option<String> = _serde::__private::None;
                        while let _serde::__private::Some(__key) = _serde::de::MapAccess::next_key::<
                            __Field,
                        >(&mut __map)? {
                            match __key {
                                __Field::__field0 => {
                                    if _serde::__private::Option::is_some(&__field0) {
                                        return _serde::__private::Err(
                                            <__A::Error as _serde::de::Error>::duplicate_field("symbol"),
                                        );
                                    }
                                    __field0 = _serde::__private::Some(
                                        _serde::de::MapAccess::next_value::<String>(&mut __map)?,
                                    );
                                }
                                __Field::__field1 => {
                                    if _serde::__private::Option::is_some(&__field1) {
                                        return _serde::__private::Err(
                                            <__A::Error as _serde::de::Error>::duplicate_field("pair"),
                                        );
                                    }
                                    __field1 = _serde::__private::Some(
                                        _serde::de::MapAccess::next_value::<String>(&mut __map)?,
                                    );
                                }
                                __Field::__field2 => {
                                    if _serde::__private::Option::is_some(&__field2) {
                                        return _serde::__private::Err(
                                            <__A::Error as _serde::de::Error>::duplicate_field(
                                                "contractType",
                                            ),
                                        );
                                    }
                                    __field2 = _serde::__private::Some(
                                        _serde::de::MapAccess::next_value::<String>(&mut __map)?,
                                    );
                                }
                                __Field::__field3 => {
                                    if _serde::__private::Option::is_some(&__field3) {
                                        return _serde::__private::Err(
                                            <__A::Error as _serde::de::Error>::duplicate_field(
                                                "deliveryDate",
                                            ),
                                        );
                                    }
                                    __field3 = _serde::__private::Some(
                                        _serde::de::MapAccess::next_value::<i64>(&mut __map)?,
                                    );
                                }
                                __Field::__field4 => {
                                    if _serde::__private::Option::is_some(&__field4) {
                                        return _serde::__private::Err(
                                            <__A::Error as _serde::de::Error>::duplicate_field(
                                                "onboardDate",
                                            ),
                                        );
                                    }
                                    __field4 = _serde::__private::Some(
                                        _serde::de::MapAccess::next_value::<i64>(&mut __map)?,
                                    );
                                }
                                __Field::__field5 => {
                                    if _serde::__private::Option::is_some(&__field5) {
                                        return _serde::__private::Err(
                                            <__A::Error as _serde::de::Error>::duplicate_field("status"),
                                        );
                                    }
                                    __field5 = _serde::__private::Some(
                                        _serde::de::MapAccess::next_value::<String>(&mut __map)?,
                                    );
                                }
                                __Field::__field6 => {
                                    if _serde::__private::Option::is_some(&__field6) {
                                        return _serde::__private::Err(
                                            <__A::Error as _serde::de::Error>::duplicate_field(
                                                "maintMarginPercent",
                                            ),
                                        );
                                    }
                                    __field6 = _serde::__private::Some(
                                        _serde::de::MapAccess::next_value::<String>(&mut __map)?,
                                    );
                                }
                                __Field::__field7 => {
                                    if _serde::__private::Option::is_some(&__field7) {
                                        return _serde::__private::Err(
                                            <__A::Error as _serde::de::Error>::duplicate_field(
                                                "requiredMarginPercent",
                                            ),
                                        );
                                    }
                                    __field7 = _serde::__private::Some(
                                        _serde::de::MapAccess::next_value::<String>(&mut __map)?,
                                    );
                                }
                                __Field::__field8 => {
                                    if _serde::__private::Option::is_some(&__field8) {
                                        return _serde::__private::Err(
                                            <__A::Error as _serde::de::Error>::duplicate_field(
                                                "baseAsset",
                                            ),
                                        );
                                    }
                                    __field8 = _serde::__private::Some(
                                        _serde::de::MapAccess::next_value::<String>(&mut __map)?,
                                    );
                                }
                                __Field::__field9 => {
                                    if _serde::__private::Option::is_some(&__field9) {
                                        return _serde::__private::Err(
                                            <__A::Error as _serde::de::Error>::duplicate_field(
                                                "quoteAsset",
                                            ),
                                        );
                                    }
                                    __field9 = _serde::__private::Some(
                                        _serde::de::MapAccess::next_value::<String>(&mut __map)?,
                                    );
                                }
                                __Field::__field10 => {
                                    if _serde::__private::Option::is_some(&__field10) {
                                        return _serde::__private::Err(
                                            <__A::Error as _serde::de::Error>::duplicate_field(
                                                "marginAsset",
                                            ),
                                        );
                                    }
                                    __field10 = _serde::__private::Some(
                                        _serde::de::MapAccess::next_value::<String>(&mut __map)?,
                                    );
                                }
                                __Field::__field11 => {
                                    if _serde::__private::Option::is_some(&__field11) {
                                        return _serde::__private::Err(
                                            <__A::Error as _serde::de::Error>::duplicate_field(
                                                "pricePrecision",
                                            ),
                                        );
                                    }
                                    __field11 = _serde::__private::Some(
                                        _serde::de::MapAccess::next_value::<u32>(&mut __map)?,
                                    );
                                }
                                __Field::__field12 => {
                                    if _serde::__private::Option::is_some(&__field12) {
                                        return _serde::__private::Err(
                                            <__A::Error as _serde::de::Error>::duplicate_field(
                                                "quantityPrecision",
                                            ),
                                        );
                                    }
                                    __field12 = _serde::__private::Some(
                                        _serde::de::MapAccess::next_value::<usize>(&mut __map)?,
                                    );
                                }
                                __Field::__field13 => {
                                    if _serde::__private::Option::is_some(&__field13) {
                                        return _serde::__private::Err(
                                            <__A::Error as _serde::de::Error>::duplicate_field(
                                                "baseAssetPrecision",
                                            ),
                                        );
                                    }
                                    __field13 = _serde::__private::Some(
                                        _serde::de::MapAccess::next_value::<u32>(&mut __map)?,
                                    );
                                }
                                __Field::__field14 => {
                                    if _serde::__private::Option::is_some(&__field14) {
                                        return _serde::__private::Err(
                                            <__A::Error as _serde::de::Error>::duplicate_field(
                                                "quotePrecision",
                                            ),
                                        );
                                    }
                                    __field14 = _serde::__private::Some(
                                        _serde::de::MapAccess::next_value::<u32>(&mut __map)?,
                                    );
                                }
                                __Field::__field15 => {
                                    if _serde::__private::Option::is_some(&__field15) {
                                        return _serde::__private::Err(
                                            <__A::Error as _serde::de::Error>::duplicate_field(
                                                "underlyingType",
                                            ),
                                        );
                                    }
                                    __field15 = _serde::__private::Some(
                                        _serde::de::MapAccess::next_value::<String>(&mut __map)?,
                                    );
                                }
                                __Field::__field16 => {
                                    if _serde::__private::Option::is_some(&__field16) {
                                        return _serde::__private::Err(
                                            <__A::Error as _serde::de::Error>::duplicate_field(
                                                "underlyingSubType",
                                            ),
                                        );
                                    }
                                    __field16 = _serde::__private::Some(
                                        _serde::de::MapAccess::next_value::<
                                            Vec<String>,
                                        >(&mut __map)?,
                                    );
                                }
                                __Field::__field17 => {
                                    if _serde::__private::Option::is_some(&__field17) {
                                        return _serde::__private::Err(
                                            <__A::Error as _serde::de::Error>::duplicate_field(
                                                "settlePlan",
                                            ),
                                        );
                                    }
                                    __field17 = _serde::__private::Some(
                                        _serde::de::MapAccess::next_value::<u32>(&mut __map)?,
                                    );
                                }
                                __Field::__field18 => {
                                    if _serde::__private::Option::is_some(&__field18) {
                                        return _serde::__private::Err(
                                            <__A::Error as _serde::de::Error>::duplicate_field(
                                                "triggerProtect",
                                            ),
                                        );
                                    }
                                    __field18 = _serde::__private::Some(
                                        _serde::de::MapAccess::next_value::<String>(&mut __map)?,
                                    );
                                }
                                __Field::__field19 => {
                                    if _serde::__private::Option::is_some(&__field19) {
                                        return _serde::__private::Err(
                                            <__A::Error as _serde::de::Error>::duplicate_field(
                                                "filters",
                                            ),
                                        );
                                    }
                                    __field19 = _serde::__private::Some(
                                        _serde::de::MapAccess::next_value::<Vec<Value>>(&mut __map)?,
                                    );
                                }
                                __Field::__field20 => {
                                    if _serde::__private::Option::is_some(&__field20) {
                                        return _serde::__private::Err(
                                            <__A::Error as _serde::de::Error>::duplicate_field(
                                                "OrderType",
                                            ),
                                        );
                                    }
                                    __field20 = _serde::__private::Some(
                                        _serde::de::MapAccess::next_value::<
                                            Option<Vec<String>>,
                                        >(&mut __map)?,
                                    );
                                }
                                __Field::__field21 => {
                                    if _serde::__private::Option::is_some(&__field21) {
                                        return _serde::__private::Err(
                                            <__A::Error as _serde::de::Error>::duplicate_field(
                                                "timeInForce",
                                            ),
                                        );
                                    }
                                    __field21 = _serde::__private::Some(
                                        _serde::de::MapAccess::next_value::<
                                            Vec<String>,
                                        >(&mut __map)?,
                                    );
                                }
                                __Field::__field22 => {
                                    if _serde::__private::Option::is_some(&__field22) {
                                        return _serde::__private::Err(
                                            <__A::Error as _serde::de::Error>::duplicate_field(
                                                "liquidationFee",
                                            ),
                                        );
                                    }
                                    __field22 = _serde::__private::Some(
                                        _serde::de::MapAccess::next_value::<String>(&mut __map)?,
                                    );
                                }
                                __Field::__field23 => {
                                    if _serde::__private::Option::is_some(&__field23) {
                                        return _serde::__private::Err(
                                            <__A::Error as _serde::de::Error>::duplicate_field(
                                                "marketTakeBound",
                                            ),
                                        );
                                    }
                                    __field23 = _serde::__private::Some(
                                        _serde::de::MapAccess::next_value::<String>(&mut __map)?,
                                    );
                                }
                                _ => {
                                    let _ = _serde::de::MapAccess::next_value::<
                                        _serde::de::IgnoredAny,
                                    >(&mut __map)?;
                                }
                            }
                        }
                        let __field0 = match __field0 {
                            _serde::__private::Some(__field0) => __field0,
                            _serde::__private::None => {
                                _serde::__private::de::missing_field("symbol")?
                            }
                        };
                        let __field1 = match __field1 {
                            _serde::__private::Some(__field1) => __field1,
                            _serde::__private::None => {
                                _serde::__private::de::missing_field("pair")?
                            }
                        };
                        let __field2 = match __field2 {
                            _serde::__private::Some(__field2) => __field2,
                            _serde::__private::None => {
                                _serde::__private::de::missing_field("contractType")?
                            }
                        };
                        let __field3 = match __field3 {
                            _serde::__private::Some(__field3) => __field3,
                            _serde::__private::None => {
                                _serde::__private::de::missing_field("deliveryDate")?
                            }
                        };
                        let __field4 = match __field4 {
                            _serde::__private::Some(__field4) => __field4,
                            _serde::__private::None => {
                                _serde::__private::de::missing_field("onboardDate")?
                            }
                        };
                        let __field5 = match __field5 {
                            _serde::__private::Some(__field5) => __field5,
                            _serde::__private::None => {
                                _serde::__private::de::missing_field("status")?
                            }
                        };
                        let __field6 = match __field6 {
                            _serde::__private::Some(__field6) => __field6,
                            _serde::__private::None => {
                                _serde::__private::de::missing_field("maintMarginPercent")?
                            }
                        };
                        let __field7 = match __field7 {
                            _serde::__private::Some(__field7) => __field7,
                            _serde::__private::None => {
                                _serde::__private::de::missing_field(
                                    "requiredMarginPercent",
                                )?
                            }
                        };
                        let __field8 = match __field8 {
                            _serde::__private::Some(__field8) => __field8,
                            _serde::__private::None => {
                                _serde::__private::de::missing_field("baseAsset")?
                            }
                        };
                        let __field9 = match __field9 {
                            _serde::__private::Some(__field9) => __field9,
                            _serde::__private::None => {
                                _serde::__private::de::missing_field("quoteAsset")?
                            }
                        };
                        let __field10 = match __field10 {
                            _serde::__private::Some(__field10) => __field10,
                            _serde::__private::None => {
                                _serde::__private::de::missing_field("marginAsset")?
                            }
                        };
                        let __field11 = match __field11 {
                            _serde::__private::Some(__field11) => __field11,
                            _serde::__private::None => {
                                _serde::__private::de::missing_field("pricePrecision")?
                            }
                        };
                        let __field12 = match __field12 {
                            _serde::__private::Some(__field12) => __field12,
                            _serde::__private::None => {
                                _serde::__private::de::missing_field("quantityPrecision")?
                            }
                        };
                        let __field13 = match __field13 {
                            _serde::__private::Some(__field13) => __field13,
                            _serde::__private::None => {
                                _serde::__private::de::missing_field("baseAssetPrecision")?
                            }
                        };
                        let __field14 = match __field14 {
                            _serde::__private::Some(__field14) => __field14,
                            _serde::__private::None => {
                                _serde::__private::de::missing_field("quotePrecision")?
                            }
                        };
                        let __field15 = match __field15 {
                            _serde::__private::Some(__field15) => __field15,
                            _serde::__private::None => {
                                _serde::__private::de::missing_field("underlyingType")?
                            }
                        };
                        let __field16 = match __field16 {
                            _serde::__private::Some(__field16) => __field16,
                            _serde::__private::None => {
                                _serde::__private::de::missing_field("underlyingSubType")?
                            }
                        };
                        let __field17 = match __field17 {
                            _serde::__private::Some(__field17) => __field17,
                            _serde::__private::None => {
                                _serde::__private::de::missing_field("settlePlan")?
                            }
                        };
                        let __field18 = match __field18 {
                            _serde::__private::Some(__field18) => __field18,
                            _serde::__private::None => {
                                _serde::__private::de::missing_field("triggerProtect")?
                            }
                        };
                        let __field19 = match __field19 {
                            _serde::__private::Some(__field19) => __field19,
                            _serde::__private::None => {
                                _serde::__private::de::missing_field("filters")?
                            }
                        };
                        let __field20 = match __field20 {
                            _serde::__private::Some(__field20) => __field20,
                            _serde::__private::None => {
                                _serde::__private::de::missing_field("OrderType")?
                            }
                        };
                        let __field21 = match __field21 {
                            _serde::__private::Some(__field21) => __field21,
                            _serde::__private::None => {
                                _serde::__private::de::missing_field("timeInForce")?
                            }
                        };
                        let __field22 = match __field22 {
                            _serde::__private::Some(__field22) => __field22,
                            _serde::__private::None => {
                                _serde::__private::de::missing_field("liquidationFee")?
                            }
                        };
                        let __field23 = match __field23 {
                            _serde::__private::Some(__field23) => __field23,
                            _serde::__private::None => {
                                _serde::__private::de::missing_field("marketTakeBound")?
                            }
                        };
                        _serde::__private::Ok(FuturesSymbol {
                            symbol: __field0,
                            pair: __field1,
                            contractType: __field2,
                            deliveryDate: __field3,
                            onboardDate: __field4,
                            status: __field5,
                            maintMarginPercent: __field6,
                            requiredMarginPercent: __field7,
                            baseAsset: __field8,
                            quoteAsset: __field9,
                            marginAsset: __field10,
                            pricePrecision: __field11,
                            quantityPrecision: __field12,
                            baseAssetPrecision: __field13,
                            quotePrecision: __field14,
                            underlyingType: __field15,
                            underlyingSubType: __field16,
                            settlePlan: __field17,
                            triggerProtect: __field18,
                            filters: __field19,
                            OrderType: __field20,
                            timeInForce: __field21,
                            liquidationFee: __field22,
                            marketTakeBound: __field23,
                        })
                    }
                }
                #[doc(hidden)]
                const FIELDS: &'static [&'static str] = &[
                    "symbol",
                    "pair",
                    "contractType",
                    "deliveryDate",
                    "onboardDate",
                    "status",
                    "maintMarginPercent",
                    "requiredMarginPercent",
                    "baseAsset",
                    "quoteAsset",
                    "marginAsset",
                    "pricePrecision",
                    "quantityPrecision",
                    "baseAssetPrecision",
                    "quotePrecision",
                    "underlyingType",
                    "underlyingSubType",
                    "settlePlan",
                    "triggerProtect",
                    "filters",
                    "OrderType",
                    "timeInForce",
                    "liquidationFee",
                    "marketTakeBound",
                ];
                _serde::Deserializer::deserialize_struct(
                    __deserializer,
                    "FuturesSymbol",
                    FIELDS,
                    __Visitor {
                        marker: _serde::__private::PhantomData::<FuturesSymbol>,
                        lifetime: _serde::__private::PhantomData,
                    },
                )
            }
        }
    };
    #[doc(hidden)]
    #[allow(non_upper_case_globals, unused_attributes, unused_qualifications)]
    const _: () = {
        #[allow(unused_extern_crates, clippy::useless_attribute)]
        extern crate serde as _serde;
        #[automatically_derived]
        impl _serde::Serialize for FuturesSymbol {
            fn serialize<__S>(
                &self,
                __serializer: __S,
            ) -> _serde::__private::Result<__S::Ok, __S::Error>
            where
                __S: _serde::Serializer,
            {
                let mut __serde_state = _serde::Serializer::serialize_struct(
                    __serializer,
                    "FuturesSymbol",
                    false as usize + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1
                        + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1,
                )?;
                _serde::ser::SerializeStruct::serialize_field(
                    &mut __serde_state,
                    "symbol",
                    &self.symbol,
                )?;
                _serde::ser::SerializeStruct::serialize_field(
                    &mut __serde_state,
                    "pair",
                    &self.pair,
                )?;
                _serde::ser::SerializeStruct::serialize_field(
                    &mut __serde_state,
                    "contractType",
                    &self.contractType,
                )?;
                _serde::ser::SerializeStruct::serialize_field(
                    &mut __serde_state,
                    "deliveryDate",
                    &self.deliveryDate,
                )?;
                _serde::ser::SerializeStruct::serialize_field(
                    &mut __serde_state,
                    "onboardDate",
                    &self.onboardDate,
                )?;
                _serde::ser::SerializeStruct::serialize_field(
                    &mut __serde_state,
                    "status",
                    &self.status,
                )?;
                _serde::ser::SerializeStruct::serialize_field(
                    &mut __serde_state,
                    "maintMarginPercent",
                    &self.maintMarginPercent,
                )?;
                _serde::ser::SerializeStruct::serialize_field(
                    &mut __serde_state,
                    "requiredMarginPercent",
                    &self.requiredMarginPercent,
                )?;
                _serde::ser::SerializeStruct::serialize_field(
                    &mut __serde_state,
                    "baseAsset",
                    &self.baseAsset,
                )?;
                _serde::ser::SerializeStruct::serialize_field(
                    &mut __serde_state,
                    "quoteAsset",
                    &self.quoteAsset,
                )?;
                _serde::ser::SerializeStruct::serialize_field(
                    &mut __serde_state,
                    "marginAsset",
                    &self.marginAsset,
                )?;
                _serde::ser::SerializeStruct::serialize_field(
                    &mut __serde_state,
                    "pricePrecision",
                    &self.pricePrecision,
                )?;
                _serde::ser::SerializeStruct::serialize_field(
                    &mut __serde_state,
                    "quantityPrecision",
                    &self.quantityPrecision,
                )?;
                _serde::ser::SerializeStruct::serialize_field(
                    &mut __serde_state,
                    "baseAssetPrecision",
                    &self.baseAssetPrecision,
                )?;
                _serde::ser::SerializeStruct::serialize_field(
                    &mut __serde_state,
                    "quotePrecision",
                    &self.quotePrecision,
                )?;
                _serde::ser::SerializeStruct::serialize_field(
                    &mut __serde_state,
                    "underlyingType",
                    &self.underlyingType,
                )?;
                _serde::ser::SerializeStruct::serialize_field(
                    &mut __serde_state,
                    "underlyingSubType",
                    &self.underlyingSubType,
                )?;
                _serde::ser::SerializeStruct::serialize_field(
                    &mut __serde_state,
                    "settlePlan",
                    &self.settlePlan,
                )?;
                _serde::ser::SerializeStruct::serialize_field(
                    &mut __serde_state,
                    "triggerProtect",
                    &self.triggerProtect,
                )?;
                _serde::ser::SerializeStruct::serialize_field(
                    &mut __serde_state,
                    "filters",
                    &self.filters,
                )?;
                _serde::ser::SerializeStruct::serialize_field(
                    &mut __serde_state,
                    "OrderType",
                    &self.OrderType,
                )?;
                _serde::ser::SerializeStruct::serialize_field(
                    &mut __serde_state,
                    "timeInForce",
                    &self.timeInForce,
                )?;
                _serde::ser::SerializeStruct::serialize_field(
                    &mut __serde_state,
                    "liquidationFee",
                    &self.liquidationFee,
                )?;
                _serde::ser::SerializeStruct::serialize_field(
                    &mut __serde_state,
                    "marketTakeBound",
                    &self.marketTakeBound,
                )?;
                _serde::ser::SerializeStruct::end(__serde_state)
            }
        }
    };
}
pub mod config {
    use anyhow::{Context, Result};
    use serde::de::{self, Deserializer, Visitor};
    use serde::Deserialize;
    use std::convert::TryFrom;
    use std::fmt;
    use v_utils::expanded_path::ExpandedPath;
    impl TryFrom<ExpandedPath> for Config {
        type Error = anyhow::Error;
        fn try_from(path: ExpandedPath) -> Result<Self> {
            let raw_config_str = std::fs::read_to_string(&path)
                .with_context(|| {
                    let res = ::alloc::fmt::format(
                        format_args!("Failed to read config file at {0:?}", path),
                    );
                    res
                })?;
            let raw_config: RawConfig = toml::from_str(&raw_config_str)
                .with_context(|| {
                    "The config file is not correctly formatted TOML\nand/or\n is missing some of the required fields"
                })?;
            let config: Config = raw_config.process()?;
            Ok(config)
        }
    }
    pub struct Config {
        pub binance: Binance,
    }
    #[automatically_derived]
    impl ::core::clone::Clone for Config {
        #[inline]
        fn clone(&self) -> Config {
            Config {
                binance: ::core::clone::Clone::clone(&self.binance),
            }
        }
    }
    #[automatically_derived]
    impl ::core::fmt::Debug for Config {
        #[inline]
        fn fmt(&self, f: &mut ::core::fmt::Formatter) -> ::core::fmt::Result {
            ::core::fmt::Formatter::debug_struct_field1_finish(
                f,
                "Config",
                "binance",
                &&self.binance,
            )
        }
    }
    pub struct Binance {
        pub full_key: String,
        pub full_secret: String,
        pub read_key: String,
        pub read_secret: String,
    }
    #[automatically_derived]
    impl ::core::clone::Clone for Binance {
        #[inline]
        fn clone(&self) -> Binance {
            Binance {
                full_key: ::core::clone::Clone::clone(&self.full_key),
                full_secret: ::core::clone::Clone::clone(&self.full_secret),
                read_key: ::core::clone::Clone::clone(&self.read_key),
                read_secret: ::core::clone::Clone::clone(&self.read_secret),
            }
        }
    }
    #[automatically_derived]
    impl ::core::fmt::Debug for Binance {
        #[inline]
        fn fmt(&self, f: &mut ::core::fmt::Formatter) -> ::core::fmt::Result {
            ::core::fmt::Formatter::debug_struct_field4_finish(
                f,
                "Binance",
                "full_key",
                &self.full_key,
                "full_secret",
                &self.full_secret,
                "read_key",
                &self.read_key,
                "read_secret",
                &&self.read_secret,
            )
        }
    }
    pub struct RawConfig {
        pub binance: RawBinance,
    }
    #[doc(hidden)]
    #[allow(non_upper_case_globals, unused_attributes, unused_qualifications)]
    const _: () = {
        #[allow(unused_extern_crates, clippy::useless_attribute)]
        extern crate serde as _serde;
        #[automatically_derived]
        impl<'de> _serde::Deserialize<'de> for RawConfig {
            fn deserialize<__D>(
                __deserializer: __D,
            ) -> _serde::__private::Result<Self, __D::Error>
            where
                __D: _serde::Deserializer<'de>,
            {
                #[allow(non_camel_case_types)]
                #[doc(hidden)]
                enum __Field {
                    __field0,
                    __ignore,
                }
                #[doc(hidden)]
                struct __FieldVisitor;
                impl<'de> _serde::de::Visitor<'de> for __FieldVisitor {
                    type Value = __Field;
                    fn expecting(
                        &self,
                        __formatter: &mut _serde::__private::Formatter,
                    ) -> _serde::__private::fmt::Result {
                        _serde::__private::Formatter::write_str(
                            __formatter,
                            "field identifier",
                        )
                    }
                    fn visit_u64<__E>(
                        self,
                        __value: u64,
                    ) -> _serde::__private::Result<Self::Value, __E>
                    where
                        __E: _serde::de::Error,
                    {
                        match __value {
                            0u64 => _serde::__private::Ok(__Field::__field0),
                            _ => _serde::__private::Ok(__Field::__ignore),
                        }
                    }
                    fn visit_str<__E>(
                        self,
                        __value: &str,
                    ) -> _serde::__private::Result<Self::Value, __E>
                    where
                        __E: _serde::de::Error,
                    {
                        match __value {
                            "binance" => _serde::__private::Ok(__Field::__field0),
                            _ => _serde::__private::Ok(__Field::__ignore),
                        }
                    }
                    fn visit_bytes<__E>(
                        self,
                        __value: &[u8],
                    ) -> _serde::__private::Result<Self::Value, __E>
                    where
                        __E: _serde::de::Error,
                    {
                        match __value {
                            b"binance" => _serde::__private::Ok(__Field::__field0),
                            _ => _serde::__private::Ok(__Field::__ignore),
                        }
                    }
                }
                impl<'de> _serde::Deserialize<'de> for __Field {
                    #[inline]
                    fn deserialize<__D>(
                        __deserializer: __D,
                    ) -> _serde::__private::Result<Self, __D::Error>
                    where
                        __D: _serde::Deserializer<'de>,
                    {
                        _serde::Deserializer::deserialize_identifier(
                            __deserializer,
                            __FieldVisitor,
                        )
                    }
                }
                #[doc(hidden)]
                struct __Visitor<'de> {
                    marker: _serde::__private::PhantomData<RawConfig>,
                    lifetime: _serde::__private::PhantomData<&'de ()>,
                }
                impl<'de> _serde::de::Visitor<'de> for __Visitor<'de> {
                    type Value = RawConfig;
                    fn expecting(
                        &self,
                        __formatter: &mut _serde::__private::Formatter,
                    ) -> _serde::__private::fmt::Result {
                        _serde::__private::Formatter::write_str(
                            __formatter,
                            "struct RawConfig",
                        )
                    }
                    #[inline]
                    fn visit_seq<__A>(
                        self,
                        mut __seq: __A,
                    ) -> _serde::__private::Result<Self::Value, __A::Error>
                    where
                        __A: _serde::de::SeqAccess<'de>,
                    {
                        let __field0 = match _serde::de::SeqAccess::next_element::<
                            RawBinance,
                        >(&mut __seq)? {
                            _serde::__private::Some(__value) => __value,
                            _serde::__private::None => {
                                return _serde::__private::Err(
                                    _serde::de::Error::invalid_length(
                                        0usize,
                                        &"struct RawConfig with 1 element",
                                    ),
                                );
                            }
                        };
                        _serde::__private::Ok(RawConfig { binance: __field0 })
                    }
                    #[inline]
                    fn visit_map<__A>(
                        self,
                        mut __map: __A,
                    ) -> _serde::__private::Result<Self::Value, __A::Error>
                    where
                        __A: _serde::de::MapAccess<'de>,
                    {
                        let mut __field0: _serde::__private::Option<RawBinance> = _serde::__private::None;
                        while let _serde::__private::Some(__key) = _serde::de::MapAccess::next_key::<
                            __Field,
                        >(&mut __map)? {
                            match __key {
                                __Field::__field0 => {
                                    if _serde::__private::Option::is_some(&__field0) {
                                        return _serde::__private::Err(
                                            <__A::Error as _serde::de::Error>::duplicate_field(
                                                "binance",
                                            ),
                                        );
                                    }
                                    __field0 = _serde::__private::Some(
                                        _serde::de::MapAccess::next_value::<RawBinance>(&mut __map)?,
                                    );
                                }
                                _ => {
                                    let _ = _serde::de::MapAccess::next_value::<
                                        _serde::de::IgnoredAny,
                                    >(&mut __map)?;
                                }
                            }
                        }
                        let __field0 = match __field0 {
                            _serde::__private::Some(__field0) => __field0,
                            _serde::__private::None => {
                                _serde::__private::de::missing_field("binance")?
                            }
                        };
                        _serde::__private::Ok(RawConfig { binance: __field0 })
                    }
                }
                #[doc(hidden)]
                const FIELDS: &'static [&'static str] = &["binance"];
                _serde::Deserializer::deserialize_struct(
                    __deserializer,
                    "RawConfig",
                    FIELDS,
                    __Visitor {
                        marker: _serde::__private::PhantomData::<RawConfig>,
                        lifetime: _serde::__private::PhantomData,
                    },
                )
            }
        }
    };
    #[automatically_derived]
    impl ::core::clone::Clone for RawConfig {
        #[inline]
        fn clone(&self) -> RawConfig {
            RawConfig {
                binance: ::core::clone::Clone::clone(&self.binance),
            }
        }
    }
    #[automatically_derived]
    impl ::core::fmt::Debug for RawConfig {
        #[inline]
        fn fmt(&self, f: &mut ::core::fmt::Formatter) -> ::core::fmt::Result {
            ::core::fmt::Formatter::debug_struct_field1_finish(
                f,
                "RawConfig",
                "binance",
                &&self.binance,
            )
        }
    }
    impl RawConfig {
        pub fn process(&self) -> Result<Config> {
            Ok(Config {
                binance: self.binance.process()?,
            })
        }
    }
    pub struct RawBinance {
        pub full_key: PrivateValue,
        pub full_secret: PrivateValue,
        pub read_key: PrivateValue,
        pub read_secret: PrivateValue,
    }
    #[doc(hidden)]
    #[allow(non_upper_case_globals, unused_attributes, unused_qualifications)]
    const _: () = {
        #[allow(unused_extern_crates, clippy::useless_attribute)]
        extern crate serde as _serde;
        #[automatically_derived]
        impl<'de> _serde::Deserialize<'de> for RawBinance {
            fn deserialize<__D>(
                __deserializer: __D,
            ) -> _serde::__private::Result<Self, __D::Error>
            where
                __D: _serde::Deserializer<'de>,
            {
                #[allow(non_camel_case_types)]
                #[doc(hidden)]
                enum __Field {
                    __field0,
                    __field1,
                    __field2,
                    __field3,
                    __ignore,
                }
                #[doc(hidden)]
                struct __FieldVisitor;
                impl<'de> _serde::de::Visitor<'de> for __FieldVisitor {
                    type Value = __Field;
                    fn expecting(
                        &self,
                        __formatter: &mut _serde::__private::Formatter,
                    ) -> _serde::__private::fmt::Result {
                        _serde::__private::Formatter::write_str(
                            __formatter,
                            "field identifier",
                        )
                    }
                    fn visit_u64<__E>(
                        self,
                        __value: u64,
                    ) -> _serde::__private::Result<Self::Value, __E>
                    where
                        __E: _serde::de::Error,
                    {
                        match __value {
                            0u64 => _serde::__private::Ok(__Field::__field0),
                            1u64 => _serde::__private::Ok(__Field::__field1),
                            2u64 => _serde::__private::Ok(__Field::__field2),
                            3u64 => _serde::__private::Ok(__Field::__field3),
                            _ => _serde::__private::Ok(__Field::__ignore),
                        }
                    }
                    fn visit_str<__E>(
                        self,
                        __value: &str,
                    ) -> _serde::__private::Result<Self::Value, __E>
                    where
                        __E: _serde::de::Error,
                    {
                        match __value {
                            "full_key" => _serde::__private::Ok(__Field::__field0),
                            "full_secret" => _serde::__private::Ok(__Field::__field1),
                            "read_key" => _serde::__private::Ok(__Field::__field2),
                            "read_secret" => _serde::__private::Ok(__Field::__field3),
                            _ => _serde::__private::Ok(__Field::__ignore),
                        }
                    }
                    fn visit_bytes<__E>(
                        self,
                        __value: &[u8],
                    ) -> _serde::__private::Result<Self::Value, __E>
                    where
                        __E: _serde::de::Error,
                    {
                        match __value {
                            b"full_key" => _serde::__private::Ok(__Field::__field0),
                            b"full_secret" => _serde::__private::Ok(__Field::__field1),
                            b"read_key" => _serde::__private::Ok(__Field::__field2),
                            b"read_secret" => _serde::__private::Ok(__Field::__field3),
                            _ => _serde::__private::Ok(__Field::__ignore),
                        }
                    }
                }
                impl<'de> _serde::Deserialize<'de> for __Field {
                    #[inline]
                    fn deserialize<__D>(
                        __deserializer: __D,
                    ) -> _serde::__private::Result<Self, __D::Error>
                    where
                        __D: _serde::Deserializer<'de>,
                    {
                        _serde::Deserializer::deserialize_identifier(
                            __deserializer,
                            __FieldVisitor,
                        )
                    }
                }
                #[doc(hidden)]
                struct __Visitor<'de> {
                    marker: _serde::__private::PhantomData<RawBinance>,
                    lifetime: _serde::__private::PhantomData<&'de ()>,
                }
                impl<'de> _serde::de::Visitor<'de> for __Visitor<'de> {
                    type Value = RawBinance;
                    fn expecting(
                        &self,
                        __formatter: &mut _serde::__private::Formatter,
                    ) -> _serde::__private::fmt::Result {
                        _serde::__private::Formatter::write_str(
                            __formatter,
                            "struct RawBinance",
                        )
                    }
                    #[inline]
                    fn visit_seq<__A>(
                        self,
                        mut __seq: __A,
                    ) -> _serde::__private::Result<Self::Value, __A::Error>
                    where
                        __A: _serde::de::SeqAccess<'de>,
                    {
                        let __field0 = match _serde::de::SeqAccess::next_element::<
                            PrivateValue,
                        >(&mut __seq)? {
                            _serde::__private::Some(__value) => __value,
                            _serde::__private::None => {
                                return _serde::__private::Err(
                                    _serde::de::Error::invalid_length(
                                        0usize,
                                        &"struct RawBinance with 4 elements",
                                    ),
                                );
                            }
                        };
                        let __field1 = match _serde::de::SeqAccess::next_element::<
                            PrivateValue,
                        >(&mut __seq)? {
                            _serde::__private::Some(__value) => __value,
                            _serde::__private::None => {
                                return _serde::__private::Err(
                                    _serde::de::Error::invalid_length(
                                        1usize,
                                        &"struct RawBinance with 4 elements",
                                    ),
                                );
                            }
                        };
                        let __field2 = match _serde::de::SeqAccess::next_element::<
                            PrivateValue,
                        >(&mut __seq)? {
                            _serde::__private::Some(__value) => __value,
                            _serde::__private::None => {
                                return _serde::__private::Err(
                                    _serde::de::Error::invalid_length(
                                        2usize,
                                        &"struct RawBinance with 4 elements",
                                    ),
                                );
                            }
                        };
                        let __field3 = match _serde::de::SeqAccess::next_element::<
                            PrivateValue,
                        >(&mut __seq)? {
                            _serde::__private::Some(__value) => __value,
                            _serde::__private::None => {
                                return _serde::__private::Err(
                                    _serde::de::Error::invalid_length(
                                        3usize,
                                        &"struct RawBinance with 4 elements",
                                    ),
                                );
                            }
                        };
                        _serde::__private::Ok(RawBinance {
                            full_key: __field0,
                            full_secret: __field1,
                            read_key: __field2,
                            read_secret: __field3,
                        })
                    }
                    #[inline]
                    fn visit_map<__A>(
                        self,
                        mut __map: __A,
                    ) -> _serde::__private::Result<Self::Value, __A::Error>
                    where
                        __A: _serde::de::MapAccess<'de>,
                    {
                        let mut __field0: _serde::__private::Option<PrivateValue> = _serde::__private::None;
                        let mut __field1: _serde::__private::Option<PrivateValue> = _serde::__private::None;
                        let mut __field2: _serde::__private::Option<PrivateValue> = _serde::__private::None;
                        let mut __field3: _serde::__private::Option<PrivateValue> = _serde::__private::None;
                        while let _serde::__private::Some(__key) = _serde::de::MapAccess::next_key::<
                            __Field,
                        >(&mut __map)? {
                            match __key {
                                __Field::__field0 => {
                                    if _serde::__private::Option::is_some(&__field0) {
                                        return _serde::__private::Err(
                                            <__A::Error as _serde::de::Error>::duplicate_field(
                                                "full_key",
                                            ),
                                        );
                                    }
                                    __field0 = _serde::__private::Some(
                                        _serde::de::MapAccess::next_value::<
                                            PrivateValue,
                                        >(&mut __map)?,
                                    );
                                }
                                __Field::__field1 => {
                                    if _serde::__private::Option::is_some(&__field1) {
                                        return _serde::__private::Err(
                                            <__A::Error as _serde::de::Error>::duplicate_field(
                                                "full_secret",
                                            ),
                                        );
                                    }
                                    __field1 = _serde::__private::Some(
                                        _serde::de::MapAccess::next_value::<
                                            PrivateValue,
                                        >(&mut __map)?,
                                    );
                                }
                                __Field::__field2 => {
                                    if _serde::__private::Option::is_some(&__field2) {
                                        return _serde::__private::Err(
                                            <__A::Error as _serde::de::Error>::duplicate_field(
                                                "read_key",
                                            ),
                                        );
                                    }
                                    __field2 = _serde::__private::Some(
                                        _serde::de::MapAccess::next_value::<
                                            PrivateValue,
                                        >(&mut __map)?,
                                    );
                                }
                                __Field::__field3 => {
                                    if _serde::__private::Option::is_some(&__field3) {
                                        return _serde::__private::Err(
                                            <__A::Error as _serde::de::Error>::duplicate_field(
                                                "read_secret",
                                            ),
                                        );
                                    }
                                    __field3 = _serde::__private::Some(
                                        _serde::de::MapAccess::next_value::<
                                            PrivateValue,
                                        >(&mut __map)?,
                                    );
                                }
                                _ => {
                                    let _ = _serde::de::MapAccess::next_value::<
                                        _serde::de::IgnoredAny,
                                    >(&mut __map)?;
                                }
                            }
                        }
                        let __field0 = match __field0 {
                            _serde::__private::Some(__field0) => __field0,
                            _serde::__private::None => {
                                _serde::__private::de::missing_field("full_key")?
                            }
                        };
                        let __field1 = match __field1 {
                            _serde::__private::Some(__field1) => __field1,
                            _serde::__private::None => {
                                _serde::__private::de::missing_field("full_secret")?
                            }
                        };
                        let __field2 = match __field2 {
                            _serde::__private::Some(__field2) => __field2,
                            _serde::__private::None => {
                                _serde::__private::de::missing_field("read_key")?
                            }
                        };
                        let __field3 = match __field3 {
                            _serde::__private::Some(__field3) => __field3,
                            _serde::__private::None => {
                                _serde::__private::de::missing_field("read_secret")?
                            }
                        };
                        _serde::__private::Ok(RawBinance {
                            full_key: __field0,
                            full_secret: __field1,
                            read_key: __field2,
                            read_secret: __field3,
                        })
                    }
                }
                #[doc(hidden)]
                const FIELDS: &'static [&'static str] = &[
                    "full_key",
                    "full_secret",
                    "read_key",
                    "read_secret",
                ];
                _serde::Deserializer::deserialize_struct(
                    __deserializer,
                    "RawBinance",
                    FIELDS,
                    __Visitor {
                        marker: _serde::__private::PhantomData::<RawBinance>,
                        lifetime: _serde::__private::PhantomData,
                    },
                )
            }
        }
    };
    #[automatically_derived]
    impl ::core::clone::Clone for RawBinance {
        #[inline]
        fn clone(&self) -> RawBinance {
            RawBinance {
                full_key: ::core::clone::Clone::clone(&self.full_key),
                full_secret: ::core::clone::Clone::clone(&self.full_secret),
                read_key: ::core::clone::Clone::clone(&self.read_key),
                read_secret: ::core::clone::Clone::clone(&self.read_secret),
            }
        }
    }
    #[automatically_derived]
    impl ::core::fmt::Debug for RawBinance {
        #[inline]
        fn fmt(&self, f: &mut ::core::fmt::Formatter) -> ::core::fmt::Result {
            ::core::fmt::Formatter::debug_struct_field4_finish(
                f,
                "RawBinance",
                "full_key",
                &self.full_key,
                "full_secret",
                &self.full_secret,
                "read_key",
                &self.read_key,
                "read_secret",
                &&self.read_secret,
            )
        }
    }
    impl RawBinance {
        pub fn process(&self) -> Result<Binance> {
            Ok(Binance {
                full_key: self.full_key.process()?,
                full_secret: self.full_secret.process()?,
                read_key: self.read_key.process()?,
                read_secret: self.read_secret.process()?,
            })
        }
    }
    pub enum PrivateValue {
        String(String),
        Env { env: String },
    }
    #[automatically_derived]
    impl ::core::clone::Clone for PrivateValue {
        #[inline]
        fn clone(&self) -> PrivateValue {
            match self {
                PrivateValue::String(__self_0) => {
                    PrivateValue::String(::core::clone::Clone::clone(__self_0))
                }
                PrivateValue::Env { env: __self_0 } => {
                    PrivateValue::Env {
                        env: ::core::clone::Clone::clone(__self_0),
                    }
                }
            }
        }
    }
    #[automatically_derived]
    impl ::core::fmt::Debug for PrivateValue {
        #[inline]
        fn fmt(&self, f: &mut ::core::fmt::Formatter) -> ::core::fmt::Result {
            match self {
                PrivateValue::String(__self_0) => {
                    ::core::fmt::Formatter::debug_tuple_field1_finish(
                        f,
                        "String",
                        &__self_0,
                    )
                }
                PrivateValue::Env { env: __self_0 } => {
                    ::core::fmt::Formatter::debug_struct_field1_finish(
                        f,
                        "Env",
                        "env",
                        &__self_0,
                    )
                }
            }
        }
    }
    impl PrivateValue {
        pub fn process(&self) -> Result<String> {
            match self {
                PrivateValue::String(s) => Ok(s.clone()),
                PrivateValue::Env { env } => {
                    std::env::var(env)
                        .with_context(|| {
                            let res = ::alloc::fmt::format(
                                format_args!("Environment variable \'{0}\' not found", env),
                            );
                            res
                        })
                }
            }
        }
    }
    impl<'de> Deserialize<'de> for PrivateValue {
        fn deserialize<D>(deserializer: D) -> Result<PrivateValue, D::Error>
        where
            D: Deserializer<'de>,
        {
            struct PrivateValueVisitor;
            impl<'de> Visitor<'de> for PrivateValueVisitor {
                type Value = PrivateValue;
                fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
                    formatter.write_str("a string or a map with a single key 'env'")
                }
                fn visit_str<E>(self, value: &str) -> Result<Self::Value, E>
                where
                    E: de::Error,
                {
                    Ok(PrivateValue::String(value.to_owned()))
                }
                fn visit_map<M>(self, mut access: M) -> Result<Self::Value, M::Error>
                where
                    M: de::MapAccess<'de>,
                {
                    let key: String = access
                        .next_key()?
                        .ok_or_else(|| de::Error::custom("expected a key"))?;
                    if key == "env" {
                        let value: String = access.next_value()?;
                        Ok(PrivateValue::Env { env: value })
                    } else {
                        Err(de::Error::custom("expected key to be 'env'"))
                    }
                }
            }
            deserializer.deserialize_any(PrivateValueVisitor)
        }
    }
}
pub mod exchange_interactions {
    use crate::binance_api;
    use url::Url;
    use crate::config::Config;
    use anyhow::Result;
    use v_utils::trades::Side;
    pub async fn compile_total_balance(config: Config) -> Result<f32> {
        let read_key = config.binance.read_key.clone();
        let read_secret = config.binance.read_secret.clone();
        let mut handlers = Vec::new();
        handlers
            .push(
                binance_api::get_balance(
                    read_key.clone(),
                    read_secret.clone(),
                    Market::BinanceFutures,
                ),
            );
        handlers
            .push(
                binance_api::get_balance(
                    read_key.clone(),
                    read_secret.clone(),
                    Market::BinanceSpot,
                ),
            );
        let mut total_balance = 0.0;
        for handler in handlers {
            let balance = handler.await?;
            total_balance += balance;
        }
        Ok(total_balance)
    }
    pub async fn open_futures_position(
        config: Config,
        symbol: String,
        side: Side,
        usdt_quantity: f32,
    ) -> Result<()> {
        let full_key = config.binance.full_key.clone();
        let full_secret = config.binance.full_secret.clone();
        let current_price_handler = binance_api::futures_price(symbol.clone());
        let quantity_percision_handler = binance_api::futures_quantity_precision(
            symbol.clone(),
        );
        let current_price = current_price_handler.await?;
        let quantity_precision: usize = quantity_percision_handler.await?;
        let coin_quantity = usdt_quantity / current_price;
        let factor = 10_f32.powi(quantity_precision as i32);
        let coin_quantity_adjusted = (coin_quantity * factor).round() / factor;
        let futures_trade = binance_api::post_futures_trade(
                full_key,
                full_secret,
                binance_api::OrderType::Market,
                symbol,
                side,
                coin_quantity_adjusted,
            )
            .await?;
        match &futures_trade {
            tmp => {
                {
                    ::std::io::_eprint(
                        format_args!(
                            "[{0}:{1}] {2} = {3:#?}\n",
                            "src/exchange_interactions.rs",
                            38u32,
                            "&futures_trade",
                            &tmp,
                        ),
                    );
                };
                tmp
            }
        };
        Ok(())
    }
    #[allow(dead_code)]
    pub enum Market {
        BinanceFutures,
        BinanceSpot,
        BinanceMargin,
    }
    #[automatically_derived]
    #[allow(dead_code)]
    impl ::core::clone::Clone for Market {
        #[inline]
        fn clone(&self) -> Market {
            match self {
                Market::BinanceFutures => Market::BinanceFutures,
                Market::BinanceSpot => Market::BinanceSpot,
                Market::BinanceMargin => Market::BinanceMargin,
            }
        }
    }
    #[automatically_derived]
    #[allow(dead_code)]
    impl ::core::fmt::Debug for Market {
        #[inline]
        fn fmt(&self, f: &mut ::core::fmt::Formatter) -> ::core::fmt::Result {
            ::core::fmt::Formatter::write_str(
                f,
                match self {
                    Market::BinanceFutures => "BinanceFutures",
                    Market::BinanceSpot => "BinanceSpot",
                    Market::BinanceMargin => "BinanceMargin",
                },
            )
        }
    }
    impl Market {
        pub fn get_base_url(&self) -> Url {
            match self {
                Market::BinanceFutures => {
                    Url::parse("https://fapi.binance.com/").unwrap()
                }
                Market::BinanceSpot => Url::parse("https://api.binance.com/").unwrap(),
                Market::BinanceMargin => Url::parse("https://api.binance.com/").unwrap(),
            }
        }
    }
}
mod follow {
    use crate::positions::Position;
    use anyhow::{Error, Result};
    use std::collections::HashMap;
    use std::str::FromStr;
    pub enum Protocol {
        TrailingStop(TrailingStop),
        SAR(SAR),
    }
    #[automatically_derived]
    impl ::core::clone::Clone for Protocol {
        #[inline]
        fn clone(&self) -> Protocol {
            match self {
                Protocol::TrailingStop(__self_0) => {
                    Protocol::TrailingStop(::core::clone::Clone::clone(__self_0))
                }
                Protocol::SAR(__self_0) => {
                    Protocol::SAR(::core::clone::Clone::clone(__self_0))
                }
            }
        }
    }
    #[automatically_derived]
    impl ::core::fmt::Debug for Protocol {
        #[inline]
        fn fmt(&self, f: &mut ::core::fmt::Formatter) -> ::core::fmt::Result {
            match self {
                Protocol::TrailingStop(__self_0) => {
                    ::core::fmt::Formatter::debug_tuple_field1_finish(
                        f,
                        "TrailingStop",
                        &__self_0,
                    )
                }
                Protocol::SAR(__self_0) => {
                    ::core::fmt::Formatter::debug_tuple_field1_finish(
                        f,
                        "SAR",
                        &__self_0,
                    )
                }
            }
        }
    }
    impl FromStr for Protocol {
        type Err = anyhow::Error;
        fn from_str(s: &str) -> Result<Self> {
            let mut parts = s.splitn(2, '-');
            let name = parts.next().ok_or_else(|| Error::msg("No protocol name"))?;
            let params = parts
                .next()
                .ok_or_else(|| Error::msg("Missing parameter specifications"))?;
            let protocol: Protocol = match name.to_lowercase().as_str() {
                "trailing" | "trailing_stop" | "ts" => {
                    Protocol::TrailingStop(TrailingStop::from_str(params)?)
                }
                "sar" => Protocol::SAR(SAR::from_str(params)?),
                _ => return Err(Error::msg("Unknown protocol")),
            };
            Ok(protocol)
        }
    }
    pub struct TrailingStop {
        pub percent: f32,
    }
    #[automatically_derived]
    impl ::core::clone::Clone for TrailingStop {
        #[inline]
        fn clone(&self) -> TrailingStop {
            TrailingStop {
                percent: ::core::clone::Clone::clone(&self.percent),
            }
        }
    }
    #[automatically_derived]
    impl ::core::fmt::Debug for TrailingStop {
        #[inline]
        fn fmt(&self, f: &mut ::core::fmt::Formatter) -> ::core::fmt::Result {
            ::core::fmt::Formatter::debug_struct_field1_finish(
                f,
                "TrailingStop",
                "percent",
                &&self.percent,
            )
        }
    }
    impl FromStr for TrailingStop {
        type Err = anyhow::Error;
        fn from_str(s: &str) -> Result<Self> {
            let params: Vec<&str> = s.split('-').collect();
            match (&params.len(), &1) {
                (left_val, right_val) => {
                    if !(*left_val == *right_val) {
                        let kind = ::core::panicking::AssertKind::Eq;
                        ::core::panicking::assert_failed(
                            kind,
                            &*left_val,
                            &*right_val,
                            ::core::option::Option::None,
                        );
                    }
                }
            };
            let (first_char, rest) = params[0].split_at(1);
            match first_char {
                "p" => {
                    let percent = rest.parse::<f32>()?;
                    Ok(TrailingStop { percent })
                }
                _ => Err(Error::msg("Unknown trailing stop parameter")),
            }
        }
    }
    impl TrailingStop {
        pub fn follow(&self, position: &Position) {
            ::core::panicking::panic("not yet implemented")
        }
    }
    pub struct SAR {
        pub start: f32,
        pub increment: f32,
        pub max: f32,
    }
    #[automatically_derived]
    impl ::core::clone::Clone for SAR {
        #[inline]
        fn clone(&self) -> SAR {
            SAR {
                start: ::core::clone::Clone::clone(&self.start),
                increment: ::core::clone::Clone::clone(&self.increment),
                max: ::core::clone::Clone::clone(&self.max),
            }
        }
    }
    #[automatically_derived]
    impl ::core::fmt::Debug for SAR {
        #[inline]
        fn fmt(&self, f: &mut ::core::fmt::Formatter) -> ::core::fmt::Result {
            ::core::fmt::Formatter::debug_struct_field3_finish(
                f,
                "SAR",
                "start",
                &self.start,
                "increment",
                &self.increment,
                "max",
                &&self.max,
            )
        }
    }
    impl FromStr for SAR {
        type Err = anyhow::Error;
        fn from_str(s: &str) -> Result<Self> {
            let mut params: HashMap<&str, f32> = HashMap::new();
            for param in s.split('-') {
                let (name, value) = param.split_at(1);
                if let Ok(val) = value.parse::<f32>() {
                    params.insert(name, val);
                } else {
                    return Err(Error::msg("Invalid parameter value"));
                }
            }
            if let (Some(&start), Some(&increment), Some(&max)) = (
                params.get("s"),
                params.get("i"),
                params.get("m"),
            ) {
                Ok(SAR { start, increment, max })
            } else {
                Err(Error::msg("Missing SAR parameter(s)"))
            }
        }
    }
}
pub mod positions {
    use crate::exchange_interactions::Market;
    use crate::follow::Protocol;
    use std::collections::HashMap;
    use v_utils::trades::Side;
    pub struct Positions {
        positions: Vec<Position>,
        unaccounted: HashMap<String, f32>,
    }
    pub struct Position {
        market: Market,
        side: Side,
        qty_notional: f32,
        qty_usdt: f32,
        follow: Option<Protocol>,
    }
    #[automatically_derived]
    impl ::core::clone::Clone for Position {
        #[inline]
        fn clone(&self) -> Position {
            Position {
                market: ::core::clone::Clone::clone(&self.market),
                side: ::core::clone::Clone::clone(&self.side),
                qty_notional: ::core::clone::Clone::clone(&self.qty_notional),
                qty_usdt: ::core::clone::Clone::clone(&self.qty_usdt),
                follow: ::core::clone::Clone::clone(&self.follow),
            }
        }
    }
    #[automatically_derived]
    impl ::core::fmt::Debug for Position {
        #[inline]
        fn fmt(&self, f: &mut ::core::fmt::Formatter) -> ::core::fmt::Result {
            ::core::fmt::Formatter::debug_struct_field5_finish(
                f,
                "Position",
                "market",
                &self.market,
                "side",
                &self.side,
                "qty_notional",
                &self.qty_notional,
                "qty_usdt",
                &self.qty_usdt,
                "follow",
                &&self.follow,
            )
        }
    }
}
use clap::{Args, Parser, Subcommand};
use config::Config;
use follow::Protocol;
use v_utils::{expanded_path::ExpandedPath, trades::{Side, Timeframe}};
#[command(author, version, about, long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
    #[arg(long, default_value = "~/.config/discretionary_engine.toml")]
    config: ExpandedPath,
}
#[automatically_derived]
#[allow(unused_qualifications, clippy::redundant_locals)]
impl clap::Parser for Cli {}
#[allow(
    dead_code,
    unreachable_code,
    unused_variables,
    unused_braces,
    unused_qualifications,
)]
#[allow(
    clippy::style,
    clippy::complexity,
    clippy::pedantic,
    clippy::restriction,
    clippy::perf,
    clippy::deprecated,
    clippy::nursery,
    clippy::cargo,
    clippy::suspicious_else_formatting,
    clippy::almost_swapped,
    clippy::redundant_locals,
)]
#[automatically_derived]
impl clap::CommandFactory for Cli {
    fn command<'b>() -> clap::Command {
        let __clap_app = clap::Command::new("discretionary_engine");
        <Self as clap::Args>::augment_args(__clap_app)
    }
    fn command_for_update<'b>() -> clap::Command {
        let __clap_app = clap::Command::new("discretionary_engine");
        <Self as clap::Args>::augment_args_for_update(__clap_app)
    }
}
#[allow(
    dead_code,
    unreachable_code,
    unused_variables,
    unused_braces,
    unused_qualifications,
)]
#[allow(
    clippy::style,
    clippy::complexity,
    clippy::pedantic,
    clippy::restriction,
    clippy::perf,
    clippy::deprecated,
    clippy::nursery,
    clippy::cargo,
    clippy::suspicious_else_formatting,
    clippy::almost_swapped,
    clippy::redundant_locals,
)]
#[automatically_derived]
impl clap::FromArgMatches for Cli {
    fn from_arg_matches(
        __clap_arg_matches: &clap::ArgMatches,
    ) -> ::std::result::Result<Self, clap::Error> {
        Self::from_arg_matches_mut(&mut __clap_arg_matches.clone())
    }
    fn from_arg_matches_mut(
        __clap_arg_matches: &mut clap::ArgMatches,
    ) -> ::std::result::Result<Self, clap::Error> {
        #![allow(deprecated)]
        let v = Cli {
            command: {
                <Commands as clap::FromArgMatches>::from_arg_matches_mut(
                    __clap_arg_matches,
                )?
            },
            config: __clap_arg_matches
                .remove_one::<ExpandedPath>("config")
                .ok_or_else(|| clap::Error::raw(
                    clap::error::ErrorKind::MissingRequiredArgument,
                    "The following required argument was not provided: config",
                ))?,
        };
        ::std::result::Result::Ok(v)
    }
    fn update_from_arg_matches(
        &mut self,
        __clap_arg_matches: &clap::ArgMatches,
    ) -> ::std::result::Result<(), clap::Error> {
        self.update_from_arg_matches_mut(&mut __clap_arg_matches.clone())
    }
    fn update_from_arg_matches_mut(
        &mut self,
        __clap_arg_matches: &mut clap::ArgMatches,
    ) -> ::std::result::Result<(), clap::Error> {
        #![allow(deprecated)]
        {
            #[allow(non_snake_case)]
            let command = &mut self.command;
            <Commands as clap::FromArgMatches>::update_from_arg_matches_mut(
                command,
                __clap_arg_matches,
            )?;
        }
        if __clap_arg_matches.contains_id("config") {
            #[allow(non_snake_case)]
            let config = &mut self.config;
            *config = __clap_arg_matches
                .remove_one::<ExpandedPath>("config")
                .ok_or_else(|| clap::Error::raw(
                    clap::error::ErrorKind::MissingRequiredArgument,
                    "The following required argument was not provided: config",
                ))?;
        }
        ::std::result::Result::Ok(())
    }
}
#[allow(
    dead_code,
    unreachable_code,
    unused_variables,
    unused_braces,
    unused_qualifications,
)]
#[allow(
    clippy::style,
    clippy::complexity,
    clippy::pedantic,
    clippy::restriction,
    clippy::perf,
    clippy::deprecated,
    clippy::nursery,
    clippy::cargo,
    clippy::suspicious_else_formatting,
    clippy::almost_swapped,
    clippy::redundant_locals,
)]
#[automatically_derived]
impl clap::Args for Cli {
    fn group_id() -> Option<clap::Id> {
        Some(clap::Id::from("Cli"))
    }
    fn augment_args<'b>(__clap_app: clap::Command) -> clap::Command {
        {
            let __clap_app = __clap_app
                .group(
                    clap::ArgGroup::new("Cli")
                        .multiple(true)
                        .args({
                            let members: [clap::Id; 1usize] = [clap::Id::from("config")];
                            members
                        }),
                );
            let __clap_app = <Commands as clap::Subcommand>::augment_subcommands(
                __clap_app,
            );
            let __clap_app = __clap_app
                .subcommand_required(true)
                .arg_required_else_help(true);
            let __clap_app = __clap_app
                .arg({
                    #[allow(deprecated)]
                    let arg = clap::Arg::new("config")
                        .value_name("CONFIG")
                        .required(false && clap::ArgAction::Set.takes_values())
                        .value_parser({
                            use ::clap_builder::builder::via_prelude::*;
                            let auto = ::clap_builder::builder::_AutoValueParser::<
                                ExpandedPath,
                            >::new();
                            (&&&&&&auto).value_parser()
                        })
                        .action(clap::ArgAction::Set);
                    let arg = arg
                        .long("config")
                        .default_value("~/.config/discretionary_engine.toml");
                    let arg = arg;
                    arg
                });
            __clap_app.version("0.1.0").long_about(None)
        }
    }
    fn augment_args_for_update<'b>(__clap_app: clap::Command) -> clap::Command {
        {
            let __clap_app = __clap_app
                .group(
                    clap::ArgGroup::new("Cli")
                        .multiple(true)
                        .args({
                            let members: [clap::Id; 1usize] = [clap::Id::from("config")];
                            members
                        }),
                );
            let __clap_app = <Commands as clap::Subcommand>::augment_subcommands(
                __clap_app,
            );
            let __clap_app = __clap_app
                .subcommand_required(true)
                .arg_required_else_help(true)
                .subcommand_required(false)
                .arg_required_else_help(false);
            let __clap_app = __clap_app
                .arg({
                    #[allow(deprecated)]
                    let arg = clap::Arg::new("config")
                        .value_name("CONFIG")
                        .required(false && clap::ArgAction::Set.takes_values())
                        .value_parser({
                            use ::clap_builder::builder::via_prelude::*;
                            let auto = ::clap_builder::builder::_AutoValueParser::<
                                ExpandedPath,
                            >::new();
                            (&&&&&&auto).value_parser()
                        })
                        .action(clap::ArgAction::Set);
                    let arg = arg
                        .long("config")
                        .default_value("~/.config/discretionary_engine.toml");
                    let arg = arg.required(false);
                    arg
                });
            __clap_app.version("0.1.0").long_about(None)
        }
    }
}
enum Commands {
    /// Start the program
    New(PositionArgs),
}
#[allow(
    dead_code,
    unreachable_code,
    unused_variables,
    unused_braces,
    unused_qualifications,
)]
#[allow(
    clippy::style,
    clippy::complexity,
    clippy::pedantic,
    clippy::restriction,
    clippy::perf,
    clippy::deprecated,
    clippy::nursery,
    clippy::cargo,
    clippy::suspicious_else_formatting,
    clippy::almost_swapped,
    clippy::redundant_locals,
)]
#[automatically_derived]
impl clap::FromArgMatches for Commands {
    fn from_arg_matches(
        __clap_arg_matches: &clap::ArgMatches,
    ) -> ::std::result::Result<Self, clap::Error> {
        Self::from_arg_matches_mut(&mut __clap_arg_matches.clone())
    }
    fn from_arg_matches_mut(
        __clap_arg_matches: &mut clap::ArgMatches,
    ) -> ::std::result::Result<Self, clap::Error> {
        #![allow(deprecated)]
        if let Some((__clap_name, mut __clap_arg_sub_matches)) = __clap_arg_matches
            .remove_subcommand()
        {
            let __clap_arg_matches = &mut __clap_arg_sub_matches;
            if __clap_name == "new" && !__clap_arg_matches.contains_id("") {
                return ::std::result::Result::Ok(
                    Self::New(
                        <PositionArgs as clap::FromArgMatches>::from_arg_matches_mut(
                            __clap_arg_matches,
                        )?,
                    ),
                );
            }
            ::std::result::Result::Err(
                clap::Error::raw(
                    clap::error::ErrorKind::InvalidSubcommand,
                    {
                        let res = ::alloc::fmt::format(
                            format_args!(
                                "The subcommand \'{0}\' wasn\'t recognized",
                                __clap_name,
                            ),
                        );
                        res
                    },
                ),
            )
        } else {
            ::std::result::Result::Err(
                clap::Error::raw(
                    clap::error::ErrorKind::MissingSubcommand,
                    "A subcommand is required but one was not provided.",
                ),
            )
        }
    }
    fn update_from_arg_matches(
        &mut self,
        __clap_arg_matches: &clap::ArgMatches,
    ) -> ::std::result::Result<(), clap::Error> {
        self.update_from_arg_matches_mut(&mut __clap_arg_matches.clone())
    }
    fn update_from_arg_matches_mut<'b>(
        &mut self,
        __clap_arg_matches: &mut clap::ArgMatches,
    ) -> ::std::result::Result<(), clap::Error> {
        #![allow(deprecated)]
        if let Some(__clap_name) = __clap_arg_matches.subcommand_name() {
            match self {
                Self::New(ref mut __clap_arg) if "new" == __clap_name => {
                    let (_, mut __clap_arg_sub_matches) = __clap_arg_matches
                        .remove_subcommand()
                        .unwrap();
                    let __clap_arg_matches = &mut __clap_arg_sub_matches;
                    clap::FromArgMatches::update_from_arg_matches_mut(
                        __clap_arg,
                        __clap_arg_matches,
                    )?
                }
                s => {
                    *s = <Self as clap::FromArgMatches>::from_arg_matches_mut(
                        __clap_arg_matches,
                    )?;
                }
            }
        }
        ::std::result::Result::Ok(())
    }
}
#[allow(
    dead_code,
    unreachable_code,
    unused_variables,
    unused_braces,
    unused_qualifications,
)]
#[allow(
    clippy::style,
    clippy::complexity,
    clippy::pedantic,
    clippy::restriction,
    clippy::perf,
    clippy::deprecated,
    clippy::nursery,
    clippy::cargo,
    clippy::suspicious_else_formatting,
    clippy::almost_swapped,
    clippy::redundant_locals,
)]
#[automatically_derived]
impl clap::Subcommand for Commands {
    fn augment_subcommands<'b>(__clap_app: clap::Command) -> clap::Command {
        let __clap_app = __clap_app;
        let __clap_app = __clap_app
            .subcommand({
                let __clap_subcommand = clap::Command::new("new");
                let __clap_subcommand = __clap_subcommand;
                let __clap_subcommand = {
                    <PositionArgs as clap::Args>::augment_args(__clap_subcommand)
                };
                __clap_subcommand.about("Start the program").long_about(None)
            });
        __clap_app
    }
    fn augment_subcommands_for_update<'b>(__clap_app: clap::Command) -> clap::Command {
        let __clap_app = __clap_app;
        let __clap_app = __clap_app
            .subcommand({
                let __clap_subcommand = clap::Command::new("new");
                let __clap_subcommand = __clap_subcommand;
                let __clap_subcommand = {
                    <PositionArgs as clap::Args>::augment_args_for_update(
                        __clap_subcommand,
                    )
                };
                __clap_subcommand.about("Start the program").long_about(None)
            });
        __clap_app
    }
    fn has_subcommand(__clap_name: &str) -> bool {
        if "new" == __clap_name {
            return true;
        }
        false
    }
}
struct PositionArgs {
    #[arg(long)]
    /// percentage of the total balance to use
    size: f32,
    #[arg(long)]
    /// timeframe, in the format of "1m", "1h", "3M", etc.
    /// determines the target period for which we expect the edge to persist.
    tf: Timeframe,
    #[arg(long)]
    /// full ticker of the futures binance symbol
    symbol: String,
    #[arg(long)]
    /// trail parameters, in the format of "<protocol>-<params>", e.g. "trailing-p0.5". Params consist of their starting letter followed by the value, e.g. "p0.5" for 0.5% offset. If multiple params are required, they are separated by '-'.
    trail: Option<Protocol>,
}
#[allow(
    dead_code,
    unreachable_code,
    unused_variables,
    unused_braces,
    unused_qualifications,
)]
#[allow(
    clippy::style,
    clippy::complexity,
    clippy::pedantic,
    clippy::restriction,
    clippy::perf,
    clippy::deprecated,
    clippy::nursery,
    clippy::cargo,
    clippy::suspicious_else_formatting,
    clippy::almost_swapped,
    clippy::redundant_locals,
)]
#[automatically_derived]
impl clap::FromArgMatches for PositionArgs {
    fn from_arg_matches(
        __clap_arg_matches: &clap::ArgMatches,
    ) -> ::std::result::Result<Self, clap::Error> {
        Self::from_arg_matches_mut(&mut __clap_arg_matches.clone())
    }
    fn from_arg_matches_mut(
        __clap_arg_matches: &mut clap::ArgMatches,
    ) -> ::std::result::Result<Self, clap::Error> {
        #![allow(deprecated)]
        let v = PositionArgs {
            size: __clap_arg_matches
                .remove_one::<f32>("size")
                .ok_or_else(|| clap::Error::raw(
                    clap::error::ErrorKind::MissingRequiredArgument,
                    "The following required argument was not provided: size",
                ))?,
            tf: __clap_arg_matches
                .remove_one::<Timeframe>("tf")
                .ok_or_else(|| clap::Error::raw(
                    clap::error::ErrorKind::MissingRequiredArgument,
                    "The following required argument was not provided: tf",
                ))?,
            symbol: __clap_arg_matches
                .remove_one::<String>("symbol")
                .ok_or_else(|| clap::Error::raw(
                    clap::error::ErrorKind::MissingRequiredArgument,
                    "The following required argument was not provided: symbol",
                ))?,
            trail: __clap_arg_matches.remove_one::<Protocol>("trail"),
        };
        ::std::result::Result::Ok(v)
    }
    fn update_from_arg_matches(
        &mut self,
        __clap_arg_matches: &clap::ArgMatches,
    ) -> ::std::result::Result<(), clap::Error> {
        self.update_from_arg_matches_mut(&mut __clap_arg_matches.clone())
    }
    fn update_from_arg_matches_mut(
        &mut self,
        __clap_arg_matches: &mut clap::ArgMatches,
    ) -> ::std::result::Result<(), clap::Error> {
        #![allow(deprecated)]
        if __clap_arg_matches.contains_id("size") {
            #[allow(non_snake_case)]
            let size = &mut self.size;
            *size = __clap_arg_matches
                .remove_one::<f32>("size")
                .ok_or_else(|| clap::Error::raw(
                    clap::error::ErrorKind::MissingRequiredArgument,
                    "The following required argument was not provided: size",
                ))?;
        }
        if __clap_arg_matches.contains_id("tf") {
            #[allow(non_snake_case)]
            let tf = &mut self.tf;
            *tf = __clap_arg_matches
                .remove_one::<Timeframe>("tf")
                .ok_or_else(|| clap::Error::raw(
                    clap::error::ErrorKind::MissingRequiredArgument,
                    "The following required argument was not provided: tf",
                ))?;
        }
        if __clap_arg_matches.contains_id("symbol") {
            #[allow(non_snake_case)]
            let symbol = &mut self.symbol;
            *symbol = __clap_arg_matches
                .remove_one::<String>("symbol")
                .ok_or_else(|| clap::Error::raw(
                    clap::error::ErrorKind::MissingRequiredArgument,
                    "The following required argument was not provided: symbol",
                ))?;
        }
        if __clap_arg_matches.contains_id("trail") {
            #[allow(non_snake_case)]
            let trail = &mut self.trail;
            *trail = __clap_arg_matches.remove_one::<Protocol>("trail");
        }
        ::std::result::Result::Ok(())
    }
}
#[allow(
    dead_code,
    unreachable_code,
    unused_variables,
    unused_braces,
    unused_qualifications,
)]
#[allow(
    clippy::style,
    clippy::complexity,
    clippy::pedantic,
    clippy::restriction,
    clippy::perf,
    clippy::deprecated,
    clippy::nursery,
    clippy::cargo,
    clippy::suspicious_else_formatting,
    clippy::almost_swapped,
    clippy::redundant_locals,
)]
#[automatically_derived]
impl clap::Args for PositionArgs {
    fn group_id() -> Option<clap::Id> {
        Some(clap::Id::from("PositionArgs"))
    }
    fn augment_args<'b>(__clap_app: clap::Command) -> clap::Command {
        {
            let __clap_app = __clap_app
                .group(
                    clap::ArgGroup::new("PositionArgs")
                        .multiple(true)
                        .args({
                            let members: [clap::Id; 4usize] = [
                                clap::Id::from("size"),
                                clap::Id::from("tf"),
                                clap::Id::from("symbol"),
                                clap::Id::from("trail"),
                            ];
                            members
                        }),
                );
            let __clap_app = __clap_app
                .arg({
                    #[allow(deprecated)]
                    let arg = clap::Arg::new("size")
                        .value_name("SIZE")
                        .required(true && clap::ArgAction::Set.takes_values())
                        .value_parser({
                            use ::clap_builder::builder::via_prelude::*;
                            let auto = ::clap_builder::builder::_AutoValueParser::<
                                f32,
                            >::new();
                            (&&&&&&auto).value_parser()
                        })
                        .action(clap::ArgAction::Set);
                    let arg = arg
                        .help("percentage of the total balance to use")
                        .long_help(None)
                        .long("size");
                    let arg = arg;
                    arg
                });
            let __clap_app = __clap_app
                .arg({
                    #[allow(deprecated)]
                    let arg = clap::Arg::new("tf")
                        .value_name("TF")
                        .required(true && clap::ArgAction::Set.takes_values())
                        .value_parser({
                            use ::clap_builder::builder::via_prelude::*;
                            let auto = ::clap_builder::builder::_AutoValueParser::<
                                Timeframe,
                            >::new();
                            (&&&&&&auto).value_parser()
                        })
                        .action(clap::ArgAction::Set);
                    let arg = arg
                        .help(
                            "timeframe, in the format of \"1m\", \"1h\", \"3M\", etc. determines the target period for which we expect the edge to persist",
                        )
                        .long_help(None)
                        .long("tf");
                    let arg = arg;
                    arg
                });
            let __clap_app = __clap_app
                .arg({
                    #[allow(deprecated)]
                    let arg = clap::Arg::new("symbol")
                        .value_name("SYMBOL")
                        .required(true && clap::ArgAction::Set.takes_values())
                        .value_parser({
                            use ::clap_builder::builder::via_prelude::*;
                            let auto = ::clap_builder::builder::_AutoValueParser::<
                                String,
                            >::new();
                            (&&&&&&auto).value_parser()
                        })
                        .action(clap::ArgAction::Set);
                    let arg = arg
                        .help("full ticker of the futures binance symbol")
                        .long_help(None)
                        .long("symbol");
                    let arg = arg;
                    arg
                });
            let __clap_app = __clap_app
                .arg({
                    #[allow(deprecated)]
                    let arg = clap::Arg::new("trail")
                        .value_name("TRAIL")
                        .value_parser({
                            use ::clap_builder::builder::via_prelude::*;
                            let auto = ::clap_builder::builder::_AutoValueParser::<
                                Protocol,
                            >::new();
                            (&&&&&&auto).value_parser()
                        })
                        .action(clap::ArgAction::Set);
                    let arg = arg
                        .help(
                            "trail parameters, in the format of \"<protocol>-<params>\", e.g. \"trailing-p0.5\". Params consist of their starting letter followed by the value, e.g. \"p0.5\" for 0.5% offset. If multiple params are required, they are separated by '-'",
                        )
                        .long_help(None)
                        .long("trail");
                    let arg = arg;
                    arg
                });
            __clap_app
        }
    }
    fn augment_args_for_update<'b>(__clap_app: clap::Command) -> clap::Command {
        {
            let __clap_app = __clap_app
                .group(
                    clap::ArgGroup::new("PositionArgs")
                        .multiple(true)
                        .args({
                            let members: [clap::Id; 4usize] = [
                                clap::Id::from("size"),
                                clap::Id::from("tf"),
                                clap::Id::from("symbol"),
                                clap::Id::from("trail"),
                            ];
                            members
                        }),
                );
            let __clap_app = __clap_app
                .arg({
                    #[allow(deprecated)]
                    let arg = clap::Arg::new("size")
                        .value_name("SIZE")
                        .required(true && clap::ArgAction::Set.takes_values())
                        .value_parser({
                            use ::clap_builder::builder::via_prelude::*;
                            let auto = ::clap_builder::builder::_AutoValueParser::<
                                f32,
                            >::new();
                            (&&&&&&auto).value_parser()
                        })
                        .action(clap::ArgAction::Set);
                    let arg = arg
                        .help("percentage of the total balance to use")
                        .long_help(None)
                        .long("size");
                    let arg = arg.required(false);
                    arg
                });
            let __clap_app = __clap_app
                .arg({
                    #[allow(deprecated)]
                    let arg = clap::Arg::new("tf")
                        .value_name("TF")
                        .required(true && clap::ArgAction::Set.takes_values())
                        .value_parser({
                            use ::clap_builder::builder::via_prelude::*;
                            let auto = ::clap_builder::builder::_AutoValueParser::<
                                Timeframe,
                            >::new();
                            (&&&&&&auto).value_parser()
                        })
                        .action(clap::ArgAction::Set);
                    let arg = arg
                        .help(
                            "timeframe, in the format of \"1m\", \"1h\", \"3M\", etc. determines the target period for which we expect the edge to persist",
                        )
                        .long_help(None)
                        .long("tf");
                    let arg = arg.required(false);
                    arg
                });
            let __clap_app = __clap_app
                .arg({
                    #[allow(deprecated)]
                    let arg = clap::Arg::new("symbol")
                        .value_name("SYMBOL")
                        .required(true && clap::ArgAction::Set.takes_values())
                        .value_parser({
                            use ::clap_builder::builder::via_prelude::*;
                            let auto = ::clap_builder::builder::_AutoValueParser::<
                                String,
                            >::new();
                            (&&&&&&auto).value_parser()
                        })
                        .action(clap::ArgAction::Set);
                    let arg = arg
                        .help("full ticker of the futures binance symbol")
                        .long_help(None)
                        .long("symbol");
                    let arg = arg.required(false);
                    arg
                });
            let __clap_app = __clap_app
                .arg({
                    #[allow(deprecated)]
                    let arg = clap::Arg::new("trail")
                        .value_name("TRAIL")
                        .value_parser({
                            use ::clap_builder::builder::via_prelude::*;
                            let auto = ::clap_builder::builder::_AutoValueParser::<
                                Protocol,
                            >::new();
                            (&&&&&&auto).value_parser()
                        })
                        .action(clap::ArgAction::Set);
                    let arg = arg
                        .help(
                            "trail parameters, in the format of \"<protocol>-<params>\", e.g. \"trailing-p0.5\". Params consist of their starting letter followed by the value, e.g. \"p0.5\" for 0.5% offset. If multiple params are required, they are separated by '-'",
                        )
                        .long_help(None)
                        .long("trail");
                    let arg = arg.required(false);
                    arg
                });
            __clap_app
        }
    }
}
struct NoArgs {}
#[allow(
    dead_code,
    unreachable_code,
    unused_variables,
    unused_braces,
    unused_qualifications,
)]
#[allow(
    clippy::style,
    clippy::complexity,
    clippy::pedantic,
    clippy::restriction,
    clippy::perf,
    clippy::deprecated,
    clippy::nursery,
    clippy::cargo,
    clippy::suspicious_else_formatting,
    clippy::almost_swapped,
    clippy::redundant_locals,
)]
#[automatically_derived]
impl clap::FromArgMatches for NoArgs {
    fn from_arg_matches(
        __clap_arg_matches: &clap::ArgMatches,
    ) -> ::std::result::Result<Self, clap::Error> {
        Self::from_arg_matches_mut(&mut __clap_arg_matches.clone())
    }
    fn from_arg_matches_mut(
        __clap_arg_matches: &mut clap::ArgMatches,
    ) -> ::std::result::Result<Self, clap::Error> {
        #![allow(deprecated)]
        let v = NoArgs {};
        ::std::result::Result::Ok(v)
    }
    fn update_from_arg_matches(
        &mut self,
        __clap_arg_matches: &clap::ArgMatches,
    ) -> ::std::result::Result<(), clap::Error> {
        self.update_from_arg_matches_mut(&mut __clap_arg_matches.clone())
    }
    fn update_from_arg_matches_mut(
        &mut self,
        __clap_arg_matches: &mut clap::ArgMatches,
    ) -> ::std::result::Result<(), clap::Error> {
        #![allow(deprecated)]
        ::std::result::Result::Ok(())
    }
}
#[allow(
    dead_code,
    unreachable_code,
    unused_variables,
    unused_braces,
    unused_qualifications,
)]
#[allow(
    clippy::style,
    clippy::complexity,
    clippy::pedantic,
    clippy::restriction,
    clippy::perf,
    clippy::deprecated,
    clippy::nursery,
    clippy::cargo,
    clippy::suspicious_else_formatting,
    clippy::almost_swapped,
    clippy::redundant_locals,
)]
#[automatically_derived]
impl clap::Args for NoArgs {
    fn group_id() -> Option<clap::Id> {
        Some(clap::Id::from("NoArgs"))
    }
    fn augment_args<'b>(__clap_app: clap::Command) -> clap::Command {
        {
            let __clap_app = __clap_app
                .group(
                    clap::ArgGroup::new("NoArgs")
                        .multiple(true)
                        .args({
                            let members: [clap::Id; 0usize] = [];
                            members
                        }),
                );
            __clap_app
        }
    }
    fn augment_args_for_update<'b>(__clap_app: clap::Command) -> clap::Command {
        {
            let __clap_app = __clap_app
                .group(
                    clap::ArgGroup::new("NoArgs")
                        .multiple(true)
                        .args({
                            let members: [clap::Id; 0usize] = [];
                            members
                        }),
                );
            __clap_app
        }
    }
}
fn main() {
    let body = async {
        let cli = Cli::parse();
        let config = match Config::try_from(cli.config) {
            Ok(cfg) => cfg,
            Err(e) => {
                {
                    ::std::io::_eprint(format_args!("Loading config failed: {0}\n", e));
                };
                std::process::exit(1);
            }
        };
        match cli.command {
            Commands::New(position_args) => {
                let balance = exchange_interactions::compile_total_balance(
                        config.clone(),
                    )
                    .await
                    .unwrap();
                let (side, target_size) = match position_args.size {
                    s if s > 0.0 => (Side::Buy, s * balance),
                    s if s < 0.0 => (Side::Sell, -s * balance),
                    _ => {
                        {
                            ::std::io::_eprint(format_args!("Size must be non-zero\n"));
                        };
                        std::process::exit(1);
                    }
                };
                let stdin = std::io::stdin();
                {
                    ::std::io::_print(
                        format_args!(
                            "Gonna open a new {0}$ {1} order on {2}. Proceed? [Y/n]\n",
                            target_size,
                            side,
                            position_args.symbol,
                        ),
                    );
                };
                let mut input = String::new();
                stdin.read_line(&mut input).expect("Failed to read line");
                let input = input.trim().to_lowercase();
                if input == "y" {
                    exchange_interactions::open_futures_position(
                            config,
                            position_args.symbol,
                            side,
                            target_size,
                        )
                        .await
                        .unwrap();
                } else {
                    {
                        ::std::io::_print(format_args!("Cancelled.\n"));
                    };
                }
            }
        }
    };
    #[allow(clippy::expect_used, clippy::diverging_sub_expression)]
    {
        return tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()
            .expect("Failed building the Runtime")
            .block_on(body);
    }
}
