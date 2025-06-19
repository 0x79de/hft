use order_book::{Trade, Price, Side};
use std::collections::HashMap;
use chrono::{DateTime, Utc};
use uuid::Uuid;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Position {
    pub symbol: String,
    pub client_id: Uuid,
    pub quantity: f64, // Using f64 to allow positive/negative positions
    pub average_price: Price,
    pub unrealized_pnl: f64,
    pub realized_pnl: f64,
    pub total_pnl: f64,
    pub mark_price: Option<Price>,
    pub last_update: DateTime<Utc>,
    pub created_at: DateTime<Utc>,
}

impl Position {
    #[inline]
    pub fn new(symbol: String, client_id: Uuid) -> Self {
        let now = Utc::now();
        Self {
            symbol,
            client_id,
            quantity: 0.0,
            average_price: Price::ZERO,
            unrealized_pnl: 0.0,
            realized_pnl: 0.0,
            total_pnl: 0.0,
            mark_price: None,
            last_update: now,
            created_at: now,
        }
    }
    
    #[inline]
    pub fn is_long(&self) -> bool {
        self.quantity > 0.0
    }
    
    #[inline]
    pub fn is_short(&self) -> bool {
        self.quantity < 0.0
    }
    
    #[inline]
    pub fn is_flat(&self) -> bool {
        self.quantity == 0.0
    }
    
    #[inline]
    pub fn notional_value(&self) -> f64 {
        self.quantity * self.average_price.to_f64()
    }
    
    #[inline]
    pub fn update_mark_price(&mut self, mark_price: Price) {
        self.mark_price = Some(mark_price);
        self.calculate_unrealized_pnl();
        self.update_total_pnl();
        self.last_update = Utc::now();
    }
    
    #[inline]
    pub fn add_trade(&mut self, trade: &Trade, side: Side) {
        let trade_quantity = match side {
            Side::Buy => trade.quantity.to_f64(),
            Side::Sell => -trade.quantity.to_f64(),
        };
        
        if self.is_flat() {
            self.quantity = trade_quantity;
            self.average_price = trade.price;
        } else if (self.is_long() && side == Side::Buy) || (self.is_short() && side == Side::Sell) {
            let new_total_cost = self.notional_value() + (trade.quantity.to_f64() * trade.price.to_f64());
            let new_total_quantity = self.quantity + trade_quantity;
            
            if new_total_quantity != 0.0 {
                self.average_price = Price::new(new_total_cost / new_total_quantity);
            }
            self.quantity = new_total_quantity;
        } else {
            let closing_quantity = trade.quantity.to_f64().min(self.quantity.abs());
            let pnl_per_unit = match self.is_long() {
                true => trade.price.to_f64() - self.average_price.to_f64(),
                false => self.average_price.to_f64() - trade.price.to_f64(),
            };
            
            self.realized_pnl += pnl_per_unit * closing_quantity;
            
            let remaining_quantity = trade.quantity.to_f64() - closing_quantity;
            if self.is_long() {
                self.quantity -= closing_quantity;
            } else {
                self.quantity += closing_quantity;
            }
            
            if remaining_quantity > 0.0 {
                let remaining_trade_quantity = match side {
                    Side::Buy => remaining_quantity,
                    Side::Sell => -remaining_quantity,
                };
                
                self.quantity = remaining_trade_quantity;
                self.average_price = trade.price;
            }
        }
        
        self.calculate_unrealized_pnl();
        self.update_total_pnl();
        self.last_update = Utc::now();
    }
    
    fn calculate_unrealized_pnl(&mut self) {
        if let Some(mark_price) = self.mark_price {
            if !self.is_flat() {
                let pnl_per_unit = match self.is_long() {
                    true => mark_price.to_f64() - self.average_price.to_f64(),
                    false => self.average_price.to_f64() - mark_price.to_f64(),
                };
                self.unrealized_pnl = pnl_per_unit * self.quantity.abs();
            } else {
                self.unrealized_pnl = 0.0;
            }
        }
    }
    
    fn update_total_pnl(&mut self) {
        self.total_pnl = self.realized_pnl + self.unrealized_pnl;
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PositionTracker {
    pub symbol: String,
    pub positions: HashMap<Uuid, Position>,
    pub total_long_quantity: f64,
    pub total_short_quantity: f64,
    pub net_quantity: f64,
    pub total_realized_pnl: f64,
    pub total_unrealized_pnl: f64,
    pub total_pnl: f64,
    pub last_update: DateTime<Utc>,
}

impl PositionTracker {
    #[inline]
    pub fn new(symbol: String) -> Self {
        Self {
            symbol,
            positions: HashMap::new(),
            total_long_quantity: 0.0,
            total_short_quantity: 0.0,
            net_quantity: 0.0,
            total_realized_pnl: 0.0,
            total_unrealized_pnl: 0.0,
            total_pnl: 0.0,
            last_update: Utc::now(),
        }
    }
    
    #[inline]
    pub fn get_position(&self, client_id: Uuid) -> Option<&Position> {
        self.positions.get(&client_id)
    }
    
    #[inline]
    pub fn get_or_create_position(&mut self, client_id: Uuid) -> &mut Position {
        self.positions
            .entry(client_id)
            .or_insert_with(|| Position::new(self.symbol.clone(), client_id))
    }
    
    #[inline]
    pub fn update_position_with_trade(&mut self, trade: &Trade, client_id: Uuid, side: Side) {
        let position = self.get_or_create_position(client_id);
        position.add_trade(trade, side);
        self.update_aggregates();
    }
    
    #[inline]
    pub fn update_mark_prices(&mut self, mark_price: Price) {
        for position in self.positions.values_mut() {
            position.update_mark_price(mark_price);
        }
        self.update_aggregates();
    }
    
    #[inline]
    pub fn get_total_exposure(&self) -> f64 {
        self.total_long_quantity + self.total_short_quantity.abs()
    }
    
    #[inline]
    pub fn get_position_count(&self) -> usize {
        self.positions.values().filter(|p| !p.is_flat()).count()
    }
    
    #[inline]
    pub fn get_max_position_size(&self) -> f64 {
        self.positions
            .values()
            .map(|p| p.quantity.abs())
            .fold(0.0, f64::max)
    }
    
    fn update_aggregates(&mut self) {
        self.total_long_quantity = 0.0;
        self.total_short_quantity = 0.0;
        self.total_realized_pnl = 0.0;
        self.total_unrealized_pnl = 0.0;
        
        for position in self.positions.values() {
            if position.is_long() {
                self.total_long_quantity += position.quantity;
            } else if position.is_short() {
                self.total_short_quantity += position.quantity;
            }
            
            self.total_realized_pnl += position.realized_pnl;
            self.total_unrealized_pnl += position.unrealized_pnl;
        }
        
        self.net_quantity = self.total_long_quantity + self.total_short_quantity;
        self.total_pnl = self.total_realized_pnl + self.total_unrealized_pnl;
        self.last_update = Utc::now();
    }
}