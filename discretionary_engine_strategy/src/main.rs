use clap::{Parser, Subcommand};
use color_eyre::eyre::Result;
use futures_util as _;
use nautilus_bybit as _;
use nautilus_model as _;
use tracing::level_filters::LevelFilter;

#[derive(Parser)]
#[command(author, version, about, long_about = None)]
struct Cli {
	#[command(subcommand)]
	command: Commands,
}

#[derive(Subcommand)]
enum Commands {
	/// Start listening to BTC trades on Bybit
	Start,
}

#[tokio::main]
async fn main() -> Result<()> {
	color_eyre::install()?;

	tracing_subscriber::fmt().with_max_level(LevelFilter::INFO).with_target(false).compact().init();

	let cli = Cli::parse();

	match cli.command {
		Commands::Start => discretionary_engine_strategy::start().await?,
	}

	Ok(())
}
