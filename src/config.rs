use crate::utils::ExpandedPath;
use anyhow::{Context, Result};
use serde::de::{self, Deserializer, Visitor};
use serde::Deserialize;
use std::convert::TryFrom;
use std::fmt;

impl TryFrom<ExpandedPath> for Config {
	type Error = anyhow::Error;

	fn try_from(path: ExpandedPath) -> Result<Self> {
		let raw_config_str = std::fs::read_to_string(&path).with_context(|| format!("Failed to read config file at {:?}", path))?;

		let raw_config: RawConfig = toml::from_str(&raw_config_str)
			.with_context(|| "The config file is not correctly formatted TOML\nand/or\n is missing some of the required fields")?;

		let config: Config = raw_config.process()?;

		Ok(config)
	}
}

//-----------------------------------------------------------------------------
// Processed Config
//-----------------------------------------------------------------------------

#[derive(Clone, Debug)]
pub struct Config {
	pub binance: Binance,
}
#[derive(Clone, Debug)]
pub struct Binance {
	pub full_key: String,
	pub full_secret: String,
	pub read_key: String,
	pub read_secret: String,
}

//-----------------------------------------------------------------------------
// Raw Config
//-----------------------------------------------------------------------------

#[derive(Deserialize, Clone, Debug)]
pub struct RawConfig {
	pub binance: RawBinance,
}
impl RawConfig {
	pub fn process(&self) -> Result<Config> {
		Ok(Config {
			binance: self.binance.process()?,
		})
	}
}

#[derive(Deserialize, Clone, Debug)]
pub struct RawBinance {
	pub full_key: PrivateValue,
	pub full_secret: PrivateValue,
	pub read_key: PrivateValue,
	pub read_secret: PrivateValue,
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

#[derive(Clone, Debug)]
pub enum PrivateValue {
	String(String),
	Env { env: String },
}
impl PrivateValue {
	pub fn process(&self) -> Result<String> {
		match self {
			PrivateValue::String(s) => Ok(s.clone()),
			PrivateValue::Env { env } => std::env::var(env).with_context(|| format!("Environment variable '{}' not found", env)),
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
				let key: String = access.next_key()?.ok_or_else(|| de::Error::custom("expected a key"))?;
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
