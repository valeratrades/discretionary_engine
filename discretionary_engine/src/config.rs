use std::{collections::HashMap, path::PathBuf};

extern crate clap;

use color_eyre::eyre::{Context, Result};
use secrecy::SecretString;
use v_exchanges::ExchangeName;
use v_utils::macros as v_macros;

fn __default_comparison_offset_h() -> u32 {
	24
}
#[derive(Clone, Debug, v_macros::LiveSettings, v_macros::MyConfigPrimitives, v_macros::Settings)]
pub struct AppConfig {
	pub positions_dir: PathBuf,
	#[serde(default)]
	pub exchanges: HashMap<String, ExchangeConfig>,
	#[serde(default = "__default_comparison_offset_h")]
	pub comparison_offset_h: u32,
}

#[derive(Clone, Debug, v_macros::MyConfigPrimitives)]
pub struct ExchangeConfig {
	pub api_pubkey: String,
	pub api_secret: SecretString,
	#[serde(default)]
	pub passphrase: Option<SecretString>,
}

impl AppConfig {
	pub fn try_build_with_flags(flags: SettingsFlags) -> Result<Self> {
		let settings = Self::try_build(flags)?;
		std::fs::create_dir_all(&settings.positions_dir).wrap_err_with(|| format!("Failed to create positions directory at {:?}", settings.positions_dir))?;
		Ok(settings)
	}

	pub fn get_exchange(&self, exchange: ExchangeName) -> Result<&ExchangeConfig> {
		self.exchanges
			.get(&exchange.to_string())
			.ok_or_else(|| color_eyre::eyre::eyre!("{} exchange config not found", exchange))
	}
}
