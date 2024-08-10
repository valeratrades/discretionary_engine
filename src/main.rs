#![allow(clippy::len_zero)] // wait, so are the ones in Cargo.toml not enough?
#![allow(clippy::get_first)]
#![feature(trait_alias)]
#![feature(type_changing_struct_update)]

pub mod config;
pub mod exchange_apis;
pub mod positions;
pub mod protocols;
pub mod utils;
use clap::{Args, Parser, Subcommand};
use config::AppConfig;
use positions::*;
use tokio::task::JoinSet;
use v_utils::{
	io::ExpandedPath,
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
	/// _only_ the coin name itself. e.g. "BTC" or "ETH". Providing full symbol currently will error on the stage of making price requests for the coin.
	#[arg(long)]
	coin: String,
	/// position acquisition parameters, in the format of "<protocol>-<params>", e.g. "ts:p0.5". Params consist of their starting letter followed by the value, e.g. "p0.5" for 0.5% offset. If multiple params are required, they are separated by '-'.
	#[arg(short, long, default_value = "")]
	acquisition_protocols_spec: Vec<String>,
	/// position followup parameters, in the format of "<protocol>-<params>", e.g. "ts:p0.5". Params consist of their starting letter followed by the value, e.g. "p0.5" for 0.5% offset. If multiple params are required, they are separated by '-'.
	#[arg(short, long)]
	followup_protocols_spec: Vec<String>,
}
// Later on we will initialize exchange sockets once, then just have a loop listening on localhost, that accepts new positions or modification requests.

#[tokio::main]
async fn main() {
	let cli = Cli::parse();
	let config = match AppConfig::new(cli.config) {
		Ok(cfg) => cfg,
		Err(e) => {
			eprintln!("Loading config failed: {}", e);
			std::process::exit(1);
		}
	};
	utils::init_subscriber();
	let mut js = JoinSet::new();
	let tx = exchange_apis::init_hub(config.clone(), &mut js);

	match cli.command {
		Commands::New(position_args) => {
			let balance = match exchange_apis::compile_total_balance(config.clone()).await {
				Ok(b) => b,
				Err(e) => {
					eprintln!("Failed to get balance: {}", e);
					std::process::exit(1);
				}
			};
			let (side, target_size) = match position_args.size {
				s if s > 0.0 => (Side::Buy, s * balance),
				s if s < 0.0 => (Side::Sell, -s * balance),
				_ => {
					eprintln!("Size must be non-zero");
					std::process::exit(1);
				}
			};

			let followup_protocols = protocols::interpret_followup_specs(position_args.followup_protocols_spec).unwrap();

			let spec = PositionSpec::new(position_args.coin, side, target_size);
			//let acquired = PositionAcquisition::dbg_new(spec).await.unwrap();
			let acquired = PositionAcquisition::do_acquisition(spec, &config).await.unwrap();
			let _followed = PositionFollowup::do_followup(acquired, followup_protocols, tx.clone()).await.unwrap();
		}
	}
}
