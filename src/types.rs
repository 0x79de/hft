//! Core data types for the HFT trading system

use serde::{Deserialize, Serialize};
use std::fmt;
use std::ops::Add;
use std::sync::atomic::{AtomicU64, Ordering};
use uuid::Uuid;
use chrono::{DateTime, Utc};

pub use num_traits::{Zero, One};

static ORDER_ID_COUNTER: AtomicU64 = AtomicU64::new(1);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct OrderId(pub u64);

impl OrderId {
    pub fn new() -> Self {
        Self(ORDER_ID_COUNTER.fetch_add(1, Ordering::Relaxed))
    }

    pub fn from_u64(id: u64) -> Self {
        Self(id)
    }

    pub fn as_u64(&self) -> u64 {
        self.0
    }
}

impl Default for OrderId {
    fn default() -> Self {
        Self::new()
    }
}

impl fmt::Display for OrderId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Side {
    Bid,
    Ask,
}

impl fmt::Display for Side {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Side::Bid => write!(f, "BID"),
            Side::Ask => write!(f, "ASK"),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct Price(pub u64);

impl Price {
    const SCALE: u64 = 100_000_000; // 8 decimal places

    pub fn from_f64(price: f64) -> Self {
        Self((price * Self::SCALE as f64) as u64)
    }

    pub fn to_f64(&self) -> f64 {
        self.0 as f64 / Self::SCALE as f64
    }

    pub fn raw(&self) -> u64 {
        self.0
    }
}

impl Add for Price {
    type Output = Self;

    fn add(self, other: Self) -> Self::Output {
        Self(self.0 + other.0)
    }
}

impl Zero for Price {
    fn zero() -> Self {
        Self(0)
    }

    fn is_zero(&self) -> bool {
        self.0 == 0
    }
}

impl fmt::Display for Price {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:.8}", self.to_f64())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct Quantity(pub u64);

impl Quantity {
    pub fn new(qty: u64) -> Self {
        Self(qty)
    }

    pub fn raw(&self) -> u64 {
        self.0
    }
}

impl Add for Quantity {
    type Output = Self;

    fn add(self, other: Self) -> Self::Output {
        Self(self.0 + other.0)
    }
}

impl Zero for Quantity {
    fn zero() -> Self {
        Self(0)
    }

    fn is_zero(&self) -> bool {
        self.0 == 0
    }
}

impl fmt::Display for Quantity {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Order {
    pub id: OrderId,
    pub symbol: String,
    pub side: Side,
    pub price: Price,
    pub quantity: Quantity,
    pub filled: Quantity,
    pub timestamp: DateTime<Utc>,
    pub client_id: Option<String>,
}

impl Order {
    pub fn new(
        symbol: String,
        side: Side,
        price: Price,
        quantity: Quantity,
        client_id: Option<String>,
    ) -> Self {
        Self {
            id: OrderId::new(),
            symbol,
            side,
            price,
            quantity,
            filled: Quantity::zero(),
            timestamp: Utc::now(),
            client_id,
        }
    }

    pub fn remaining(&self) -> Quantity {
        Quantity(self.quantity.0.saturating_sub(self.filled.0))
    }

    pub fn is_filled(&self) -> bool {
        self.filled >= self.quantity
    }

    pub fn fill(&mut self, qty: Quantity) -> Quantity {
        let fillable = std::cmp::min(qty.0, self.remaining().0);
        self.filled.0 += fillable;
        Quantity(fillable)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Trade {
    pub id: Uuid,
    pub symbol: String,
    pub price: Price,
    pub quantity: Quantity,
    pub buyer_order_id: OrderId,
    pub seller_order_id: OrderId,
    pub timestamp: DateTime<Utc>,
}

impl Trade {
    pub fn new(
        symbol: String,
        price: Price,
        quantity: Quantity,
        buyer_order_id: OrderId,
        seller_order_id: OrderId,
    ) -> Self {
        Self {
            id: Uuid::new_v4(),
            symbol,
            price,
            quantity,
            buyer_order_id,
            seller_order_id,
            timestamp: Utc::now(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MarketData {
    pub symbol: String,
    pub best_bid: Option<Price>,
    pub best_ask: Option<Price>,
    pub bid_quantity: Quantity,
    pub ask_quantity: Quantity,
    pub last_trade_price: Option<Price>,
    pub timestamp: DateTime<Utc>,
}

impl MarketData {
    pub fn new(symbol: String) -> Self {
        Self {
            symbol,
            best_bid: None,
            best_ask: None,
            bid_quantity: Quantity::zero(),
            ask_quantity: Quantity::zero(),
            last_trade_price: None,
            timestamp: Utc::now(),
        }
    }

    pub fn spread(&self) -> Option<Price> {
        match (self.best_bid, self.best_ask) {
            (Some(bid), Some(ask)) => Some(Price(ask.0.saturating_sub(bid.0))),
            _ => None,
        }
    }

    pub fn mid_price(&self) -> Option<Price> {
        match (self.best_bid, self.best_ask) {
            (Some(bid), Some(ask)) => Some(Price((bid.0 + ask.0) / 2)),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_order_id_generation() {
        let id1 = OrderId::new();
        let id2 = OrderId::new();
        assert!(id2.0 > id1.0);
    }

    #[test]
    fn test_price_conversion() {
        let price = Price::from_f64(123.45678901);
        assert_eq!(price.to_f64(), 123.45678901);
    }

    #[test]
    fn test_order_fill() {
        let mut order = Order::new(
            "BTCUSD".to_string(),
            Side::Bid,
            Price::from_f64(50000.0),
            Quantity::new(100),
            None,
        );

        let filled = order.fill(Quantity::new(30));
        assert_eq!(filled, Quantity::new(30));
        assert_eq!(order.filled, Quantity::new(30));
        assert_eq!(order.remaining(), Quantity::new(70));
        assert!(!order.is_filled());

        let filled = order.fill(Quantity::new(80));
        assert_eq!(filled, Quantity::new(70));
        assert_eq!(order.filled, Quantity::new(100));
        assert!(order.is_filled());
    }

    #[test]
    fn test_market_data() {
        let mut data = MarketData::new("BTCUSD".to_string());
        data.best_bid = Some(Price::from_f64(50000.0));
        data.best_ask = Some(Price::from_f64(50010.0));

        assert_eq!(data.spread(), Some(Price::from_f64(10.0)));
        assert_eq!(data.mid_price(), Some(Price::from_f64(50005.0)));
    }
}