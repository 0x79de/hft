use std::collections::HashMap;
use crate::types::MarketContext;
use rust_decimal::Decimal;
use rust_decimal::prelude::ToPrimitive;
use chrono::{Timelike, Datelike};

#[derive(Debug, Clone)]
pub struct FeatureExtractor {
    price_window: Vec<f64>,
    volume_window: Vec<f64>,
    max_window_size: usize,
}

impl FeatureExtractor {
    pub fn new() -> Self {
        Self {
            price_window: Vec::new(),
            volume_window: Vec::new(),
            max_window_size: 100, // Keep last 100 data points
        }
    }
    
    pub fn extract_features(&mut self, market_context: &MarketContext) -> HashMap<String, f64> {
        let mut features = HashMap::new();
        
        // Update price and volume windows
        self.update_windows(market_context);
        
        // Basic price features
        features.insert("current_price".to_string(), 
            market_context.current_price.to_f64().unwrap_or(0.0));
        features.insert("bid_price".to_string(), 
            market_context.bid.to_f64().unwrap_or(0.0));
        features.insert("ask_price".to_string(), 
            market_context.ask.to_f64().unwrap_or(0.0));
        
        // Spread features
        let spread = market_context.ask - market_context.bid;
        let mid_price = (market_context.bid + market_context.ask) / Decimal::from(2);
        features.insert("spread_absolute".to_string(), 
            spread.to_f64().unwrap_or(0.0));
        features.insert("spread_relative".to_string(), 
            if !mid_price.is_zero() { 
                (spread / mid_price).to_f64().unwrap_or(0.0) 
            } else { 
                0.0 
            });
        
        // Volume features
        features.insert("volume_24h".to_string(), 
            market_context.volume_24h.to_f64().unwrap_or(0.0));
        features.insert("change_24h".to_string(), 
            market_context.change_24h.to_f64().unwrap_or(0.0));
        features.insert("change_24h_percent".to_string(), 
            if !market_context.current_price.is_zero() {
                (market_context.change_24h / market_context.current_price * Decimal::from(100))
                    .to_f64().unwrap_or(0.0)
            } else {
                0.0
            });
        
        // Volatility features
        if let Some(volatility) = market_context.volatility {
            features.insert("volatility".to_string(), volatility);
        }
        
        // Order book features
        if let Some(ref depth) = market_context.order_book_depth {
            features.insert("bid_depth".to_string(), 
                depth.bid_depth.to_f64().unwrap_or(0.0));
            features.insert("ask_depth".to_string(), 
                depth.ask_depth.to_f64().unwrap_or(0.0));
            features.insert("order_book_imbalance".to_string(), depth.imbalance);
            features.insert("order_book_spread".to_string(), 
                depth.spread.to_f64().unwrap_or(0.0));
        }
        
        // Technical indicators from price window
        if self.price_window.len() >= 2 {
            // Price momentum
            if let Some(current_price) = self.price_window.last() {
                let prev_price = self.price_window[self.price_window.len() - 2];
                features.insert("price_momentum_1".to_string(), 
                    (current_price - prev_price) / prev_price);
                
                // Simple moving averages
                if self.price_window.len() >= 5 {
                    let sma_5 = self.calculate_sma(5);
                    features.insert("sma_5".to_string(), sma_5);
                    features.insert("price_to_sma_5".to_string(), current_price / sma_5);
                }
                
                if self.price_window.len() >= 10 {
                    let sma_10 = self.calculate_sma(10);
                    features.insert("sma_10".to_string(), sma_10);
                    features.insert("price_to_sma_10".to_string(), current_price / sma_10);
                }
                
                if self.price_window.len() >= 20 {
                    let sma_20 = self.calculate_sma(20);
                    features.insert("sma_20".to_string(), sma_20);
                    features.insert("price_to_sma_20".to_string(), current_price / sma_20);
                    
                    // Bollinger Bands approximation
                    let std_dev = self.calculate_std_dev(20);
                    let bb_upper = sma_20 + 2.0 * std_dev;
                    let bb_lower = sma_20 - 2.0 * std_dev;
                    features.insert("bb_upper".to_string(), bb_upper);
                    features.insert("bb_lower".to_string(), bb_lower);
                    features.insert("bb_position".to_string(), 
                        (current_price - bb_lower) / (bb_upper - bb_lower));
                }
            }
            
            // RSI approximation (can be calculated without current_price variable)
            if self.price_window.len() >= 14 {
                let rsi = self.calculate_rsi(14);
                features.insert("rsi_14".to_string(), rsi);
            }
        }
        
        // Volume-based features
        if self.volume_window.len() >= 2 {
            if let Some(current_volume) = self.volume_window.last() {
                let prev_volume = self.volume_window[self.volume_window.len() - 2];
                features.insert("volume_momentum".to_string(), 
                    (current_volume - prev_volume) / prev_volume.max(1.0));
                
                if self.volume_window.len() >= 10 {
                    let volume_sma = self.calculate_volume_sma(10);
                    features.insert("volume_to_sma".to_string(), current_volume / volume_sma.max(1.0));
                }
            }
        }
        
        // Time-based features
        let hour = market_context.timestamp.hour() as f64;
        let day_of_week = market_context.timestamp.weekday().number_from_monday() as f64;
        features.insert("hour_of_day".to_string(), hour);
        features.insert("day_of_week".to_string(), day_of_week);
        features.insert("is_weekend".to_string(), if day_of_week >= 6.0 { 1.0 } else { 0.0 });
        
        features
    }
    
