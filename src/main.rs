pub mod binance_api;
pub mod config;
pub mod exchange_interactions;
mod follow;
pub mod positions;
use clap::{Args, Parser, Subcommand};
use config::Config;
use follow::Protocol;
use v_utils::{
	expanded_path::ExpandedPath,
	trades::{Side, Timeframe},
};

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
	size: f32,
	#[arg(long)]
	/// timeframe, in the format of "1m", "1h", "3M", etc.
	/// determines the target period for which we expect the edge to persist.
	tf: Timeframe,
	#[arg(long)]
	/// full ticker of the futures binance symbol
	symbol: String,
	#[arg(long)]
	/// trail parameters, in the format of "<protocol>-<params>", e.g. "trailing-p0.5". Params consist of their starting letter followed by the value, e.g. "p0.5" for 0.5% offset. If multiple params are required, they are separated by '-'.
	follow: Option<Protocol>,
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
			println!("Protocol: {}", &position_args.follow.unwrap());
			let balance = exchange_interactions::compile_total_balance(config.clone()).await.unwrap();

			let (side, target_size) = match position_args.size {
				s if s > 0.0 => (Side::Buy, s * balance),
				s if s < 0.0 => (Side::Sell, -s * balance),
				_ => {
					eprintln!("Size must be non-zero");
					std::process::exit(1);
				}
			};
			let stdin = std::io::stdin();
			println!("Gonna open a new {}$ {} order on {}. Proceed? [Y/n]", target_size, side, position_args.symbol);
			let mut input = String::new();
			stdin.read_line(&mut input).expect("Failed to read line");
			let input = input.trim().to_lowercase();
			if input == "y" {
				exchange_interactions::open_futures_position(config, position_args.symbol, side, target_size)
					.await
					.unwrap();
			} else {
				println!("Cancelled.");
			}
		}
	}
}
