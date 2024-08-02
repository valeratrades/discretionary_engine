#![allow(dead_code, unused_imports)]
use anyhow::Result;
use rand::{rngs::StdRng, SeedableRng};
use rand_distr::{Distribution, Normal};
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

/// Generates a random walk using Laplace distribution.
///
/// # Arguments
/// - `scale`: Var(laplace) = $2 * scale^2$
/// - `drift`: `mu` parameter, $start + drift$ is point of peak probability density.
pub fn laplace_random_walk(start: f64, num_steps: usize, scale: f64, drift: f64, seed: Option<u64>) -> Vec<f64> {
	let mut rng = match seed {
		Some(s) => StdRng::seed_from_u64(s),
		None => StdRng::from_entropy(),
	};

	let normal = Normal::new(0.0, 1.0).unwrap();

	let steps: Vec<f64> = (0..num_steps)
		.map(|_| {
			let u: f64 = normal.sample(&mut rng);
			let v: f64 = normal.sample(&mut rng);
			drift + scale * (u.abs() - v.abs())
		})
		.collect();

	let walk: Vec<f64> = steps
		.iter()
		.scan(start, |state, &x| {
			*state += x;
			Some(*state)
		})
		.collect();

	std::iter::once(start).chain(walk).collect()
}
/// Basically reqwest's `json()`, but prints the target's content on deserialization error.
pub async fn deser_reqwest<T: DeserializeOwned>(r: reqwest::Response) -> Result<T> {
	let text = r.text().await?;

	match serde_json::from_str::<T>(&text) {
		Ok(deserialized) => Ok(deserialized),
		Err(_) => {
			let s = match serde_json::from_str::<serde_json::Value>(&text) {
				Ok(v) => serde_json::to_string_pretty(&v).unwrap(),
				Err(_) => text,
			};
			Err(anyhow::anyhow!("Unexpected API response:\n{}", s))
		}
	}
}

mod tests {
	use super::*;
	use v_utils::utils::snapshot_plot_p;

	#[test]
	fn test_laplace_random_walk() {
		let start = 100.0;
		let num_steps = 1000;
		let scale = 0.1;
		let drift = 0.0;
		let seed = Some(42);

		let walk = laplace_random_walk(start, num_steps, scale, drift, seed);
		let plot = snapshot_plot_p(&walk, 90, 12);

		insta::assert_snapshot!(plot, @r###"
                                                                      ▂▃▄▃                  
                                                                   ▃  █████▆▁▆▇▄            
                                                                  ▅█▅▆██████████▃       ▃▆▄▄
                                                                ▄▄███████████████▅▅▆▂  ▂████
                                                              ▅▅█████████████████████▅▇█████
                                                             ███████████████████████████████
                     ▂                ▂        ▅▄▁▄         ▁███████████████████████████████
                   ▆██▃▁         ▂▁  ▅█▇▄   ▁ █████▁ ▅    ▃▅████████████████████████████████
  ▂▃  ▃           ▄█████▇     ▆▆▇██▇▆████▆▅▆█▇██████▇█▇ ▂▁██████████████████████████████████
  ██▃▅█▇▆ ▃       ███████▇ ▇█▅█████████████████████████▆████████████████████████████████████
  █████████▇▃ ▁  ▇████████▄█████████████████████████████████████████████████████████████████
  ███████████▇█▇▇███████████████████████████████████████████████████████████████████████████
  "###);
	}
}