    fn update_windows(&mut self, market_context: &MarketContext) {
        let current_price = market_context.current_price.to_f64().unwrap_or(0.0);
        let current_volume = market_context.volume_24h.to_f64().unwrap_or(0.0);
        
        self.price_window.push(current_price);
        self.volume_window.push(current_volume);
        
        // Keep window size under control
        if self.price_window.len() > self.max_window_size {
            self.price_window.remove(0);
        }
        if self.volume_window.len() > self.max_window_size {
            self.volume_window.remove(0);
        }
    }
    
    fn calculate_sma(&self, period: usize) -> f64 {
        if self.price_window.len() < period {
            return 0.0;
        }
        
        let start_idx = self.price_window.len() - period;
        let sum: f64 = self.price_window[start_idx..].iter().sum();
        sum / period as f64
    }
    
    fn calculate_volume_sma(&self, period: usize) -> f64 {
        if self.volume_window.len() < period {
            return 0.0;
        }
        
        let start_idx = self.volume_window.len() - period;
        let sum: f64 = self.volume_window[start_idx..].iter().sum();
        sum / period as f64
    }
    
    fn calculate_std_dev(&self, period: usize) -> f64 {
        if self.price_window.len() < period {
            return 0.0;
        }
        
        let sma = self.calculate_sma(period);
        let start_idx = self.price_window.len() - period;
        let variance: f64 = self.price_window[start_idx..]
            .iter()
            .map(|&price| (price - sma).powi(2))
            .sum::<f64>() / period as f64;
        
        variance.sqrt()
    }
    
    fn calculate_rsi(&self, period: usize) -> f64 {
        if self.price_window.len() < period + 1 {
            return 50.0; // Neutral RSI
        }
        
        let mut gains = 0.0;
        let mut losses = 0.0;
        
        let start_idx = self.price_window.len() - period;
        for i in start_idx..self.price_window.len() {
            let change = self.price_window[i] - self.price_window[i - 1];
            if change > 0.0 {
                gains += change;
            } else {
                losses += change.abs();
            }
        }
        
        let avg_gain = gains / period as f64;
        let avg_loss = losses / period as f64;
        
        if avg_loss == 0.0 {
            return 100.0;
        }
        
        let rs = avg_gain / avg_loss;
        100.0 - (100.0 / (1.0 + rs))
    }
    
    pub fn get_feature_importance(&self) -> HashMap<String, f64> {
        let mut importance = HashMap::new();
        
        // Define feature importance based on trading knowledge
        importance.insert("current_price".to_string(), 0.9);
        importance.insert("spread_relative".to_string(), 0.8);
        importance.insert("order_book_imbalance".to_string(), 0.85);
        importance.insert("volume_momentum".to_string(), 0.7);
        importance.insert("price_momentum_1".to_string(), 0.75);
        importance.insert("rsi_14".to_string(), 0.7);
        importance.insert("bb_position".to_string(), 0.65);
        importance.insert("volatility".to_string(), 0.8);
        importance.insert("change_24h_percent".to_string(), 0.6);
        importance.insert("volume_to_sma".to_string(), 0.5);
        
        importance
    }
    
    pub fn normalize_features(&self, features: &mut HashMap<String, f64>) {
        // Normalize certain features to [0, 1] or [-1, 1] ranges
        
        // RSI is already in [0, 100], normalize to [0, 1]
        if let Some(rsi) = features.get_mut("rsi_14") {
            *rsi /= 100.0;
        }
        
        // Bollinger band position should be in [0, 1]
        if let Some(bb_pos) = features.get_mut("bb_position") {
            *bb_pos = bb_pos.clamp(0.0, 1.0);
        }
        
        // Hour of day to [0, 1]
        if let Some(hour) = features.get_mut("hour_of_day") {
            *hour /= 24.0;
        }
        
        // Day of week to [0, 1]
        if let Some(day) = features.get_mut("day_of_week") {
            *day /= 7.0;
        }
        
        // Clamp momentum features to reasonable ranges
        let momentum_features = ["price_momentum_1", "volume_momentum"];
        for feature in &momentum_features {
            if let Some(momentum) = features.get_mut(*feature) {
                *momentum = momentum.clamp(-1.0, 1.0);
            }
        }
    }
}

impl Default for FeatureExtractor {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use rust_decimal::Decimal;
    
    fn create_test_market_context() -> MarketContext {
        MarketContext {
            symbol: "BTC-USDT".to_string(),
            current_price: Decimal::new(50000, 0),
            bid: Decimal::new(49995, 0),
            ask: Decimal::new(50005, 0),
            volume_24h: Decimal::new(1000, 0),
            change_24h: Decimal::new(500, 0),
            volatility: Some(0.25),
            order_book_depth: Some(crate::types::OrderBookDepth {
                bid_depth: Decimal::new(10, 0),
                ask_depth: Decimal::new(12, 0),
                spread: Decimal::new(10, 0),
                imbalance: 0.1,
            }),
            timestamp: Utc::now(),
        }
    }
    
    #[test]
    fn test_feature_extraction() {
        let mut extractor = FeatureExtractor::new();
        let market_context = create_test_market_context();
        
        let features = extractor.extract_features(&market_context);
        
        assert!(features.contains_key("current_price"));
        assert!(features.contains_key("spread_relative"));
        assert!(features.contains_key("order_book_imbalance"));
        assert!(features.contains_key("hour_of_day"));
    }
    
    #[test]
    fn test_technical_indicators() {
        let mut extractor = FeatureExtractor::new();
        
        // Add multiple price points to test technical indicators
        for i in 1..=20 {
            let mut context = create_test_market_context();
            context.current_price = Decimal::new(50000 + i * 10, 0);
            extractor.extract_features(&context);
        }
        
        let context = create_test_market_context();
        let features = extractor.extract_features(&context);
        
        assert!(features.contains_key("sma_5"));
        assert!(features.contains_key("sma_10"));
        assert!(features.contains_key("rsi_14"));
        assert!(features.contains_key("bb_position"));
    }
}