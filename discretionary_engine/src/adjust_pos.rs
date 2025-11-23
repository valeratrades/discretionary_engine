use v_utils::trades::Timeframe;

#[derive(clap::Args, Debug)]
pub(crate) struct AdjustPosArgs {
	/// Target change in exposure. So positive for buying, negative for selling.
	#[arg(short, long)]
	size_usdt: f64,
	/// _only_ the coin name itself. e.g. "BTC" or "ETH". Providing full symbol currently will error on the stage of making price requests for the coin.
	#[arg(short, long)]
	coin: String,
	/// timeframe, in the format of "1m", "1h", "3M", etc.
	/// determines the target period for which we expect the edge to persist.
	#[arg(short, long)]
	tf: Option<Timeframe>,
}
