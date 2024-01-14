use crate::utils::ExpandedPath;
use anyhow::{Context, Result};
use serde::Deserialize;
use std::convert::TryFrom;

#[derive(Deserialize, Clone, Debug)]
pub struct Config {
	pub binance: Binance,
}

#[derive(Deserialize, Clone, Debug)]
pub struct Binance {
	pub full_key: String,
	pub full_secret: String,
	pub read_key: String,
	pub read_secret: String,
}

impl TryFrom<ExpandedPath> for Config {
	type Error = anyhow::Error;

	//TODO!!!!: add a comprehension for env variable names instead of full key values
	fn try_from(path: ExpandedPath) -> Result<Self> {
		let config_str = std::fs::read_to_string(&path).with_context(|| format!("Failed to read config file at {:?}", path))?;

		let config: Config = toml::from_str(&config_str)
			.with_context(|| "The config file is not correctly formatted TOML\nand/or\n is missing some of the required fields")?;

		Ok(config)
	}
}
