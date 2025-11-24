use color_eyre::eyre::Result;
use v_exchanges::Ticker;
use v_utils::trades::Timeframe;

#[derive(clap::Args, Debug)]
#[command(group(
    clap::ArgGroup::new("size")
        .required(true)
        .multiple(true)
        .args(["size_quote", "size_usd"]),
))]
pub(crate) struct AdjustPosArgs {
	/// Ticker to adjust position for.
	ticker: Ticker,

	/// Size in quote currency.
	#[arg(short = 'q', long)]
	size_quote: Option<f64>,

	/// Size in USD
	#[arg(short = 's', long)]
	size_usd: Option<f64>,

	/// timeframe, in the format of "1m", "1h", "3M", etc.
	/// determines the target period for which we expect the edge to persist.
	#[arg(short, long)]
	tf: Option<Timeframe>,
}

pub(crate) fn main(args: AdjustPosArgs) -> Result<()> {
	todo!();
}
