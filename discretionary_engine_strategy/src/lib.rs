//! Strategy execution engine for discretionary_engine.
//!
//! # Architecture
//!
//! This crate follows the Nautilus Trader architecture principles:
//!
//! - **Data Layer** (`data/`): Exchange-specific adapters that normalize data into Nautilus types.
//!   Only this layer knows about specific exchanges (Bybit, Binance, etc.).
//!
//! - **Strategy Layer** (`strategy.rs`): Exchange-agnostic strategy implementations that only
//!   work with Nautilus types (`TradeTick`, `QuoteTick`, `Bar`, etc.).
//!
//! This separation ensures:
//! 1. Strategies can be easily tested with mock data
//! 2. Strategies can switch between exchanges without code changes
//! 3. The same strategy code works for backtesting and live trading

pub mod data;
pub mod strategy;

mod start;

use clap as _;
pub use start::start;
use tracing_subscriber as _;
