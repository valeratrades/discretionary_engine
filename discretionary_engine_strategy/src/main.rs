use clap::{Args, Parser, Subcommand};
use color_eyre::eyre::{Result, WrapErr};
use discretionary_engine_strategy::{protocols::interpret_protocol_specs, redis_bus};
use futures_util as _;
use nautilus_bybit as _;
use nautilus_model as _;
use tracing::{info, level_filters::LevelFilter};

#[derive(Parser)]
#[command(author, version, about, long_about = None)]
struct Cli {
	#[command(subcommand)]
	command: Commands,
	/// Use testnet instead of mainnet
	#[arg(long, global = true)]
	testnet: bool,
	/// Redis port for command communication
	#[arg(long, global = true, default_value = "6379", env = "REDIS_PORT")]
	redis_port: u16,
}

#[derive(Subcommand)]
enum Commands {
	/// Submit a position request (sends to running strategy via Redis)
	Submit(PositionArgs),
	/// Start the strategy and listen for commands via Redis
	Start,
}

#[derive(Args, Clone, Debug)]
struct PositionArgs {
	/// Target change in exposure. So positive for buying, negative for selling.
	#[arg(short, long, allow_hyphen_values = true)]
	size_usdt: f64,
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

/// Reconstruct the CLI string from parsed args.
fn build_cli_string(args: &PositionArgs, testnet: bool) -> String {
	let mut parts = Vec::new();

	if testnet {
		parts.push("--testnet".to_string());
	}

	parts.push("submit".to_string());
	parts.push(format!("-s {}", args.size_usdt));
	parts.push(format!("-c {}", args.coin));

	for proto in &args.acquisition_protocols {
		parts.push(format!("-a {}", proto));
	}
	for proto in &args.followup_protocols {
		parts.push(format!("-f {}", proto));
	}

	parts.join(" ")
}

#[tokio::main]
async fn main() -> Result<()> {
	color_eyre::install()?;

	tracing_subscriber::fmt().with_max_level(LevelFilter::INFO).with_target(false).compact().init();

	let cli = Cli::parse();

	match cli.command {
		Commands::Submit(args) => {
			// Validate protocols first
			let _acquisition_protocols = interpret_protocol_specs(args.acquisition_protocols.clone()).wrap_err("Invalid acquisition protocols")?;
			let _followup_protocols = interpret_protocol_specs(args.followup_protocols.clone()).wrap_err("Invalid followup protocols")?;

			// Build CLI string and publish to Redis
			let cli_string = build_cli_string(&args, cli.testnet);
			println!("Publishing command: {}", cli_string);

			let mut conn = redis_bus::connect(cli.redis_port).await?;
			let id = redis_bus::publish_command(&mut conn, &cli_string).await?;
			println!("Command published with ID: {}", id);
		}
		Commands::Start => {
			info!("Starting strategy, listening for commands on Redis port {}...", cli.redis_port);

			// Generate a unique consumer name
			let consumer_name = format!("strategy-{}", std::process::id());

			let mut conn = redis_bus::connect(cli.redis_port).await?;
			let mut subscriber = redis_bus::subscribe_commands(&mut conn, &consumer_name).await?;

			info!("Listening for commands (Ctrl+C to exit)...");

			loop {
				tokio::select! {
					result = subscriber.next() => {
						match result {
							Ok(Some((id, cmd))) => {
								info!("Received command [{}]: {}", id, cmd);
								// TODO: Parse and forward to Nautilus Actor
								println!("[STRATEGY] Received: {}", cmd);
							}
							Ok(None) => {
								// Timeout, continue waiting
							}
							Err(e) => {
								tracing::error!("Error reading command: {}", e);
							}
						}
					}
					_ = tokio::signal::ctrl_c() => {
						info!("Shutting down...");
						break;
					}
				}
			}
		}
	}

	Ok(())
}
