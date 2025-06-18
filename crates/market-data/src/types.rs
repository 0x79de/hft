use serde::{Deserialize, Serialize};
use chrono::{DateTime, Utc};
use order_book::{Price, Quantity, Side};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[repr(C, align(64))]
pub struct Tick {
    pub symbol: String,
    pub price: Price,
    pub quantity: Quantity,
    pub side: Side,
    pub timestamp: DateTime<Utc>,
}

impl Tick {
    #[inline]
    pub fn new(symbol: String, price: Price, quantity: Quantity, side: Side) -> Self {
        Self {
            symbol,
            price,
            quantity,
            side,
            timestamp: Utc::now(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[repr(C, align(64))]
pub struct Level2Update {
    pub symbol: String,
    pub side: Side,
    pub price: Price,
    pub quantity: Quantity,
    pub update_type: UpdateType,
    pub timestamp: DateTime<Utc>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[repr(u8)]
pub enum UpdateType {
    Add = 0,
    Update = 1,
    Delete = 2,
}

impl Level2Update {
    #[inline]
    pub fn add(symbol: String, side: Side, price: Price, quantity: Quantity) -> Self {
        Self {
            symbol,
            side,
            price,
            quantity,
            update_type: UpdateType::Add,
            timestamp: Utc::now(),
        }
    }
    
    #[inline]
    pub fn update(symbol: String, side: Side, price: Price, quantity: Quantity) -> Self {
        Self {
            symbol,
            side,
            price,
            quantity,
            update_type: UpdateType::Update,
            timestamp: Utc::now(),
        }
    }
    
    #[inline]
    pub fn delete(symbol: String, side: Side, price: Price) -> Self {
        Self {
            symbol,
            side,
            price,
            quantity: Quantity::ZERO,
            update_type: UpdateType::Delete,
            timestamp: Utc::now(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OrderBookSnapshot {
    pub symbol: String,
    pub bids: Vec<(Price, Quantity)>,
    pub asks: Vec<(Price, Quantity)>,
    pub timestamp: DateTime<Utc>,
    pub sequence_number: u64,
}

impl OrderBookSnapshot {
    #[inline]
    pub fn new(symbol: String, sequence_number: u64) -> Self {
        Self {
            symbol,
            bids: Vec::with_capacity(100),
            asks: Vec::with_capacity(100),
            timestamp: Utc::now(),
            sequence_number,
        }
    }
    
    #[inline]
    pub fn best_bid(&self) -> Option<Price> {
        self.bids.first().map(|(price, _)| *price)
    }
    
    #[inline]
    pub fn best_ask(&self) -> Option<Price> {
        self.asks.first().map(|(price, _)| *price)
    }
    
    #[inline]
    pub fn spread(&self) -> Option<Price> {
        match (self.best_ask(), self.best_bid()) {
            (Some(ask), Some(bid)) => Some(ask - bid),
            _ => None,
        }
    }
    
    #[inline]
    pub fn mid_price(&self) -> Option<Price> {
        match (self.best_ask(), self.best_bid()) {
            (Some(ask), Some(bid)) => Some((ask + bid) / 2.0),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MarketSummary {
    pub symbol: String,
    pub open: Price,
    pub high: Price,
    pub low: Price,
    pub close: Price,
    pub volume: Quantity,
    pub vwap: Price,
    pub num_trades: u64,
    pub timestamp: DateTime<Utc>,
}

impl MarketSummary {
    #[inline]
    pub fn new(symbol: String, open: Price) -> Self {
        Self {
            symbol,
            open,
            high: open,
            low: open,
            close: open,
            volume: Quantity::ZERO,
            vwap: open,
            num_trades: 0,
            timestamp: Utc::now(),
        }
    }
    
    #[inline]
    pub fn update_trade(&mut self, price: Price, quantity: Quantity) {
        self.high = self.high.max(price);
        self.low = self.low.min(price);
        self.close = price;
        
        let old_notional = self.vwap.to_f64() * self.volume.to_f64();
        let new_notional = price.to_f64() * quantity.to_f64();
        
        self.volume += quantity;
        self.vwap = Price::new((old_notional + new_notional) / self.volume.to_f64());
        self.num_trades += 1;
        self.timestamp = Utc::now();
    }
}