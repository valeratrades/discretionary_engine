use clap::{Args, Parser, Subcommand};
use color_eyre::eyre::Result;
use discretionary_engine_strategy::protocols::interpret_protocol_specs;
use futures_util as _;
use nautilus_bybit as _;
use nautilus_model as _;
use tracing::level_filters::LevelFilter;

#[derive(Parser)]
#[command(author, version, about, long_about = None)]
struct Cli {
	#[command(subcommand)]
	command: Commands,
	/// Use testnet instead of mainnet
	#[arg(long, global = true)]
	testnet: bool,
}

#[derive(Subcommand)]
enum Commands {
	/// Submit a position request
	Submit(PositionArgs),
	/// Start the strategy with live market data (legacy)
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

#[tokio::main]
async fn main() -> Result<()> {
	color_eyre::install()?;

	tracing_subscriber::fmt().with_max_level(LevelFilter::INFO).with_target(false).compact().init();

	let cli = Cli::parse();

	match cli.command {
		Commands::Submit(args) => {
			// Parse and validate protocols (no execution yet)
			let acquisition_protocols = interpret_protocol_specs(args.acquisition_protocols)?;
			let followup_protocols = interpret_protocol_specs(args.followup_protocols)?;

			println!("Position: {} USDT of {}", args.size_usdt, args.coin);
			println!("Testnet: {}", cli.testnet);
			println!();
			println!("Acquisition protocols ({}):", acquisition_protocols.len());
			for (i, protocol) in acquisition_protocols.iter().enumerate() {
				println!("  [{}] {:?} -> {}", i, protocol, protocol.signature());
			}
			println!();
			println!("Followup protocols ({}):", followup_protocols.len());
			for (i, protocol) in followup_protocols.iter().enumerate() {
				println!("  [{}] {:?} -> {}", i, protocol, protocol.signature());
			}
		}
		Commands::Start => discretionary_engine_strategy::start().await?,
	}

	Ok(())
}
