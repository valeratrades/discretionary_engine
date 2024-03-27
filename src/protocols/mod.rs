mod trailing_stop;
use crate::api::order_types::*;
use crate::positions::PositionSpec;
use anyhow::Result;
use async_trait::async_trait;
use std::any::Any;
//use async_trait::async_trait;
use std::str::FromStr;
use std::sync::{Arc, Mutex, RwLock};

pub use trailing_stop::{TrailingStop, TrailingStopCache};

/// Used when determining sizing or the changes in it, in accordance to the current distribution of rm on types of algorithms.
pub enum ProtocolType {
	Momentum,
	TP,
	SL,
}

pub struct ProtocolHandle<T>
where
	T: FollowupProtocol + Clone + Send + Sync + FromStr,
{
	pub spec: T,
	pub orders: Arc<RwLock<Vec<OrderP>>>,
	pub cache: T::Cache,
}

impl<T> ProtocolHandle<T>
where
	T: FollowupProtocol,
{
	async fn build(protocol: T, spec: &PositionSpec) -> Result<Self> {
		Ok(Self {
			spec: protocol,
			orders: Arc::new(RwLock::new(Vec::new())),
			cache: T::Cache::build(spec).await?,
		})
	}

	async fn run(&mut self) -> Result<()> {
		let protocol = Arc::new(protocol);
		let protocol_clone = Arc::clone(&protocol);

		let cache = runtime.block_on(async { TrailingStopCache::build(&acquired._spec.clone()).await })?;
		let orders = Arc::new(RwLock::new(Vec::new()));
		self.orders = Arc::clone(&orders);
		let orders_clone = Arc::clone(&orders);
		let cache_shared = Arc::new(Mutex::new(cache));

		runtime.spawn(async move {
			if let Some(ts_protocol) = protocol_clone.as_any().downcast_ref::<TrailingStop>() {
				ts_protocol.attach(orders_clone, cache_shared).await.unwrap();
			}
		});
	}

	async fn orders(&self) -> Vec<OrderP> {
		self.orders.read().unwrap().clone()
	}
}

trait ProtocolTrait {
	fn build(protocol: Box<dyn Any>, spec: &PositionSpec) -> Result<Self>
	where
		Self: Sized;
	fn run(&mut self) -> Result<()>;
	fn orders(&self) -> Vec<OrderP>;
}

// want to track for protocol from outside of handle:
// 	- requested_orders
// from within the handle:
// 	- cache

#[async_trait]
pub trait FollowupProtocol: Clone + Send + Sync + FromStr + std::fmt::Debug + Any
where
	Self::Cache: ProtocolCache,
{
	type Item;
	type Cache: ProtocolCache;

	async fn attach(&self, orders: Arc<Mutex<Vec<OrderP>>>, cache: Arc<Mutex<Self::Cache>>) -> Result<()>;
	fn subtype(&self) -> ProtocolType;
	fn as_any(&self) -> &dyn Any;
	fn get_item(&self) -> Self::Item;
}

#[async_trait]
pub trait ProtocolCache {
	async fn build(position_spec: &PositionSpec) -> Result<Self>
	where
		Self: Sized;
}

pub enum FProtocol {
	TrailingStop(TrailingStop),
}

//macro_rules! try_build_protocol {
//    ($spec:expr => $($struct:ty)|+) => {
//        {
//            let mut result: Option<Protocol<_>> = None;
//            $(
//                if result.is_none() {
//                    if let Ok(instance) = <$struct as std::str::FromStr>::from_str($spec) {
//                        result = Some(Protocol::build(instance, your_position_spec_here).await?);
//                    }
//                }
//            )+
//            result.ok_or_else(|| anyhow::Error::msg("Could not convert string to any FollowupProtocol and build Protocol"))
//        }
//    };
//}

pub async fn interpret_followup_spec(position_specs: Vec<String>, position_spec: &PositionSpec) -> Result<Vec<Box<dyn ProtocolTrait>>> {
	//TODO!!!!: implement the rest of the protocols
	let mut handles = Vec::new();
	let mut protocols: Vec<ProtocolHandle<TrailingStop>> = Vec::new();

	for spec in position_specs {
		if let Ok(ts) = TrailingStop::from_str(&spec) {
			let handle = tokio::spawn(async move {
				let protocol = ProtocolHandle::build(ts, position_spec).await;
				protocol
			});
			handles.push(handle);
		} else {
			//return Err(anyhow::Error::msg("Could not convert string to any FollowupProtocol and build Protocol"));
			panic!();
		}
	}

	for handle in handles {
		let r = handle.await?;
		let protocol = r?;
		protocols.push(protocol);
	}

	//let mut protocols = Vec::new();
	//for spec in position_specs {
	//	if let Ok(ts) = TrailingStop::from_str(&spec) {
	//		protocols.push(FProtocol::TrailingStop(ts));
	//	} else {
	//		return Err(anyhow::Error::msg("Could not convert string to any FollowupProtocol and build Protocol"));
	//	}
	//}

	Ok(protocols)
}
