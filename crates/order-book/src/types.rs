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
        symbol: String,
        buyer_order_id: OrderId,
        seller_order_id: OrderId,
        price: Price,
        quantity: Quantity,
        buyer_client_id: Uuid,
        seller_client_id: Uuid,
    ) -> Self {
        Self {
            id: TRADE_ID_COUNTER.fetch_add(1, AtomicOrdering::Relaxed),
            symbol,
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