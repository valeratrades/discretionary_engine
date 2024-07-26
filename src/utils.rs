#![allow(dead_code, unused_imports)]
use rand::{rngs::StdRng, SeedableRng};
use rand_distr::{Distribution, Normal};
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
	let env_filter = EnvFilter::try_from_default_env().unwrap_or(EnvFilter::new("info"));

	// Fucking rust. And No, you can't make this shit with any less duplication, without sacrificing your soul.
	if std::env::var("TEST_LOG").is_ok() {
		let formatting_layer = BunyanFormattingLayer::new("discretionary_engine".into(), std::io::stdout);
		let subscriber = Registry::default()
			.with(env_filter)
			.with(JsonStorageLayer)
			//.with(console_layer)
			.with(formatting_layer);
		set_global_default(subscriber).expect("Failed to set subscriber");
	} else {
		let formatting_layer = BunyanFormattingLayer::new("discretionary_engine".into(), std::io::sink);
		let subscriber = Registry::default()
			.with(env_filter)
			.with(JsonStorageLayer)
			//.with(console_layer)
			.with(formatting_layer);
		set_global_default(subscriber).expect("Failed to set subscriber");
	};

	//let formatting_layer = fmt::layer().json().pretty().with_writer(std::io::stdout);
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
