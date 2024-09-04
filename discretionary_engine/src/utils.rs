#![allow(dead_code, unused_imports)]
use std::{fs::File, io::Write, path::Path, sync::Arc, time::Duration};

use color_eyre::eyre::{bail, eyre, Result};
use serde::de::DeserializeOwned;
use tokio::{runtime::Runtime, time::sleep};
use tracing::{instrument, subscriber::set_global_default, Subscriber};
use tracing_bunyan_formatter::{BunyanFormattingLayer, JsonStorageLayer, Type};
use tracing_error::ErrorLayer;
use tracing_log::LogTracer;
use tracing_subscriber::{
	fmt::{self, MakeWriter},
	layer::SubscriberExt,
	prelude::*,
	util::SubscriberInitExt,
	EnvFilter, Registry,
};
//let console_layer = console_subscriber::spawn();
/// # Panics
pub fn init_subscriber(log_path: Option<Box<Path>>) {
	let setup = |make_writer: Box<dyn Fn() -> Box<dyn Write> + Send + Sync>| {
		let formatting_layer = tracing_subscriber::fmt::layer().json().pretty().with_writer(make_writer).with_file(true).with_line_number(true);

		let env_filter = tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or(tracing_subscriber::EnvFilter::new("info"));

		let error_layer = ErrorLayer::default();

		let subscriber = tracing_subscriber::Registry::default().with(env_filter).with(formatting_layer).with(error_layer);

		tracing::subscriber::set_global_default(subscriber).expect("Failed to set subscriber");
	};

	match log_path {
		Some(path) => {
			let path = path.to_owned();

			// Truncate the file before setting up the logger
			{
				let _ = std::fs::OpenOptions::new()
					.create(true)
					.write(true)
					.truncate(true)
					.open(&path)
					.expect("Failed to truncate log file");
			}

			setup(Box::new(move || {
				let file = std::fs::OpenOptions::new().create(true).append(true).open(&path).expect("Failed to open log file");
				Box::new(file) as Box<dyn Write>
			}));
		}
		None => {
			setup(Box::new(|| Box::new(std::io::stdout())));
		}
	};
}

pub fn format_eyre_chain_for_user(e: eyre::Report) -> String {
	let chain = e.chain().rev().collect::<Vec<_>>();
	let mut s = String::new();
	for (i, e) in chain.into_iter().enumerate() {
		if i > 0 {
			s.push('\n');
		}
		s.push_str("-> ");
		s.push_str(&e.to_string());
	}
	s
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
	let report = eyre::Report::msg(s);
	report.wrap_err("Unexpected API response")
}
