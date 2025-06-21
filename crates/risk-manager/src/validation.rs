use order_book::{Order, Price, Quantity, Side, OrderType};
use thiserror::Error;
use serde::{Deserialize, Serialize};

#[derive(Debug, Error, Clone, Serialize, Deserialize)]
pub enum ValidationError {
    #[error("Order size {size} exceeds maximum allowed {max_size}")]
    OrderSizeExceedsLimit { size: f64, max_size: f64 },
    
    #[error("Position limit would be exceeded: current {current}, new {new_position}, limit {limit}")]
    PositionLimitExceeded { current: f64, new_position: f64, limit: f64 },
    
    #[error("Daily P&L limit exceeded: current {current_pnl}, limit {limit}")]
    DailyPnLLimitExceeded { current_pnl: f64, limit: f64 },
    
    #[error("Price {price} deviates {deviation}% from reference {reference_price}, limit {limit}%")]
    PriceDeviationExceedsLimit { 
        price: f64, 
        reference_price: f64, 
        deviation: f64, 
        limit: f64 
    },
    
    #[error("Notional value {notional} exceeds limit {limit}")]
    NotionalValueExceedsLimit { notional: f64, limit: f64 },
    
    #[error("Invalid order: {reason}")]
    InvalidOrder { reason: String },
    
    #[error("Market is closed for symbol {symbol}")]
    MarketClosed { symbol: String },
    
    #[error("Symbol {symbol} is not supported")]
    UnsupportedSymbol { symbol: String },
    
    #[error("Order quantity must be positive")]
    InvalidQuantity,
    
    #[error("Order price must be positive")]
    InvalidPrice,
    
    #[error("Order size is below minimum: {size} < {min_size}")]
    OrderSizeBelowMinimum { size: f64, min_size: f64 },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ValidationConfig {
    pub enable_price_validation: bool,
    pub enable_size_validation: bool,
    pub enable_position_validation: bool,
    pub enable_pnl_validation: bool,
    pub enable_notional_validation: bool,
    pub enable_market_hours_validation: bool,
    pub max_order_size: Quantity,
    pub min_order_size: Quantity,
    pub max_price_deviation_pct: f64,
    pub max_notional_value: f64,
    pub supported_symbols: Vec<String>,
}

impl Default for ValidationConfig {
    fn default() -> Self {
        Self {
            enable_price_validation: true,
            enable_size_validation: true,
            enable_position_validation: true,
            enable_pnl_validation: true,
            enable_notional_validation: true,
            enable_market_hours_validation: false,
            max_order_size: Quantity::new(1000.0),
            min_order_size: Quantity::new(0.001),
            max_price_deviation_pct: 5.0,
            max_notional_value: 1_000_000.0,
            supported_symbols: vec!["BTCUSD".to_string(), "ETHUSD".to_string()],
        }
    }
}

#[derive(Debug, Clone)]
pub struct OrderValidator {
    config: ValidationConfig,
}

impl OrderValidator {
    #[inline]
    pub fn new() -> Self {
        Self::with_config(ValidationConfig::default())
    }
    
    #[inline]
    pub fn with_config(config: ValidationConfig) -> Self {
        Self { config }
    }
    
    #[inline]
    pub fn validate_order(&self, order: &Order) -> Result<(), ValidationError> {
        self.validate_basic_order_properties(order)?;
        
        if self.config.enable_size_validation {
            self.validate_order_size(order)?;
        }
        
        if self.config.enable_notional_validation {
            self.validate_notional_value(order)?;
        }
        
        Ok(())
    }
    
    #[inline]
    pub fn validate_order_with_reference_price(
        &self, 
        order: &Order, 
        reference_price: Option<Price>
    ) -> Result<(), ValidationError> {
        self.validate_order(order)?;
        
        if self.config.enable_price_validation {
            if let Some(ref_price) = reference_price {
                self.validate_price_deviation(order.price, ref_price)?;
            }
        }
        
        Ok(())
    }
    
    #[inline]
    pub fn validate_position_impact(
        &self,
        order: &Order,
        current_position: f64,
        position_limit: f64,
    ) -> Result<(), ValidationError> {
        if !self.config.enable_position_validation {
            return Ok(());
        }
        
        let order_quantity = match order.side {
            Side::Buy => order.quantity.to_f64(),
            Side::Sell => -order.quantity.to_f64(),
        };
        
        let new_position = current_position + order_quantity;
        let new_position_abs = new_position.abs();
        
        if new_position_abs > position_limit {
            return Err(ValidationError::PositionLimitExceeded {
                current: current_position,
                new_position,
                limit: position_limit,
            });
        }
        
        Ok(())
    }
    
    #[inline]
    pub fn validate_pnl_impact(&self, current_pnl: f64, pnl_limit: f64) -> Result<(), ValidationError> {
        if !self.config.enable_pnl_validation {
            return Ok(());
        }
        
        if current_pnl < -pnl_limit {
            return Err(ValidationError::DailyPnLLimitExceeded {
                current_pnl,
                limit: pnl_limit,
            });
        }
        
        Ok(())
    }
    
    fn validate_basic_order_properties(&self, order: &Order) -> Result<(), ValidationError> {
        if order.quantity <= Quantity::ZERO {
            return Err(ValidationError::InvalidQuantity);
        }
        
        if order.price <= Price::ZERO && order.order_type != OrderType::Market {
            return Err(ValidationError::InvalidPrice);
        }
        
        if self.config.enable_market_hours_validation && !self.config.supported_symbols.contains(&order.symbol) {
            return Err(ValidationError::UnsupportedSymbol {
                symbol: order.symbol.clone(),
            });
        }
        
        Ok(())
    }
    
    fn validate_order_size(&self, order: &Order) -> Result<(), ValidationError> {
        if order.quantity > self.config.max_order_size {
            return Err(ValidationError::OrderSizeExceedsLimit {
                size: order.quantity.to_f64(),
                max_size: self.config.max_order_size.to_f64(),
            });
        }
        
        if order.quantity < self.config.min_order_size {
            return Err(ValidationError::OrderSizeBelowMinimum {
                size: order.quantity.to_f64(),
                min_size: self.config.min_order_size.to_f64(),
            });
        }
        
        Ok(())
    }
    
    fn validate_notional_value(&self, order: &Order) -> Result<(), ValidationError> {
        let notional = order.quantity.to_f64() * order.price.to_f64();
        
        if notional > self.config.max_notional_value {
            return Err(ValidationError::NotionalValueExceedsLimit {
                notional,
                limit: self.config.max_notional_value,
            });
        }
        
        Ok(())
    }
    
    fn validate_price_deviation(&self, order_price: Price, reference_price: Price) -> Result<(), ValidationError> {
        let deviation_pct = ((order_price.to_f64() - reference_price.to_f64()) / reference_price.to_f64()).abs() * 100.0;
        
        if deviation_pct > self.config.max_price_deviation_pct {
            return Err(ValidationError::PriceDeviationExceedsLimit {
                price: order_price.to_f64(),
                reference_price: reference_price.to_f64(),
                deviation: deviation_pct,
                limit: self.config.max_price_deviation_pct,
            });
        }
        
        Ok(())
    }
}

impl Default for OrderValidator {
    fn default() -> Self {
        Self::new()
    }
}