//! # HFT: Ultra-Fast High-Frequency Trading System
//!
//! A high-performance trading system built in Rust featuring:
//! - Lock-free order book implementation
//! - Sub-microsecond latency profiling
//! - Multi-threaded event processing
//! - Real-time market data streaming
//! - Advanced risk management

pub mod config;
pub mod metrics;
pub mod types;
pub mod utils;

pub use order_book;
pub use event_processor;
pub use trading_engine;
pub use risk_manager;
pub use market_data;
pub use latency_profiler;

use std::sync::Arc;
use tokio::sync::RwLock;

pub type SharedOrderBook = Arc<RwLock<order_book::OrderBook>>;
pub type Result<T> = std::result::Result<T, Box<dyn std::error::Error + Send + Sync>>;

pub const VERSION: &str = env!("CARGO_PKG_VERSION");
pub const NAME: &str = env!("CARGO_PKG_NAME");

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_version() {
        assert!(!VERSION.trim().is_empty());
    }

    #[test]
    fn test_name() {
        assert_eq!(NAME, "hft");
    }
}