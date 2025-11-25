#![allow(clippy::comparison_to_empty)]
#![allow(clippy::get_first)]
#![allow(clippy::len_zero)] // wait, so are the ones in Cargo.toml not enough?
#![feature(trait_alias)]
#![feature(type_changing_struct_update)]
#![feature(stmt_expr_attributes)]

mod adjust_pos;
mod bybit_common;
pub mod config;
pub mod exchange_apis;
mod nuke;
pub mod positions;
pub mod protocols;
pub mod utils;
use std::sync::{Arc, atomic::AtomicU32};

use clap::{Args, Parser, Subcommand};
use color_eyre::eyre::{Context, Result, bail};
use config::{AppConfig, SettingsFlags};
use exchange_apis::{exchanges::Exchanges, hub, hub::PositionToHub};
use positions::*;
use tokio::{sync::mpsc, task::JoinSet};
use tracing::{info, instrument};
use v_utils::{
	io::ExpandedPath,
	trades::{Side, Timeframe},
	utils::exit_on_error,
};

pub static MAX_CONNECTION_FAILURES: u32 = 10;
pub static MUT_CURRENT_CONNECTION_FAILURES: AtomicU32 = AtomicU32::new(0);

#[derive(Parser)]
#[command(author, version, about, long_about = None)]
struct Cli {
	#[command(subcommand)]
	command: Commands,
	#[command(flatten)]
	settings: SettingsFlags,
	#[arg(short, long, action = clap::ArgAction::SetTrue)]
	noconfirm: bool,
	/// Artifacts directory, where logs and other files are stored.
	#[arg(long, default_value = "~/.discretionary_engine")]
	artifacts: ExpandedPath,
	/// Use testnet instead of mainnet
	#[arg(long, global = true)]
	testnet: bool,
}
#[derive(Subcommand)]
enum Commands {
	/// Start the main program
	Run(PositionArgs),
	/// Adjust an existing position size smartly
	AdjustPos(adjust_pos::AdjustPosArgs),
	/// Close position completely
	Nuke(nuke::NukeArgs),
}
#[derive(Args, Clone, Debug)]
struct PositionArgs {
	/// Target change in exposure. So positive for buying, negative for selling.
	#[arg(short, long)]
	size_usdt: f64,
	/// timeframe, in the format of "1m", "1h", "3M", etc.
	/// determines the target period for which we expect the edge to persist.
	#[arg(short, long)]
	tf: Option<Timeframe>,
	/// _only_ the coin name itself. e.g. "BTC" or "ETH". Providing full symbol currently will error on the stage of making price requests for the coin.
	#[arg(short, long)]
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
	color_eyre::install()?;
	let cli = Cli::parse();
	let config = match AppConfig::try_build_with_flags(cli.settings) {
		Ok(cfg) => cfg,
		Err(e) => {
			eprintln!("Loading config failed: {}", e);
			std::process::exit(1);
		}
	};
	let config_arc = Arc::new(config);
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
		Err(_) => Some(cli.artifacts.0.join(".log").clone().into_boxed_path()),
	};
	utils::init_subscriber(log_path);
	let mut js = JoinSet::new();
	let exchanges_arc = Arc::new(
		Exchanges::init(config_arc.clone())
			.await
			.wrap_err_with(|| "Error initializing Exchanges, likely indicative of bad internet connection")?,
	);
	let tx = hub::init_hub(config_arc.clone(), &mut js, exchanges_arc.clone());

	exit_on_error(match cli.command {
		Commands::Run(args) => command_new(args, config_arc.clone(), tx, exchanges_arc).await,
		Commands::AdjustPos(adjust_pos_args) => adjust_pos::main(adjust_pos_args, config_arc.clone(), cli.testnet).await,
		Commands::Nuke(nuke_args) => nuke::main(nuke_args, config_arc.clone(), cli.testnet).await,
	});

	Ok(())
}

#[instrument(skip(config_arc, tx, exchanges_arc))]
async fn command_new(position_args: PositionArgs, config_arc: Arc<AppConfig>, tx: mpsc::Sender<PositionToHub>, exchanges_arc: Arc<Exchanges>) -> Result<()> {
	// Currently here mostly for purposes of checking server connectivity.
	let balance = match Exchanges::compile_total_balance(exchanges_arc.clone(), config_arc.clone()).await {
		Ok(b) => b,
		Err(e) => {
			eprintln!("Failed to get balance: {}", e);
			std::process::exit(1);
		}
	};
	info!("Total balance: {}", balance);
	println!("Current total available balance: {}", balance);

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
