use tracing::subscriber::set_global_default;
use tracing_bunyan_formatter::{BunyanFormattingLayer, JsonStorageLayer};
use tracing_subscriber::{fmt, layer::SubscriberExt, EnvFilter, Registry};

pub fn init_subscriber() {
	let env_filter = EnvFilter::try_from_default_env().unwrap_or(EnvFilter::new("info"));
	let formatting_layer = BunyanFormattingLayer::new("discretionary_engine".into(), std::io::stdout);

	let subscriber = Registry::default().with(env_filter).with(JsonStorageLayer).with(formatting_layer);
	set_global_default(subscriber).expect("Failed to set subscriber");
}
