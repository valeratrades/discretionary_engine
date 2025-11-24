use v_exchanges::Ticker;
use v_utils::trades::Timeframe;

#[derive(clap::Args, Debug)]
pub(crate) struct AdjustPosArgs {
	/// Target change in exposure. So positive for buying, negative for selling.
	#[arg(short, long)]
	size_usdt: f64,

	#[arg(short, long)]
	ticker: Ticker,

	/// timeframe, in the format of "1m", "1h", "3M", etc.
	/// determines the target period for which we expect the edge to persist.
	#[arg(short, long)]
	tf: Option<Timeframe>,
}

pub(crate) fn main(args: AdjustPosArgs) -> Result<()> {
	todo!();
}
