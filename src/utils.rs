use tracing::subscriber::set_global_default;
use tracing_bunyan_formatter::JsonStorageLayer;
use tracing_subscriber::{fmt, layer::SubscriberExt, EnvFilter, Registry};

pub fn init_subscriber() {
	let env_filter = EnvFilter::try_from_default_env().unwrap_or(EnvFilter::new("info"));
	//let formatting_layer = BunyanFormattingLayer::new("discretionary_engine".into(), std::io::stdout);
	let formatting_layer = fmt::layer().json().pretty().with_writer(std::io::stdout);

	let subscriber = Registry::default().with(env_filter).with(JsonStorageLayer).with(formatting_layer);
	set_global_default(subscriber).expect("Failed to set subscriber");
}

//pub fn init_subscriber_orion() {
//	let stdout_log = tracing_subscriber::fmt::layer().pretty();
//	let file = File::create("debug.log");
//	let file = match file {
//		Ok(file) => file,
//		Err(error) => panic!("Error: {:?}", error),
//	};
//	let debug_log = tracing_subscriber::fmt::layer().with_writer(std::sync::Arc::new(file));
//
//	tracing_subscriber::registry()
//		.with(stdout_log
//                // Add an `INFO` filter to the stdout logging layer
//                .with_filter(filter::LevelFilter::TRACE)
//                // Combine the filtered `stdout_log` layer with the
//                // `debug_log` layer, producing a new `Layered` layer.
//                .and_then(debug_log))
//		.init();
//}
