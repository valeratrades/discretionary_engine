use anyhow::{Context, Result};
use config::{Conifg, File};
use serde::de::{self, Deserializer, Visitor};
use serde::Deserialize;
use std::convert::TryFrom;
use std::fmt;
use std::path::PathBuf;
use v_utils::{io::ExpandedPath, macros::PrivateValues};

#[derive(Clone, Debug)]
pub struct AppConfig {
	pub positions_dir: PathBuf,
	pub binance: Binance,
}
#[derive(Clone, Debug, PrivateValues)]
pub struct Binance {
	pub full_key: String,
	pub full_secret: String,
	pub read_key: String,
	pub read_secret: String,
}

impl AppConfig {
	pub fn new(path: ExpandedPath) -> Result<Self, ConfigError> {
		let builder = config::Config::builder()
			.set_default("comparison_offset_h", 24)?
			.add_source(File::with_name(&path.to_string()));

		let settings: config::Config = builder.build()?;
		let settings: Self = settings.try_deserialize()?;

		let _ = std::fs::create_dir_all(&settings.positions_dir)
			.with_context(|| format!("Failed to create positions directory at {:?}", config.positions_dir))?;

		Ok(settings)
	}
}
