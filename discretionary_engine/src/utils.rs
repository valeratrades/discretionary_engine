#![allow(dead_code, unused_imports)]
use std::{
	fs::File,
	io::Write,
	path::Path,
	sync::{atomic::Ordering, Arc},
	time::Duration,
};

use color_eyre::eyre::{bail, eyre, Report, Result, WrapErr};
use function_name::named;
use serde::de::DeserializeOwned;
use serde_json::Value;
use tokio::{runtime::Runtime, time::sleep};
use tracing::{error, instrument, subscriber::set_global_default, warn, Subscriber};
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

use crate::{MAX_CONNECTION_FAILURES, MUT_CURRENT_CONNECTION_FAILURES};

//let console_layer = console_subscriber::spawn();
/// # Panics
pub fn init_subscriber(log_path: Option<Box<Path>>) {
	let setup = |make_writer: Box<dyn Fn() -> Box<dyn Write> + Send + Sync>| {
		//let tokio_console_artifacts_filter = EnvFilter::new("tokio[trace]=off,runtime[trace]=off");
		let formatting_layer = tracing_subscriber::fmt::layer().json().pretty().with_writer(make_writer).with_file(true).with_line_number(true)/*.with_filter(tokio_console_artifacts_filter)*/;

		let env_filter = tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or(tracing_subscriber::EnvFilter::new("info"));
		//let env_filter = env_filter
		//      .add_directive("tokio=off".parse().unwrap())
		//      .add_directive("runtime=off".parse().unwrap());

		let error_layer = ErrorLayer::default();

		let console_layer = console_subscriber::spawn::<Registry>(); // does nothing unless `RUST_LOG=tokio=trace,runtime=trace`. But how do I make it not write to file for them?

		tracing_subscriber::registry()
			.with(console_layer)
			.with(env_filter)
			.with(formatting_layer)
			.with(error_layer)
			.init();
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

// Deser Reqwest {{{
fn deser_reqwest_core<T: DeserializeOwned>(text: String) -> Result<T> {
	match serde_json::from_str::<T>(&text) {
		Ok(deserialized) => Ok(deserialized),
		Err(e) => {
			let mut error_msg = e.to_string(); 
			if let Ok(v) = serde_json::from_str::<Value>(&text) { // serde_json's errors are bad, so if the response is valid JSON at all, can convert it to something else and get Context for the error too
				if let Err(e) = serde_json::from_value::<T>(v) {
					error_msg = e.to_string();
				}
			}
			Err(unexpected_response_str(&text)).wrap_err_with(|| error_msg)
		},
	}
}

/// Tracks the caller; once the max number of failures is reached, formats with all the callers that contributed, then sends a notification with `v_notify`
///
/// # Returns
/// `true` if the max number of failures is reached, `false` otherwise
///
/// # Dependencies
/// [v_notify](<https://crates.io/crates/v_notify>) locally installed
//TODO!: print the list of "contributors" to the failure
pub async fn report_connection_problem(e: Report) -> bool {
	let failures = MUT_CURRENT_CONNECTION_FAILURES.fetch_add(1, Ordering::Relaxed);
	warn!("Likely connection problem: {:?}", e);

	if failures + 1 >= MAX_CONNECTION_FAILURES {
		error!("Reached max current connection failures ({MAX_CONNECTION_FAILURES})");

		MUT_CURRENT_CONNECTION_FAILURES.store(0, Ordering::Relaxed);
		return true;
	}

	false
}

/// Basically reqwest's `json()`, but prints the target's content on deserialization error.
pub async fn deser_reqwest<T: DeserializeOwned>(r: reqwest::Response) -> Result<T> {
	let text = r.text().await?;
	deser_reqwest_core(text)
}

/// Blocking [deser_reqwest]
pub fn deser_reqwest_blocking<T: DeserializeOwned>(r: reqwest::blocking::Response) -> Result<T> {
	let text = r.text()?;
	deser_reqwest_core(text)
}

pub fn unexpected_response_str(s: &str) -> eyre::Report {
	let s = match serde_json::from_str::<serde_json::Value>(s) {
		Ok(v) => serde_json::to_string_pretty(&v).unwrap(),
		Err(_) => s.to_owned(),
	};
	let report = report_msg(s);
	report.wrap_err("Unexpected API response")
}
//,}}}

/// Constructs `eyre::Report` with capped size
#[track_caller]
#[named]
pub fn report_msg(s: String) -> eyre::Report {
	let lines: Vec<&str> = s.lines().collect();
	let total_lines = lines.len();

	let truncated_message = if total_lines > 50 {
		let first_25 = &lines[..25];
		let last_25 = &lines[total_lines - 25..];
		let truncation_message = format!("------------------------- // truncated at {} by `{}`\n", std::panic::Location::caller(), function_name!());
		let concat_message = format!("{}\n{truncation_message}{}", first_25.join("\n"), last_25.join("\n"));

		tracing::trace!("Had to concat an error message\n:{s}");

		concat_message
	} else {
		s
	};

	eyre::Report::msg(truncated_message)
}
