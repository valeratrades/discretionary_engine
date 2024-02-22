use crate::api::{get_positions, Market};
use crate::config::Config;
use crate::protocols::*;
use anyhow::Result;
use atomic_float::AtomicF64;
use chrono::{DateTime, Utc};
use std::collections::HashMap;
use std::sync::atomic::Ordering;
use std::sync::{Arc, Mutex};
use v_utils::trades::Side;

/// What the Position _*is*_
pub struct PositionSpec {
	pub asset: String,
	pub side: Side,
	pub size_usdt: f64,
}

pub enum Position {
	Spec(PositionSpec),
	Acquisition(PositionAcquisition),
	Followup(PositionFollowup),
	Closed(PositionClosed),
}
impl Position {
	pub fn new(asset: String, side: Side, size_usdt: f64) -> Self {
		Self::Spec(PositionSpec { asset, side, size_usdt })
	}

	pub async fn execute(&mut self) -> Result<Self::Closed> {
		match self {
			Self::Spec(spec) => {
				let mut acquisition = PositionAcquisition {
					_previous: spec.take(),
					target_notional: todo!(),
					acquired_notional: 0.0,
					protocols_spec: acquisition_protocols_spec.into(),
					cache: Arc::new(Mutex::new(HashMap::new())),
				};
				acquisition.execute().await
			}
			Self::Acquisition(acquisition) => {
				todo!()
			}
			Self::Followup(followup) => {
				todo!()
			}
			Self::Closed(closed) => Ok(closed),
		}
	}

	pub fn size_usdt(self) -> f64 {
		match self {
			Self::Spec(spec) => 0.0,
			Self::Acquisition(acquisition) => acquisition._previous.size_usdt * acquisition.target_notional / acquisition.acquired_notional,
			Self::Followup(followup) => followup._previous._previous.size_usdt * followup._previous.closed_notional / followup._previous.target_notional,
			Self::Closed(closed) => 0.0,
		}
	}
}

pub struct PositionAcquisition {
	_previous: PositionSpec,
	target_notional: f64,
	acquired_notional: f64,
	protocols_spec: AcquisitionProtocolsSpec,
	cache: AcquisitionCache,
}

pub struct PositionFollowup {
	_previous: PositionAcquisition,
	protocols_spec: FollowupProtocolsSpec,
	cache: FollowupCache,
}

pub struct PositionClosed {
	_previous: PositionFollowup,
	t_closed: DateTime<Utc>,
}
