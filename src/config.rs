//! Configuration management

use serde::{Deserialize, Serialize};
use anyhow::Result;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TradingConfig {
    pub symbols: Vec<String>,
    pub max_orders_per_second: u64,
    pub risk_limits: RiskLimits,
    pub latency_thresholds: LatencyThresholds,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RiskLimits {
    pub max_position_size: u64,
    pub max_order_value: f64,
    pub daily_loss_limit: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LatencyThresholds {
    pub order_processing_ns: u64,
    pub market_data_ns: u64,
    pub trade_execution_ns: u64,
}

impl Default for TradingConfig {
    fn default() -> Self {
        Self {
            symbols: vec!["BTCUSD".to_string(), "ETHUSD".to_string()],
            max_orders_per_second: 1_000_000,
            risk_limits: RiskLimits {
                max_position_size: 1000,
                max_order_value: 100_000.0,
                daily_loss_limit: 10_000.0,
            },
            latency_thresholds: LatencyThresholds {
                order_processing_ns: 1_000,
                market_data_ns: 500,
                trade_execution_ns: 2_000,
            },
        }
    }
}

impl TradingConfig {
    pub fn load_from_file(path: &str) -> Result<Self> {
        let content = std::fs::read_to_string(path)?;
        let config: TradingConfig = toml::from_str(&content)?;
        Ok(config)
    }

    pub fn save_to_file(&self, path: &str) -> Result<()> {
        let content = toml::to_string_pretty(self)?;
        std::fs::write(path, content)?;
        Ok(())
    }
}