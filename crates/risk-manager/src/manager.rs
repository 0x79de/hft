use crate::limits::{RiskLimits, RiskLimitType};
use crate::position::{Position, PositionTracker};
use crate::validation::OrderValidator;
use order_book::{Order, Trade, Quantity, Side};
use std::collections::HashMap;
use std::sync::Arc;
use parking_lot::RwLock;
use anyhow::Result;
use chrono::{DateTime, Utc};
use uuid::Uuid;
use tracing::info;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RiskConfig {
    pub enable_position_limits: bool,
    pub enable_pnl_limits: bool,
    pub enable_order_size_limits: bool,
    pub enable_price_validation: bool,
    pub max_symbols: usize,
    pub default_position_limit: Quantity,
    pub default_daily_loss_limit: f64,
    pub max_order_size: Quantity,
    pub price_tolerance_pct: f64,
}

impl Default for RiskConfig {
    fn default() -> Self {
        Self {
            enable_position_limits: true,
            enable_pnl_limits: true,
            enable_order_size_limits: true,
            enable_price_validation: true,
            max_symbols: 1000,
            default_position_limit: Quantity::new(1000.0),
            default_daily_loss_limit: 100_000.0,
            max_order_size: Quantity::new(100.0),
            price_tolerance_pct: 5.0,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RiskMetrics {
    pub total_positions: usize,
    pub total_pnl: f64,
    pub daily_pnl: f64,
    pub max_position_size: Quantity,
    pub violations_count: u64,
    pub last_update: DateTime<Utc>,
}

impl Default for RiskMetrics {
    fn default() -> Self {
        Self {
            total_positions: 0,
            total_pnl: 0.0,
            daily_pnl: 0.0,
            max_position_size: Quantity::ZERO,
            violations_count: 0,
            last_update: Utc::now(),
        }
    }
}

pub struct RiskManager {
    config: RiskConfig,
    limits: Arc<RwLock<HashMap<String, RiskLimits>>>,
    positions: Arc<RwLock<HashMap<String, PositionTracker>>>,
    validator: OrderValidator,
    metrics: Arc<RwLock<RiskMetrics>>,
    daily_pnl: Arc<RwLock<HashMap<Uuid, f64>>>,
}

impl RiskManager {
    #[inline]
    pub fn new() -> Self {
        Self::with_config(RiskConfig::default())
    }
    
    #[inline]
    pub fn with_config(config: RiskConfig) -> Self {
        Self {
            config,
            limits: Arc::new(RwLock::new(HashMap::new())),
            positions: Arc::new(RwLock::new(HashMap::new())),
            validator: OrderValidator::new(),
            metrics: Arc::new(RwLock::new(RiskMetrics::default())),
            daily_pnl: Arc::new(RwLock::new(HashMap::new())),
        }
    }
    
    #[inline]
    pub fn validate_order(&self, order: &Order) -> Result<()> {
        self.validator.validate_order(order)
            .map_err(|e| anyhow::anyhow!("Risk validation failed: {}", e))?;
        
        if self.config.enable_position_limits {
            self.validate_position_limits(order)?;
        }
        
        if self.config.enable_pnl_limits {
            self.validate_pnl_limits(order.client_id)?;
        }
        
        Ok(())
    }
    
    #[inline]
    pub fn process_trade(&self, trade: &Trade) -> Result<()> {
        self.update_positions(trade)?;
        self.update_pnl(trade)?;
        self.update_metrics();
        
        Ok(())
    }
    
    #[inline]
    pub fn add_symbol_limits(&self, symbol: String, limits: RiskLimits) {
        self.limits.write().insert(symbol, limits);
    }
    
    #[inline]
    pub fn get_symbol_limits(&self, symbol: &str) -> Option<RiskLimits> {
        self.limits.read().get(symbol).cloned()
    }
    
    #[inline]
    pub fn get_position(&self, symbol: &str, client_id: Uuid) -> Option<Position> {
        let positions = self.positions.read();
        positions.get(symbol)?.get_position(client_id).cloned()
    }
    
    #[inline]
    pub fn get_all_positions(&self, symbol: &str) -> Option<PositionTracker> {
        self.positions.read().get(symbol).cloned()
    }
    
    #[inline]
    pub fn get_metrics(&self) -> RiskMetrics {
        self.metrics.read().clone()
    }
    
    #[inline]
    pub fn get_daily_pnl(&self, client_id: Uuid) -> f64 {
        self.daily_pnl.read().get(&client_id).copied().unwrap_or(0.0)
    }
    
    #[inline]
    pub fn reset_daily_pnl(&self) {
        self.daily_pnl.write().clear();
        info!("Daily P&L reset for all clients");
    }
    
    #[inline]
    pub fn set_position_limit(&self, symbol: &str, limit: f64) {
        if let Some(limits) = self.limits.write().get_mut(symbol) {
            limits.get_limit_mut(RiskLimitType::PositionSize).max_value = limit;
        }
    }
    
    #[inline]
    pub fn set_daily_pnl_limit(&self, symbol: &str, limit: f64) {
        if let Some(limits) = self.limits.write().get_mut(symbol) {
            limits.get_limit_mut(RiskLimitType::DailyPnL).max_value = limit;
        }
    }
    
    #[inline]
    pub fn check_risk_violations(&self) -> Vec<(String, Vec<RiskLimitType>)> {
        let limits = self.limits.read();
        let mut violations = Vec::new();
        
        for (symbol, symbol_limits) in limits.iter() {
            let symbol_violations = symbol_limits.get_violations();
            if !symbol_violations.is_empty() {
                violations.push((symbol.clone(), symbol_violations));
            }
        }
        
        violations
    }
    
    fn validate_position_limits(&self, order: &Order) -> Result<()> {
        let limits = self.limits.read();
        let positions = self.positions.read();
        
        let symbol_limits = limits.get(&order.symbol).cloned()
            .unwrap_or_else(|| RiskLimits::new(order.symbol.clone()));
        
        let current_position = if let Some(tracker) = positions.get(&order.symbol) {
            tracker.get_position(order.client_id)
                .map(|p| p.quantity)
                .unwrap_or(0.0)
        } else {
            0.0
        };
        
        self.validator.validate_position_impact(
            order,
            current_position,
            symbol_limits.position_limit.max_value,
        ).map_err(|e| anyhow::anyhow!("Position limit validation failed: {}", e))?;
        
        Ok(())
    }
    
    fn validate_pnl_limits(&self, client_id: Uuid) -> Result<()> {
        let daily_pnl = self.get_daily_pnl(client_id);
        
        self.validator.validate_pnl_impact(daily_pnl, self.config.default_daily_loss_limit)
            .map_err(|e| anyhow::anyhow!("P&L limit validation failed: {}", e))?;
        
        Ok(())
    }
    
    fn update_positions(&self, trade: &Trade) -> Result<()> {
        let mut positions = self.positions.write();
        
        let tracker = positions
            .entry(trade.symbol.clone())
            .or_insert_with(|| PositionTracker::new(trade.symbol.clone()));
        
        tracker.update_position_with_trade(trade, trade.buyer_client_id, Side::Buy);
        tracker.update_position_with_trade(trade, trade.seller_client_id, Side::Sell);
        
        Ok(())
    }
    
    fn update_pnl(&self, trade: &Trade) -> Result<()> {
        let positions = self.positions.read();
        let mut daily_pnl = self.daily_pnl.write();
        
        if let Some(tracker) = positions.get(&trade.symbol) {
            if let Some(buyer_position) = tracker.get_position(trade.buyer_client_id) {
                daily_pnl.insert(trade.buyer_client_id, buyer_position.realized_pnl);
            }
            
            if let Some(seller_position) = tracker.get_position(trade.seller_client_id) {
                daily_pnl.insert(trade.seller_client_id, seller_position.realized_pnl);
            }
        }
        
        Ok(())
    }
    
    fn update_metrics(&self) {
        let positions = self.positions.read();
        let mut metrics = self.metrics.write();
        
        metrics.total_positions = positions.values().map(|t| t.get_position_count()).sum();
        metrics.total_pnl = positions.values().map(|t| t.total_pnl).sum();
        metrics.daily_pnl = self.daily_pnl.read().values().sum();
        metrics.max_position_size = Quantity::new(positions.values()
            .map(|t| t.get_max_position_size())
            .fold(0.0, f64::max));
        
        let violations = self.check_risk_violations();
        metrics.violations_count = violations.len() as u64;
        metrics.last_update = Utc::now();
    }
}

impl Default for RiskManager {
    fn default() -> Self {
        Self::new()
    }
}