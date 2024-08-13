#![allow(dead_code, unused_imports)]
use anyhow::Result;
use serde::de::DeserializeOwned;
use std::time::Duration;
use tokio::runtime::Runtime;
use tokio::time::sleep;
use tracing::{subscriber::set_global_default, Subscriber};
use tracing_bunyan_formatter::{BunyanFormattingLayer, JsonStorageLayer, Type};
use tracing_log::LogTracer;
use tracing_subscriber::{
	fmt::{self, MakeWriter},
	layer::SubscriberExt,
	EnvFilter, Registry,
};

///# Panics
pub fn init_subscriber() {
	//let console_layer = console_subscriber::spawn();
	//let formatting_layer = BunyanFormattingLayer::new("discretionary_engine".into(), std::io::stdout);
	let setup = |output: fn() -> Box<dyn std::io::Write>| {
		let formatting_layer = fmt::layer().json().pretty().with_writer(output).with_file(true).with_line_number(true);
		let env_filter = EnvFilter::try_from_default_env().unwrap_or(EnvFilter::new("info"));
		let subscriber = Registry::default().with(env_filter).with(JsonStorageLayer).with(formatting_layer);
		set_global_default(subscriber).expect("Failed to set subscriber");
	};

	let output = match std::env::var("TEST_LOG") {
		Ok(_) => || Box::new(std::io::stdout()) as Box<dyn std::io::Write>,
		Err(_) => || Box::new(std::io::sink()) as Box<dyn std::io::Write>,
	};

	setup(output);
}

/// Basically reqwest's `json()`, but prints the target's content on deserialization error.
pub async fn deser_reqwest<T: DeserializeOwned>(r: reqwest::Response) -> Result<T> {
	let text = r.text().await?;

	match serde_json::from_str::<T>(&text) {
		Ok(deserialized) => Ok(deserialized),
		Err(_) => {
			Err(unexpected_response_str(&text))
		}
	}
}

pub fn deser_reqwest_blocking<T: DeserializeOwned>(r: reqwest::blocking::Response) -> Result<T> {
	let text = r.text()?;

	match serde_json::from_str::<T>(&text) {
		Ok(deserialized) => Ok(deserialized),
		Err(_) => {
			Err(unexpected_response_str(&text))
		}
	}
}

pub fn unexpected_response_str(s: &str) -> anyhow::Error {
	let s = match serde_json::from_str::<serde_json::Value>(s) {
		Ok(v) => serde_json::to_string_pretty(&v).unwrap(),
		Err(_) => s.to_owned(),
	};
	anyhow::anyhow!("Unexpected API response:\n{}", s)
}
