use std::path::PathBuf;

extern crate clap;

use color_eyre::eyre::{Context, Result};
use v_utils::macros as v_macros;

fn default_comparison_offset_h() -> u32 {
	24
}

#[derive(Clone, Debug, v_macros::MyConfigPrimitives, v_macros::Settings)]
pub struct AppConfig {
	pub positions_dir: PathBuf,
	#[settings(flatten)]
	pub binance: Binance,
	#[serde(default = "default_comparison_offset_h")]
	pub comparison_offset_h: u32,
}

#[derive(Clone, Debug, v_macros::MyConfigPrimitives, v_macros::SettingsBadlyNested)]
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
