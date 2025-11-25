use std::{collections::HashMap, path::PathBuf, str::FromStr};

extern crate clap;

use color_eyre::eyre::{Context, Result};
use secrecy::SecretString;
use serde::{Deserialize, Serialize};
use v_exchanges::ExchangeName;
use v_utils::macros as v_macros;

fn default_comparison_offset_h() -> u32 {
	24
}

#[derive(Clone, Debug, v_macros::MyConfigPrimitives, v_macros::Settings)]
pub struct AppConfig {
	pub positions_dir: PathBuf,
	#[serde(default)]
	pub exchanges: HashMap<String, ExchangeConfig>,
	#[serde(default = "default_comparison_offset_h")]
	pub comparison_offset_h: u32,
}

#[derive(Clone, Debug, v_macros::MyConfigPrimitives)]
pub struct ExchangeConfig {
	pub api_pubkey: String,
	pub api_secret: SecretString,
	#[serde(default)]
	pub api_passphrase: Option<SecretString>,
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

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn test_exchange_config_structure() {
		// Test that the config structure is correct
		let config = ExchangeConfig {
			api_pubkey: "test_key".to_string(),
			api_secret: SecretString::from("test_secret".to_string()),
			api_passphrase: None,
		};

		assert_eq!(config.api_pubkey, "test_key");
	}

	#[test]
	fn test_exchange_config_with_passphrase() {
		let config = ExchangeConfig {
			api_pubkey: "test_key".to_string(),
			api_secret: SecretString::from("test_secret".to_string()),
			api_passphrase: Some(SecretString::from("test_passphrase".to_string())),
		};

		assert_eq!(config.api_pubkey, "test_key");
		assert!(config.api_passphrase.is_some());
	}

	#[test]
	fn test_get_exchange() {
		let mut exchanges = HashMap::new();
		exchanges.insert(
			"binance".to_string(),
			ExchangeConfig {
				api_pubkey: "key".to_string(),
				api_secret: SecretString::from("secret_value".to_string()),
				api_passphrase: None,
			},
		);

		let config = AppConfig {
			positions_dir: PathBuf::from("/tmp"),
			exchanges,
			comparison_offset_h: 24,
		};

		let binance = config.get_exchange(ExchangeName::Binance);
		assert!(binance.is_ok());
		assert_eq!(binance.unwrap().api_pubkey, "key");

		let kucoin = config.get_exchange(ExchangeName::Kucoin);
		assert!(kucoin.is_err());
	}
}
