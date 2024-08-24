#![allow(dead_code, unused_imports)]
use std::{fs::File, io::Write, path::Path, sync::Arc, time::Duration};

use eyre::{bail, eyre, Result};
use serde::de::DeserializeOwned;
use tokio::{runtime::Runtime, time::sleep};
use tracing::{instrument, subscriber::set_global_default, Subscriber};
use tracing_bunyan_formatter::{BunyanFormattingLayer, JsonStorageLayer, Type};
use tracing_log::LogTracer;
use tracing_subscriber::{
	fmt::{self, MakeWriter},
	layer::SubscriberExt,
	prelude::*,
	EnvFilter, Registry,
};
//let console_layer = console_subscriber::spawn();
/// # Panics
pub fn init_subscriber(log_path: Option<Box<Path>>) {
	let setup = |make_writer: Box<dyn Fn() -> Box<dyn Write> + Send + Sync>| {
		let formatting_layer = fmt::layer().json().pretty().with_writer(make_writer).with_file(true).with_line_number(true);
		let env_filter = EnvFilter::try_from_default_env().unwrap_or(EnvFilter::new("info"));
		let subscriber = Registry::default().with(env_filter).with(JsonStorageLayer).with(formatting_layer);
		set_global_default(subscriber).expect("Failed to set subscriber");
	};

	match log_path {
		Some(path) => {
			let path = path.to_owned();
			setup(Box::new(move || Box::new(File::create(&path).expect("Failed to create log file"))));
		}
		None => {
			setup(Box::new(|| Box::new(std::io::stdout())));
		}
	};
}

/// Basically reqwest's `json()`, but prints the target's content on deserialization error.
pub async fn deser_reqwest<T: DeserializeOwned>(r: reqwest::Response) -> Result<T> {
	let text = r.text().await?;

	match serde_json::from_str::<T>(&text) {
		Ok(deserialized) => Ok(deserialized),
		Err(_) => Err(unexpected_response_str(&text)),
	}
}

pub fn deser_reqwest_blocking<T: DeserializeOwned>(r: reqwest::blocking::Response) -> Result<T> {
	let text = r.text()?;

	match serde_json::from_str::<T>(&text) {
		Ok(deserialized) => Ok(deserialized),
		Err(_) => Err(unexpected_response_str(&text)),
	}
}

pub fn unexpected_response_str(s: &str) -> eyre::Report {
	let s = match serde_json::from_str::<serde_json::Value>(s) {
		Ok(v) => serde_json::to_string_pretty(&v).unwrap(),
		Err(_) => s.to_owned(),
	};
	eyre!("Unexpected API response:\n{}", s)
}
