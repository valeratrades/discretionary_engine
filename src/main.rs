pub mod binance_api;
pub mod config;
mod exchange_interactions;
pub mod utils;
use clap::{Args, Parser, Subcommand};
use config::Config;
use utils::ExpandedPath;
use v_utils::klines::Timeframe;

#[derive(Parser)]
#[command(author, version, about, long_about = None)]
struct Cli {
	#[command(subcommand)]
	command: Commands,
	#[arg(long, default_value = "~/.config/discretionary_engine.toml")]
	config: ExpandedPath,
}
#[derive(Subcommand)]
enum Commands {
	/// Start the program
	New(PositionArgs),
}
#[derive(Args)]
struct PositionArgs {
	#[arg(long)]
	/// percentage of the total balance to use
	size: f64,
	#[arg(long)]
	/// timeframe, in the format of "1m", "1h", "3M", etc.
	/// determines the target period for which we expect the edge to persist.
	tf: Timeframe,
}

#[derive(Args)]
struct NoArgs {}

#[tokio::main]
async fn main() {
	let cli = Cli::parse();
	let config = match Config::try_from(cli.config) {
		Ok(cfg) => cfg,
		Err(e) => {
			eprintln!("Loading config failed: {}", e);
			std::process::exit(1);
		}
	};

	match cli.command {
		Commands::New(position_args) => {
			let _balance = exchange_interactions::compile_total_balance(config.clone()).await;
			dbg!(&_balance);

			println!("{}", &position_args.tf);
			dbg!(&position_args.size);
		}
	}
}
