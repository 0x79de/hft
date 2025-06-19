use serde::{Deserialize, Serialize};
use std::fmt;
use std::ops::{Add, Sub, Mul, Div, AddAssign, SubAssign};
use std::cmp::Ordering;
use std::sync::atomic::{AtomicU64, Ordering as AtomicOrdering};
use chrono::{DateTime, Utc};
use uuid::Uuid;
use fixed::{FixedI64, FixedU64};

pub type PriceFixed = FixedI64<typenum::U6>;
pub type QuantityFixed = FixedU64<typenum::U6>;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[repr(transparent)]
pub struct Price(PriceFixed);

impl Price {
    pub const ZERO: Self = Self(PriceFixed::ZERO);
    pub const MAX: Self = Self(PriceFixed::MAX);
    pub const MIN: Self = Self(PriceFixed::MIN);
    
    #[inline]
    pub fn new(value: f64) -> Self {
        Self(PriceFixed::from_num(value))
    }
    
    #[inline]
    pub fn from_raw(raw: i64) -> Self {
        Self(PriceFixed::from_bits(raw))
    }
    
    #[inline]
    pub fn to_f64(self) -> f64 {
        self.0.to_num()
    }
    
    #[inline]
    pub fn to_raw(self) -> i64 {
        self.0.to_bits()
    }
    
    #[inline]
    pub fn abs(self) -> Self {
        Self(self.0.abs())
    }
}

impl fmt::Display for Price {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:.6}", self.to_f64())
    }
}

