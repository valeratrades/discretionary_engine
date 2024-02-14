pub mod api;
pub mod config;
pub mod positions;
mod protocols;
use clap::{Args, Parser, Subcommand};
use config::Config;
use positions::Positions;
use protocols::{FollowupCache, Protocols};
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
	/// percentage of the total balance to use
	#[arg(long)]
	size: f64,
	/// timeframe, in the format of "1m", "1h", "3M", etc.
	/// determines the target period for which we expect the edge to persist.
	#[arg(long)]
	tf: Option<Timeframe>,
	/// full ticker of the futures binance symbol
	#[arg(long)]
	symbol: String,
	/// position acquisition parameters, in the format of "<protocol>-<params>", e.g. "ts:p0.5". Params consist of their starting letter followed by the value, e.g. "p0.5" for 0.5% offset. If multiple params are required, they are separated by '-'.
	#[arg(short, long, default_value = "")]
	acquisition_protocols_spec: Vec<String>,
	/// position followup parameters, in the format of "<protocol>-<params>", e.g. "ts:p0.5". Params consist of their starting letter followed by the value, e.g. "p0.5" for 0.5% offset. If multiple params are required, they are separated by '-'.
	#[arg(short, long, default_value = "")]
	followup_protocols_spec: Vec<String>,
}

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
			let protocols = Protocols::try_from(position_args.protocols).unwrap();

			let cache = FollowupCache::new();

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
				match api::open_futures_position(config, &positions, position_args.symbol, side, target_size, protocols).await {
					Ok(_) => {
						println!("Order placed successfully");
						eprintln!("Starting the endless loop");
						loop {
							//positions.sync(config.clone()).await.unwrap();
							//positions.update_orders().await.unwrap();
							tokio::time::sleep(tokio::time::Duration::from_secs(30)).await;
						}
					}
					Err(e) => {
						eprintln!("Order placement failed: {}", e);
						std::process::exit(1);
					}
				}
			}
		}
	}
}
