use serde::{Deserialize, Serialize};
use chrono::{DateTime, Utc};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[repr(u8)]
pub enum RiskLimitType {
    PositionSize = 0,
    DailyPnL = 1,
    OrderSize = 2,
    PriceDeviation = 3,
    NotionalValue = 4,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RiskLimit {
    pub limit_type: RiskLimitType,
    pub symbol: Option<String>,
    pub max_value: f64,
    pub current_value: f64,
    pub enabled: bool,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl RiskLimit {
    #[inline]
    pub fn new(limit_type: RiskLimitType, max_value: f64, symbol: Option<String>) -> Self {
        let now = Utc::now();
        Self {
            limit_type,
            symbol,
            max_value,
            current_value: 0.0,
            enabled: true,
            created_at: now,
            updated_at: now,
        }
    }
    
    #[inline]
    pub fn update_current_value(&mut self, value: f64) {
        self.current_value = value;
        self.updated_at = Utc::now();
    }
    
    #[inline]
    pub fn is_violated(&self) -> bool {
        self.enabled && self.current_value > self.max_value
    }
    
    #[inline]
    pub fn utilization_pct(&self) -> f64 {
        if self.max_value == 0.0 {
            0.0
        } else {
            (self.current_value / self.max_value) * 100.0
        }
    }
    
    #[inline]
    pub fn remaining_capacity(&self) -> f64 {
        (self.max_value - self.current_value).max(0.0)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RiskLimits {
    pub symbol: String,
    pub position_limit: RiskLimit,
    pub daily_pnl_limit: RiskLimit,
    pub order_size_limit: RiskLimit,
    pub price_deviation_limit: RiskLimit,
    pub notional_limit: RiskLimit,
}

impl RiskLimits {
    #[inline]
    pub fn new(symbol: String) -> Self {
        Self {
            symbol: symbol.clone(),
            position_limit: RiskLimit::new(
                RiskLimitType::PositionSize,
                1000.0,
                Some(symbol.clone()),
            ),
            daily_pnl_limit: RiskLimit::new(
                RiskLimitType::DailyPnL,
                100_000.0,
                Some(symbol.clone()),
            ),
            order_size_limit: RiskLimit::new(
                RiskLimitType::OrderSize,
                100.0,
                Some(symbol.clone()),
            ),
            price_deviation_limit: RiskLimit::new(
                RiskLimitType::PriceDeviation,
                5.0,
                Some(symbol.clone()),
            ),
            notional_limit: RiskLimit::new(
                RiskLimitType::NotionalValue,
                1_000_000.0,
                Some(symbol),
            ),
        }
    }
    
    #[inline]
    pub fn with_custom_limits(
        symbol: String,
        position_limit: f64,
        daily_pnl_limit: f64,
        order_size_limit: f64,
        price_deviation_limit: f64,
        notional_limit: f64,
    ) -> Self {
        Self {
            symbol: symbol.clone(),
            position_limit: RiskLimit::new(
                RiskLimitType::PositionSize,
                position_limit,
                Some(symbol.clone()),
            ),
            daily_pnl_limit: RiskLimit::new(
                RiskLimitType::DailyPnL,
                daily_pnl_limit,
                Some(symbol.clone()),
            ),
            order_size_limit: RiskLimit::new(
                RiskLimitType::OrderSize,
                order_size_limit,
                Some(symbol.clone()),
            ),
            price_deviation_limit: RiskLimit::new(
                RiskLimitType::PriceDeviation,
                price_deviation_limit,
                Some(symbol.clone()),
            ),
            notional_limit: RiskLimit::new(
                RiskLimitType::NotionalValue,
                notional_limit,
                Some(symbol),
            ),
        }
    }
    
    #[inline]
    pub fn get_limit(&self, limit_type: RiskLimitType) -> &RiskLimit {
        match limit_type {
            RiskLimitType::PositionSize => &self.position_limit,
            RiskLimitType::DailyPnL => &self.daily_pnl_limit,
            RiskLimitType::OrderSize => &self.order_size_limit,
            RiskLimitType::PriceDeviation => &self.price_deviation_limit,
            RiskLimitType::NotionalValue => &self.notional_limit,
        }
    }
    
    #[inline]
    pub fn get_limit_mut(&mut self, limit_type: RiskLimitType) -> &mut RiskLimit {
        match limit_type {
            RiskLimitType::PositionSize => &mut self.position_limit,
            RiskLimitType::DailyPnL => &mut self.daily_pnl_limit,
            RiskLimitType::OrderSize => &mut self.order_size_limit,
            RiskLimitType::PriceDeviation => &mut self.price_deviation_limit,
            RiskLimitType::NotionalValue => &mut self.notional_limit,
        }
    }
    
    #[inline]
    pub fn has_violations(&self) -> bool {
        self.position_limit.is_violated()
            || self.daily_pnl_limit.is_violated()
            || self.order_size_limit.is_violated()
            || self.price_deviation_limit.is_violated()
            || self.notional_limit.is_violated()
    }
    
    #[inline]
    pub fn get_violations(&self) -> Vec<RiskLimitType> {
        let mut violations = Vec::new();
        
        if self.position_limit.is_violated() {
            violations.push(RiskLimitType::PositionSize);
        }
        if self.daily_pnl_limit.is_violated() {
            violations.push(RiskLimitType::DailyPnL);
        }
        if self.order_size_limit.is_violated() {
            violations.push(RiskLimitType::OrderSize);
        }
        if self.price_deviation_limit.is_violated() {
            violations.push(RiskLimitType::PriceDeviation);
        }
        if self.notional_limit.is_violated() {
            violations.push(RiskLimitType::NotionalValue);
        }
        
        violations
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_risk_limit_creation() {
        let limit = RiskLimit::new(RiskLimitType::PositionSize, 1000.0, Some("BTCUSD".to_string()));
        
        assert_eq!(limit.limit_type, RiskLimitType::PositionSize);
        assert_eq!(limit.max_value, 1000.0);
        assert_eq!(limit.current_value, 0.0);
        assert!(limit.enabled);
        assert!(!limit.is_violated());
    }

    #[test]
    fn test_risk_limit_violation() {
        let mut limit = RiskLimit::new(RiskLimitType::PositionSize, 100.0, None);
        
        limit.update_current_value(50.0);
        assert!(!limit.is_violated());
        assert_eq!(limit.utilization_pct(), 50.0);
        assert_eq!(limit.remaining_capacity(), 50.0);
        
        limit.update_current_value(150.0);
        assert!(limit.is_violated());
        assert_eq!(limit.utilization_pct(), 150.0);
        assert_eq!(limit.remaining_capacity(), 0.0);
    }

    #[test]
    fn test_risk_limits_creation() {
        let limits = RiskLimits::new("BTCUSD".to_string());
        
        assert_eq!(limits.symbol, "BTCUSD");
        assert!(!limits.has_violations());
        assert!(limits.get_violations().is_empty());
    }

    #[test]
    fn test_risk_limits_with_custom_values() {
        let limits = RiskLimits::with_custom_limits(
            "ETHUSD".to_string(),
            500.0,  // position
            50_000.0,  // daily pnl
            50.0,   // order size
            3.0,    // price deviation
            500_000.0,  // notional
        );
        
        assert_eq!(limits.position_limit.max_value, 500.0);
        assert_eq!(limits.daily_pnl_limit.max_value, 50_000.0);
        assert_eq!(limits.order_size_limit.max_value, 50.0);
    }

    #[test]
    fn test_risk_limits_violations() {
        let mut limits = RiskLimits::new("BTCUSD".to_string());
        
        limits.get_limit_mut(RiskLimitType::PositionSize).update_current_value(1500.0);
        limits.get_limit_mut(RiskLimitType::OrderSize).update_current_value(150.0);
        
        assert!(limits.has_violations());
        let violations = limits.get_violations();
        assert_eq!(violations.len(), 2);
        assert!(violations.contains(&RiskLimitType::PositionSize));
        assert!(violations.contains(&RiskLimitType::OrderSize));
    }
}