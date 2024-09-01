#![allow(clippy::comparison_to_empty)]
#![allow(clippy::get_first)]
#![allow(clippy::len_zero)] // wait, so are the ones in Cargo.toml not enough?
#![feature(trait_alias)]
#![feature(type_changing_struct_update)]

pub mod config;
pub mod exchange_apis;
pub mod positions;
pub mod protocols;
pub mod utils;
use std::sync::Arc;

use clap::{Args, Parser, Subcommand};
use color_eyre::eyre::{bail, Context, Result};
use config::AppConfig;
use exchange_apis::{exchanges::Exchanges, hub, hub::HubRx};
use positions::*;
use tokio::{sync::mpsc, task::JoinSet};
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
	/// Artifacts directory, where logs and other files are stored.
	#[arg(long, default_value = "~/.discretionary_engine")]
	artifacts: ExpandedPath,
}
#[derive(Subcommand)]
enum Commands {
	/// Start the program
	New(PositionArgs),
}
#[derive(Args, Debug, Clone)]
struct PositionArgs {
	/// Target change in exposure. So positive for buying, negative for selling.
	#[arg(long)]
	size_usdt: f64,
	/// timeframe, in the format of "1m", "1h", "3M", etc.
	/// determines the target period for which we expect the edge to persist.
	#[arg(long)]
	tf: Option<Timeframe>,
	/// _only_ the coin name itself. e.g. "BTC" or "ETH". Providing full symbol currently will error on the stage of making price requests for the coin.
	#[arg(long)]
	coin: String,
	/// acquisition protocols parameters, in the format of "<protocol>-<params>", e.g. "ts:p0.5". Params consist of their starting letter followed by the value, e.g. "p0.5" for 0.5% offset. If multiple params are required, they are separated by '-'.
	#[arg(short, long)]
	acquisition_protocols: Vec<String>,
	/// followup protocols parameters, in the format of "<protocol>-<params>", e.g. "ts:p0.5". Params consist of their starting letter followed by the value, e.g. "p0.5" for 0.5% offset. If multiple params are required, they are separated by '-'.
	#[arg(short, long)]
	followup_protocols: Vec<String>,
}

// TODO: change to initializing exchange sockets once, then just have a loop listening on localhost, that accepts new positions or modification requests.

#[tokio::main]
async fn main() -> Result<()> {
	let cli = Cli::parse();
	let config = match AppConfig::new(cli.config) {
		Ok(cfg) => cfg,
		Err(e) => {
			eprintln!("Loading config failed: {}", e);
			std::process::exit(1);
		}
	};
	// ensure the artifacts directory exists, if it doesn't, create it.
	match std::fs::create_dir_all(&cli.artifacts) {
		Ok(_) => {}
		Err(e) => {
			eprintln!("Failed to create artifacts directory: {}", e);
			std::process::exit(1);
		}
	}
	let log_path = match std::env::var("TEST_LOG") {
		Ok(_) => None,
		Err(_) => Some(cli.artifacts.0.join("log").clone().into_boxed_path()),
	};
	utils::init_subscriber(log_path);
	let mut js = JoinSet::new();
	let exchanges_arc = Arc::new(Exchanges::default());
	let tx = hub::init_hub(config.clone(), &mut js, exchanges_arc.clone());

	match cli.command {
		Commands::New(position_args) => match command_new(position_args, config, tx, exchanges_arc).await {
			Ok(_) => {}
			Err(e) => {
				eprintln!("{}", utils::format_eyre_chain_for_user(e));
				std::process::exit(1);
			}
		},
	}

	Ok(())
}

async fn command_new(position_args: PositionArgs, config: AppConfig, tx: mpsc::Sender<HubRx>, exchanges_arc: Arc<Exchanges>) -> Result<()> {
	// Currently here mostly for purposes of checking server connectivity.
	let balance = match Exchanges::compile_total_balance(exchanges_arc.clone(), config.clone()).await {
		Ok(b) => b,
		Err(e) => {
			eprintln!("Failed to get balance: {}", e);
			std::process::exit(1);
		}
	};
	println!("Starting creation of a new position.\nCurrent total available balance: {}\n", balance);

	let (side, target_size) = match position_args.size_usdt {
		s if s > 0.0 => (Side::Buy, s),
		s if s < 0.0 => (Side::Sell, -s),
		_ => {
			bail!("Size must be non-zero");
		}
	};

	let followup_protocols = protocols::interpret_protocol_specs(position_args.followup_protocols).wrap_err("Failed to interpret followup protocols")?;
	let acquisition_protocols = protocols::interpret_protocol_specs(position_args.acquisition_protocols).wrap_err("Failed to interpret acquisition protocols")?;

	let spec = PositionSpec::new(position_args.coin, side, target_size);
	//let acquired = PositionAcquisition::dbg_new(spec).await?;
	let acquired = PositionAcquisition::do_acquisition(spec, acquisition_protocols, tx.clone(), exchanges_arc.clone()).await?;
	let _followed = PositionFollowup::do_followup(acquired, followup_protocols, tx.clone(), exchanges_arc.clone()).await?;

	Ok(())
}
