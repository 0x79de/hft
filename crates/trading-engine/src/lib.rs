pub mod engine;
pub mod state;
pub mod config;
pub mod portfolio;

pub use engine::TradingEngine;
pub use state::*;
pub use config::EngineConfig;
pub use portfolio::Portfolio;

pub type Result<T> = anyhow::Result<T>;