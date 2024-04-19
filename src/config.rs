use anyhow::{anyhow, Context, Result};
use std::path::PathBuf;
use v_utils::{io::ExpandedPath, macros::MyConfigPrimitives};

#[derive(Clone, Debug, MyConfigPrimitives)]
pub struct AppConfig {
	pub positions_dir: PathBuf,
	pub binance: Binance,
}
#[derive(Clone, Debug, MyConfigPrimitives)]
pub struct Binance {
	pub full_key: String,
	pub full_secret: String,
	pub read_key: String,
	pub read_secret: String,
}

impl AppConfig {
	pub fn new(path: ExpandedPath) -> Result<Self> {
		let builder = config::Config::builder()
			.set_default("comparison_offset_h", 24)?
			.add_source(config::File::with_name(&path.to_string()));

		let settings: config::Config = builder.build()?;
		let settings: Self = settings.try_deserialize()?;

		std::fs::create_dir_all(&settings.positions_dir)
			.with_context(|| anyhow!(format!("Failed to create positions directory at {:?}", settings.positions_dir)))?;

		Ok(settings)
	}
}