impl PartialOrd for Price {
    #[inline]
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for Price {
    #[inline]
    fn cmp(&self, other: &Self) -> Ordering {
        self.0.cmp(&other.0)
    }
}

impl Add for Price {
    type Output = Self;
    #[inline]
    fn add(self, rhs: Self) -> Self::Output {
        Self(self.0 + rhs.0)
    }
}

impl Sub for Price {
    type Output = Self;
    #[inline]
    fn sub(self, rhs: Self) -> Self::Output {
        Self(self.0 - rhs.0)
    }
}

impl Mul<f64> for Price {
    type Output = Self;
    #[inline]
    fn mul(self, rhs: f64) -> Self::Output {
        Self(self.0 * PriceFixed::from_num(rhs))
    }
}

impl Div<f64> for Price {
    type Output = Self;
    #[inline]
    fn div(self, rhs: f64) -> Self::Output {
        Self(self.0 / PriceFixed::from_num(rhs))
    }
}

impl AddAssign for Price {
    #[inline]
    fn add_assign(&mut self, rhs: Self) {
        self.0 += rhs.0;
    }
}

impl SubAssign for Price {
    #[inline]
    fn sub_assign(&mut self, rhs: Self) {
        self.0 -= rhs.0;
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[repr(transparent)]
pub struct Quantity(QuantityFixed);

impl Quantity {
    pub const ZERO: Self = Self(QuantityFixed::ZERO);
    pub const MAX: Self = Self(QuantityFixed::MAX);
    
    #[inline]
    pub fn new(value: f64) -> Self {
        Self(QuantityFixed::from_num(value))
    }
    
    #[inline]
    pub fn from_raw(raw: u64) -> Self {
        Self(QuantityFixed::from_bits(raw))
    }
    
    #[inline]
    pub fn to_f64(self) -> f64 {
        self.0.to_num()
    }
    
    #[inline]
    pub fn to_raw(self) -> u64 {
        self.0.to_bits()
    }
    
    #[inline]
    pub fn abs(self) -> Self {
        self // Quantity is always positive (unsigned)
    }
}

impl fmt::Display for Quantity {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:.6}", self.to_f64())
    }
}

impl PartialOrd for Quantity {
    #[inline]
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for Quantity {
    #[inline]
    fn cmp(&self, other: &Self) -> Ordering {
        self.0.cmp(&other.0)
    }
}

impl Add for Quantity {
    type Output = Self;
    #[inline]
    fn add(self, rhs: Self) -> Self::Output {
        Self(self.0 + rhs.0)
    }
}

impl Sub for Quantity {
    type Output = Self;
    #[inline]
    fn sub(self, rhs: Self) -> Self::Output {
        Self(self.0 - rhs.0)
    }
}

impl AddAssign for Quantity {
    #[inline]
    fn add_assign(&mut self, rhs: Self) {
        self.0 += rhs.0;
    }
}

impl SubAssign for Quantity {
    #[inline]
    fn sub_assign(&mut self, rhs: Self) {
        self.0 -= rhs.0;
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[repr(u8)]
pub enum Side {
    Buy = 0,
    Sell = 1,
}

impl Side {
    #[inline]
    pub fn opposite(self) -> Self {
        match self {
            Side::Buy => Side::Sell,
            Side::Sell => Side::Buy,
        }
    }
    
    #[inline]
    pub fn is_buy(self) -> bool {
        matches!(self, Side::Buy)
    }
    
    #[inline]
    pub fn is_sell(self) -> bool {
        matches!(self, Side::Sell)
    }
}

impl fmt::Display for Side {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Side::Buy => write!(f, "BUY"),
            Side::Sell => write!(f, "SELL"),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[repr(transparent)]
pub struct OrderId(u64);

static ORDER_ID_COUNTER: AtomicU64 = AtomicU64::new(1);

impl OrderId {
    #[inline]
    pub fn new() -> Self {
        Self(ORDER_ID_COUNTER.fetch_add(1, AtomicOrdering::Relaxed))
    }
    
    #[inline]
    pub fn from_raw(id: u64) -> Self {
        Self(id)
    }
    
    #[inline]
    pub fn to_raw(self) -> u64 {
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
#[repr(u8)]
pub enum OrderType {
    Market = 0,
    Limit = 1,
    Stop = 2,
    StopLimit = 3,
}

impl fmt::Display for OrderType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            OrderType::Market => write!(f, "MARKET"),
            OrderType::Limit => write!(f, "LIMIT"),
            OrderType::Stop => write!(f, "STOP"),
            OrderType::StopLimit => write!(f, "STOP_LIMIT"),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[repr(u8)]
pub enum OrderStatus {
    Pending = 0,
    PartiallyFilled = 1,
    Filled = 2,
    Cancelled = 3,
    Rejected = 4,
}

impl fmt::Display for OrderStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            OrderStatus::Pending => write!(f, "PENDING"),
            OrderStatus::PartiallyFilled => write!(f, "PARTIALLY_FILLED"),
            OrderStatus::Filled => write!(f, "FILLED"),
            OrderStatus::Cancelled => write!(f, "CANCELLED"),
            OrderStatus::Rejected => write!(f, "REJECTED"),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[repr(C, align(64))]
pub struct Order {
    pub id: OrderId,
    pub symbol: String,
    pub side: Side,
    pub order_type: OrderType,
    pub price: Price,
    pub quantity: Quantity,
    pub filled_quantity: Quantity,
    pub status: OrderStatus,
    pub timestamp: DateTime<Utc>,
    pub client_id: Uuid,
}

impl Order {
    #[inline]
    pub fn new(
        symbol: String,
        side: Side,
        order_type: OrderType,
        price: Price,
        quantity: Quantity,
        client_id: Uuid,
    ) -> Self {
        Self {
            id: OrderId::new(),
            symbol,
            side,
            order_type,
            price,
            quantity,
            filled_quantity: Quantity::ZERO,
            status: OrderStatus::Pending,
            timestamp: Utc::now(),
            client_id,
        }
    }
    
    #[inline]
    pub fn remaining_quantity(&self) -> Quantity {
        self.quantity - self.filled_quantity
    }
    
    #[inline]
    pub fn is_fully_filled(&self) -> bool {
        self.filled_quantity >= self.quantity
    }
    
    #[inline]
    pub fn fill(&mut self, quantity: Quantity) {
        self.filled_quantity += quantity;
        if self.is_fully_filled() {
            self.status = OrderStatus::Filled;
        } else {
            self.status = OrderStatus::PartiallyFilled;
        }
    }
    
    #[inline]
    pub fn cancel(&mut self) {
        self.status = OrderStatus::Cancelled;
    }
    
    #[inline]
    pub fn reject(&mut self) {
        self.status = OrderStatus::Rejected;
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[repr(C, align(64))]
pub struct Trade {
    pub id: u64,
    pub symbol: String,
    pub buyer_order_id: OrderId,
    pub seller_order_id: OrderId,
    pub price: Price,
    pub quantity: Quantity,
    pub timestamp: DateTime<Utc>,
    pub buyer_client_id: Uuid,
    pub seller_client_id: Uuid,
}

static TRADE_ID_COUNTER: AtomicU64 = AtomicU64::new(1);

impl Trade {
    #[inline]
    pub fn new(
        symbol: &str,
        buyer_order_id: OrderId,
        seller_order_id: OrderId,
        price: Price,
        quantity: Quantity,
        buyer_client_id: Uuid,
        seller_client_id: Uuid,
    ) -> Self {
        Self {
            id: TRADE_ID_COUNTER.fetch_add(1, AtomicOrdering::Relaxed),
            symbol: symbol.to_string(),
            buyer_order_id,
            seller_order_id,
            price,
            quantity,
            timestamp: Utc::now(),
            buyer_client_id,
            seller_client_id,
        }
    }
    
    #[inline]
    pub fn notional_value(&self) -> f64 {
        self.price.to_f64() * self.quantity.to_f64()
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[repr(C, align(64))]
pub struct MarketData {
    pub symbol: String,
    pub best_bid: Option<Price>,
    pub best_ask: Option<Price>,
    pub bid_size: Quantity,
    pub ask_size: Quantity,
    pub last_trade_price: Option<Price>,
    pub last_trade_quantity: Option<Quantity>,
    pub volume: Quantity,
    pub timestamp: DateTime<Utc>,
}

impl MarketData {
    #[inline]
    pub fn new(symbol: String) -> Self {
        Self {
            symbol,
            best_bid: None,
            best_ask: None,
            bid_size: Quantity::ZERO,
            ask_size: Quantity::ZERO,
            last_trade_price: None,
            last_trade_quantity: None,
            volume: Quantity::ZERO,
            timestamp: Utc::now(),
        }
    }
    
    #[inline]
    pub fn spread(&self) -> Option<Price> {
        match (self.best_ask, self.best_bid) {
            (Some(ask), Some(bid)) => Some(ask - bid),
            _ => None,
        }
    }
    
    #[inline]
    pub fn mid_price(&self) -> Option<Price> {
        match (self.best_ask, self.best_bid) {
            (Some(ask), Some(bid)) => Some((ask + bid) / 2.0),
            _ => None,
        }
    }
    
    #[inline]
    pub fn update_trade(&mut self, price: Price, quantity: Quantity) {
        self.last_trade_price = Some(price);
        self.last_trade_quantity = Some(quantity);
        self.volume += quantity;
        self.timestamp = Utc::now();
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MarketSnapshot {
    pub symbol: String,
    pub bids: Vec<(Price, Quantity)>,
    pub asks: Vec<(Price, Quantity)>,
    pub trades: Vec<Trade>,
    pub timestamp: DateTime<Utc>,
}

impl MarketSnapshot {
    #[inline]
    pub fn new(symbol: String) -> Self {
        Self {
            symbol,
            bids: Vec::with_capacity(100),
            asks: Vec::with_capacity(100),
            trades: Vec::with_capacity(1000),
            timestamp: Utc::now(),
        }
    }
    
    #[inline]
    pub fn total_bid_volume(&self) -> Quantity {
        self.bids.iter().map(|(_, qty)| *qty).fold(Quantity::ZERO, |acc, qty| acc + qty)
    }
    
    #[inline]
    pub fn total_ask_volume(&self) -> Quantity {
        self.asks.iter().map(|(_, qty)| *qty).fold(Quantity::ZERO, |acc, qty| acc + qty)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_price_creation_and_conversion() {
        let price = Price::new(100.5);
        assert!((price.to_f64() - 100.5).abs() < 0.000001);
        
        let raw = price.to_raw();
        let price2 = Price::from_raw(raw);
        assert_eq!(price, price2);
    }

    #[test]
    fn test_price_arithmetic() {
        let p1 = Price::new(100.0);
        let p2 = Price::new(50.0);
        
        assert_eq!((p1 + p2).to_f64(), 150.0);
        assert_eq!((p1 - p2).to_f64(), 50.0);
        assert_eq!((p1 * 2.0).to_f64(), 200.0);
        assert_eq!((p1 / 2.0).to_f64(), 50.0);
        
        let mut p3 = p1;
        p3 += p2;
        assert_eq!(p3.to_f64(), 150.0);
        
        p3 -= p2;
        assert_eq!(p3.to_f64(), 100.0);
    }

    #[test]
    fn test_price_ordering() {
        let p1 = Price::new(100.0);
        let p2 = Price::new(200.0);
        let p3 = Price::new(100.0);
        
        assert!(p1 < p2);
        assert!(p2 > p1);
        assert_eq!(p1, p3);
        assert!(p1 <= p3);
        assert!(p1 >= p3);
    }

    #[test]
    fn test_quantity_operations() {
        let q1 = Quantity::new(100.0);
        let q2 = Quantity::new(50.0);
        
        assert_eq!((q1 + q2).to_f64(), 150.0);
        assert_eq!((q1 - q2).to_f64(), 50.0);
        
        let mut q3 = q1;
        q3 += q2;
        assert_eq!(q3.to_f64(), 150.0);
    }

    #[test]
    fn test_side_operations() {
        assert_eq!(Side::Buy.opposite(), Side::Sell);
        assert_eq!(Side::Sell.opposite(), Side::Buy);
        
        assert!(Side::Buy.is_buy());
        assert!(!Side::Buy.is_sell());
        assert!(Side::Sell.is_sell());
        assert!(!Side::Sell.is_buy());
    }

    #[test]
    fn test_order_id_generation() {
        let id1 = OrderId::new();
        let id2 = OrderId::new();
        
        assert_ne!(id1, id2);
        assert!(id1.to_raw() < id2.to_raw());
        
        let id3 = OrderId::from_raw(12345);
        assert_eq!(id3.to_raw(), 12345);
    }

    #[test]
    fn test_order_creation_and_filling() {
        let client_id = Uuid::new_v4();
        let mut order = Order::new(
            "BTCUSD".to_string(),
            Side::Buy,
            OrderType::Limit,
            Price::new(50000.0),
            Quantity::new(1.0),
            client_id,
        );
        
        assert_eq!(order.remaining_quantity(), Quantity::new(1.0));
        assert!(!order.is_fully_filled());
        assert_eq!(order.status, OrderStatus::Pending);
        
        order.fill(Quantity::new(0.5));
        assert_eq!(order.filled_quantity, Quantity::new(0.5));
        assert_eq!(order.remaining_quantity(), Quantity::new(0.5));
        assert!(!order.is_fully_filled());
        assert_eq!(order.status, OrderStatus::PartiallyFilled);
        
        order.fill(Quantity::new(0.5));
        assert_eq!(order.filled_quantity, Quantity::new(1.0));
        assert_eq!(order.remaining_quantity(), Quantity::ZERO);
        assert!(order.is_fully_filled());
        assert_eq!(order.status, OrderStatus::Filled);
    }

    #[test]
    fn test_order_cancel_and_reject() {
        let client_id = Uuid::new_v4();
        let mut order = Order::new(
            "BTCUSD".to_string(),
            Side::Buy,
            OrderType::Limit,
            Price::new(50000.0),
            Quantity::new(1.0),
            client_id,
        );
        
        order.cancel();
        assert_eq!(order.status, OrderStatus::Cancelled);
        
        let mut order2 = Order::new(
            "BTCUSD".to_string(),
            Side::Buy,
            OrderType::Limit,
            Price::new(50000.0),
            Quantity::new(1.0),
            client_id,
        );
        
        order2.reject();
        assert_eq!(order2.status, OrderStatus::Rejected);
    }

    #[test]
    fn test_trade_creation() {
        let buyer_id = Uuid::new_v4();
        let seller_id = Uuid::new_v4();
        let buyer_order_id = OrderId::new();
        let seller_order_id = OrderId::new();
        
        let trade = Trade::new(
            "BTCUSD",
            buyer_order_id,
            seller_order_id,
            Price::new(50000.0),
            Quantity::new(1.0),
            buyer_id,
            seller_id,
        );
        
        assert_eq!(trade.notional_value(), 50000.0);
        assert_eq!(trade.buyer_order_id, buyer_order_id);
        assert_eq!(trade.seller_order_id, seller_order_id);
    }

    #[test]
    fn test_market_data_operations() {
        let mut md = MarketData::new("BTCUSD".to_string());
        
        md.best_bid = Some(Price::new(49950.0));
        md.best_ask = Some(Price::new(50050.0));
        
        assert_eq!(md.spread(), Some(Price::new(100.0)));
        assert_eq!(md.mid_price(), Some(Price::new(50000.0)));
        
        md.update_trade(Price::new(50000.0), Quantity::new(1.0));
        assert_eq!(md.last_trade_price, Some(Price::new(50000.0)));
        assert_eq!(md.volume, Quantity::new(1.0));
    }

    #[test]
    fn test_market_snapshot() {
        let mut snapshot = MarketSnapshot::new("BTCUSD".to_string());
        
        snapshot.bids.push((Price::new(49950.0), Quantity::new(1.0)));
        snapshot.bids.push((Price::new(49940.0), Quantity::new(2.0)));
        snapshot.asks.push((Price::new(50050.0), Quantity::new(1.5)));
        snapshot.asks.push((Price::new(50060.0), Quantity::new(2.5)));
        
        assert_eq!(snapshot.total_bid_volume(), Quantity::new(3.0));
        assert_eq!(snapshot.total_ask_volume(), Quantity::new(4.0));
    }

    #[test]
    fn test_constants() {
        assert_eq!(Price::ZERO.to_f64(), 0.0);
        assert_eq!(Quantity::ZERO.to_f64(), 0.0);
        
        assert!(Price::MAX.to_f64() > 0.0);
        assert!(Quantity::MAX.to_f64() > 0.0);
    }

    #[test]
    fn test_display_formatting() {
        let price = Price::new(123.456789);
        let quantity = Quantity::new(987.654321);
        let side = Side::Buy;
        let order_id = OrderId::from_raw(12345);
        
        assert!(!format!("{}", price).is_empty());
        assert!(!format!("{}", quantity).is_empty());
        assert_eq!(format!("{}", side), "BUY");
        assert_eq!(format!("{}", order_id), "12345");
    }

    #[test] 
    fn test_serialization() {
        let price = Price::new(123.45);
        let serialized = serde_json::to_string(&price).unwrap();
        let deserialized: Price = serde_json::from_str(&serialized).unwrap();
        assert_eq!(price, deserialized);
        
        let client_id = Uuid::new_v4();
        let order = Order::new(
            "BTCUSD".to_string(),
            Side::Buy,
            OrderType::Limit,
            Price::new(50000.0),
            Quantity::new(1.0),
            client_id,
        );
        
        let serialized = serde_json::to_string(&order).unwrap();
        let deserialized: Order = serde_json::from_str(&serialized).unwrap();
        assert_eq!(order, deserialized);
    }
}