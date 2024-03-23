mod trailing_stop;
use crate::api::order_types::*;
use crate::positions::PositionSpec;
use anyhow::Result;
//use async_trait::async_trait;
use std::str::FromStr;
use std::sync::{Arc, Mutex};

pub use trailing_stop::TrailingStop;

/// Used when determining sizing or the changes in it, in accordance to the current distribution of rm on types of algorithms.
pub enum ProtocolType {
	Momentum,
	TP,
	SL,
}

pub struct Protocol<T>
where
	T: FollowupProtocol + Clone + Send + Sync + FromStr,
{
	pub spec: T,
	pub orders: Vec<OrderTypeP>,
	pub cache: T::Cache,
}

impl<T> Protocol<T>
where
	T: FollowupProtocol,
{
	async fn build(s: &str, spec: &PositionSpec) -> Result<Self> {
		//TODO!: return Result instead (requires weird trait bounds) \
		let t = match T::from_str(s) {
			Ok(t) => t,
			Err(_) => panic!("Fuck it, errors are too hard"),
		};

		Ok(Self {
			spec: t.clone(),
			orders: Vec::new(),
			cache: T::Cache::build(spec).await?,
		})
	}
}
pub trait FollowupProtocol: Clone + Send + Sync + FromStr + std::fmt::Debug
where
	Self::Cache: ProtocolCache,
{
	type Cache: ProtocolCache;
	async fn attach(&self, orders: Arc<Mutex<Vec<OrderTypeP>>>, cache: Arc<Mutex<Self::Cache>>) -> Result<()>;
	fn subtype(&self) -> ProtocolType;
}

pub trait ProtocolCache {
	async fn build(position_spec: &PositionSpec) -> Result<Self>
	where
		Self: Sized;
}
