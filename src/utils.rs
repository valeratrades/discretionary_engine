#![allow(dead_code, unused_imports)]
use tracing::{subscriber::set_global_default, Subscriber};
use tracing_bunyan_formatter::{BunyanFormattingLayer, JsonStorageLayer, Type};
use tracing_log::LogTracer;
use tracing_subscriber::{
	fmt::{self, MakeWriter},
	layer::SubscriberExt,
	EnvFilter, Registry,
};

pub fn init_subscriber() {
	let env_filter = EnvFilter::try_from_default_env().unwrap_or(EnvFilter::new("info"));

	// Fucking rust. And No, you can't make this shit with any less duplication, without sacrificing your soul.
	if std::env::var("TEST_LOG").is_ok() {
		let formatting_layer = BunyanFormattingLayer::new("discretionary_engine".into(), std::io::stdout);
		let subscriber = Registry::default().with(env_filter).with(JsonStorageLayer).with(formatting_layer);
		set_global_default(subscriber).expect("Failed to set subscriber");
	} else {
		let formatting_layer = BunyanFormattingLayer::new("discretionary_engine".into(), std::io::sink);
		let subscriber = Registry::default().with(env_filter).with(JsonStorageLayer).with(formatting_layer);
		set_global_default(subscriber).expect("Failed to set subscriber");
	};

	//let formatting_layer = fmt::layer().json().pretty().with_writer(std::io::stdout);
}
