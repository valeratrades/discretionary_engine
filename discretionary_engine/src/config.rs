use std::path::PathBuf;

extern crate clap;

use color_eyre::eyre::{Context, Result};
use v_utils::{io::ExpandedPath, macros::{MyConfigPrimitives, Settings, SettingsBadlyNested}};

#[derive(Clone, Debug, MyConfigPrimitives, Settings)]
pub struct AppConfig {
	pub positions_dir: PathBuf,
	#[settings(flatten)]
	pub binance: Binance,
}
#[derive(Clone, Debug, MyConfigPrimitives, SettingsBadlyNested)]
pub struct Binance {
	pub full_key: String,
	pub full_secret: String,
	pub read_key: String,
	pub read_secret: String,
}

impl AppConfig {
	pub fn try_build_with_flags(flags: SettingsFlags) -> Result<Self> {
		let settings = Self::try_build(flags)?;
		std::fs::create_dir_all(&settings.positions_dir).wrap_err_with(|| format!("Failed to create positions directory at {:?}", settings.positions_dir))?;
		Ok(settings)
	}
}
