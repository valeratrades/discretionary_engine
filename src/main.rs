pub mod api;
pub mod config;
pub mod positions;
mod protocols;
use clap::{Args, Parser, Subcommand};
use config::Config;
use positions::Positions;
use protocols::{Protocols, Cache};
use v_utils::{
	io::{self, ExpandedPath},
	trades::{Side, Timeframe},
};

#[derive(Parser)]
#[command(author, version, about, long_about = None)]
struct Cli {
	#[command(subcommand)]
	command: Commands,
	#[arg(long, default_value = "~/.config/discretionary_engine.toml")]
	config: ExpandedPath,
	#[arg(short, long, action = clap::ArgAction::SetTrue)]
	noconfirm: bool,
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
	tf: Option<Timeframe>,
	#[arg(long)]
	/// full ticker of the futures binance symbol
	symbol: String,
	#[arg(short, long, default_value = "")]
	/// trail parameters, in the format of "<protocol>-<params>", e.g. "trailing-p0.5". Params consist of their starting letter followed by the value, e.g. "p0.5" for 0.5% offset. If multiple params are required, they are separated by '-'.
	protocols: Vec<String>,
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
	let noconfirm = cli.noconfirm;

	let positions = match Positions::read_from_file(&config).await {
		Ok(p) => p,
		Err(e) => {
			eprintln!("Loading positions from positions.lock failed: {}", e);
			std::process::exit(1);
		}
	};

	match cli.command {
		Commands::New(position_args) => {
			//let protocols = Protocols::from(position_args.protocols).unwrap();

			use std::str::FromStr;
			let ts = protocols::TrailingStop::from_str("p0.5").unwrap();
			let protocols = Protocols {
				trailing_stop: Some(ts),
				sar: None,
				tpsl: None,
				leading_crosses: None,
			};

			let cache = Cache::new();

			let balance = api::compile_total_balance(config.clone()).await.unwrap();

			let (side, target_size) = match position_args.size {
				s if s > 0.0 => (Side::Buy, s * balance),
				s if s < 0.0 => (Side::Sell, -s * balance),
				_ => {
					eprintln!("Size must be non-zero");
					std::process::exit(1);
				}
			};

			if noconfirm || io::confirm(&format!("Gonna open a new {}$ {} order on {}", target_size, side, position_args.symbol)) {
				match api::open_futures_position(config, positions, position_args.symbol, side, target_size, protocols).await {
					Ok(_) => println!("Order placed successfully"),
					Err(e) => {
						eprintln!("Order placement failed: {}", e);
						std::process::exit(1);
					}
				}
			}
		}
	}
}
