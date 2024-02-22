mod trailing_stop;

use crate::api::order_types::*;
use crate::positions::PositionSpec;
use anyhow::Result;
use std::str::FromStr;

/// Used when determining sizing or the changes in it, in accordance to the current distribution of rm on types of algorithms.
pub enum ProtocolType {
	Momentum,
	TP,
	SL,
}

pub struct Protocol<T>
where
	T: FollowupProtocol + Clone + Send + Sync + FromStr,
	T::Err: std::error::Error + Send + Sync + 'static,
{
	pub spec: T,
	pub orders: Vec<OrderTypeP>,
	pub cache: T::Cache,
}

impl<T> Protocol<T>
where
	T: FollowupProtocol + Clone + Send + Sync + FromStr,
	T::Err: std::error::Error + Send + Sync + 'static,
{
	fn build(s: &str, spec: &PositionSpec) -> anyhow::Result<Self> {
		let t = T::from_str(s)?;

		Ok(Self {
			spec: t.clone(),
			orders: Vec::new(),
			cache: T::Cache::build(t, spec),
		})
	}
}
/// Writes directly to the unprotected fields of CacheBlob, using unsafe
pub trait FollowupProtocol: Clone + Send + Sync + FromStr
where
	Self::Err: std::error::Error + Send + Sync + 'static,
{
	type Cache: ProtocolCache;
	async fn attach<T>(&self, orders: &mut Vec<OrderTypeP>, cache: &mut Self::Cache) -> Result<()>;
	fn subtype(&self) -> ProtocolType;
}

pub trait ProtocolCache {
	fn build<T>(spec: T, position_spec: &PositionSpec) -> Self;
}
