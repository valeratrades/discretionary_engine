use std::{collections::HashMap, path::PathBuf};

extern crate clap;

pub const EXE_NAME: &str = "discretionary_engine";

use color_eyre::eyre::{Result, eyre};
use secrecy::SecretString;
use v_exchanges::ExchangeName;
use v_utils::{Percent, macros as v_macros, percent::PercentU};

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
	#[settings(flatten)]
	pub risk: Option<RiskConfig>,
}

#[derive(Clone, Debug, v_macros::MyConfigPrimitives)]
pub struct ExchangeConfig {
	pub api_pubkey: String,
	pub api_secret: SecretString,
	#[serde(default)]
	pub passphrase: Option<SecretString>,
}

#[derive(Clone, Debug, Default, v_macros::MyConfigPrimitives, v_macros::SettingsNested)]
pub struct RiskConfig {
	#[settings(flatten)]
	pub size: Option<SizeConfig>,
	pub other_balances: Option<f64>,
}

#[derive(Clone, Debug, Default, v_macros::MyConfigPrimitives, v_macros::SettingsNested)]
pub struct SizeConfig {
	pub default_sl: Percent,
	#[settings(default = "PercentU::new(0.01).unwrap()")]
	pub round_bias: PercentU,
	/// Max risk for A-quality trades. Each tier below divides by e (2.718...)
	pub abs_max_risk: Percent,
	#[settings(flatten)]
	pub risk_layers: Option<RiskLayersConfig>,
}

#[derive(Clone, Debug, Default, v_macros::MyConfigPrimitives, v_macros::SettingsNested)]
pub struct RiskLayersConfig {
	#[settings(default = "true")]
	pub stop_loss_proximity: bool,
	#[settings(default = "false")]
	pub from_phone: bool,
	#[settings(default = "false")]
	pub lost_last_trade: bool,
}

impl AppConfig {
	pub fn get_exchange(&self, exchange: ExchangeName) -> Result<&ExchangeConfig> {
		self.exchanges.get(&exchange.to_string()).ok_or_else(|| eyre!("{exchange} exchange config not found"))
	}
}
