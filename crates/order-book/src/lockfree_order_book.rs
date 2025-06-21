use crate::types::{Price, Quantity, Order, OrderId, Side, Trade};
use crate::atomic_price_level::AtomicPriceLevel;
use crossbeam_skiplist::SkipMap;
use dashmap::DashMap;
use std::sync::atomic::{AtomicU64, AtomicBool, Ordering};
use std::sync::Arc;
use serde::{Deserialize, Serialize};
use chrono::{DateTime, Utc};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum LockFreeOrderBookError {
    #[error("Order not found: {order_id}")]
    OrderNotFound { order_id: OrderId },
    #[error("Invalid price: {price}")]
    InvalidPrice { price: Price },
    #[error("Invalid quantity: {quantity}")]
    InvalidQuantity { quantity: Quantity },
    #[error("Order already exists: {order_id}")]
    OrderAlreadyExists { order_id: OrderId },
    #[error("Insufficient liquidity")]
    InsufficientLiquidity,
    #[error("Price level is being modified")]
    PriceLevelBusy,
}

#[derive(Debug, Clone, PartialEq)]
pub enum LockFreeMatchResult {
    NoMatch,
    PartialMatch {
        trades: Vec<Trade>,
        remaining_quantity: Quantity,
    },
    FullMatch {
        trades: Vec<Trade>,
    },
}

/// High-performance lock-free order book implementation
/// Uses atomic operations and lock-free data structures for maximum throughput
#[derive(Debug)]
pub struct LockFreeOrderBook {
    symbol: String,
    
    // Use SkipMap for O(log n) ordered access with high concurrency
    bids: SkipMap<std::cmp::Reverse<Price>, Arc<AtomicPriceLevel>>,
    asks: SkipMap<Price, Arc<AtomicPriceLevel>>,
    
    // Fast order lookup
    orders: DashMap<OrderId, Order>,
    
    // Atomic cache for best prices (avoids locks)
    best_bid_cache: AtomicU64, // Store as raw bits for atomic access
    best_ask_cache: AtomicU64,
    
    // Dirty flags to know when cache needs updating
    best_bid_dirty: AtomicBool,
    best_ask_dirty: AtomicBool,
    
    // Statistics
    sequence_number: AtomicU64,
    total_trades: AtomicU64,
    last_update_nanos: AtomicU64,
}

impl LockFreeOrderBook {
    /// Create a new lock-free order book
    #[inline]
    pub fn new(symbol: String) -> Self {
        Self {
            symbol,
            bids: SkipMap::new(),
            asks: SkipMap::new(),
            orders: DashMap::new(),
            best_bid_cache: AtomicU64::new(0), // 0 represents None
            best_ask_cache: AtomicU64::new(u64::MAX), // MAX represents None
            best_bid_dirty: AtomicBool::new(true),
            best_ask_dirty: AtomicBool::new(true),
            sequence_number: AtomicU64::new(0),
            total_trades: AtomicU64::new(0),
            last_update_nanos: AtomicU64::new(0),
        }
    }
    
    /// Get the symbol for this order book
    #[inline]
    pub fn symbol(&self) -> &str {
        &self.symbol
    }
    
    /// Add an order to the book and attempt matching
    #[inline]
    pub fn add_order(&self, mut order: Order) -> LockFreeMatchResult {
        self.update_timestamp();
        
        // Fast path for market orders that will likely match completely
        let match_result = self.match_order(&mut order);
        
        // Add remaining quantity to book if any
        if order.remaining_quantity() > Quantity::ZERO {
            let order_side = order.side;
            let order_price = order.price;
            self.insert_order_to_book(&order);
            self.orders.insert(order.id, order);
            
            // Update best price cache
            self.maybe_update_best_price_cache(order_side, order_price);
        }
        
        match_result
    }
    
    /// Cancel an order by ID
    #[inline]
    pub fn cancel_order(&self, order_id: OrderId) -> Option<Order> {
        if let Some((_, mut order)) = self.orders.remove(&order_id) {
            order.cancel();
            self.remove_order_from_book(&order);
            self.update_timestamp();
            Some(order)
        } else {
            None
        }
    }
    
    /// Get an order by ID
    #[inline]
    pub fn get_order(&self, order_id: OrderId) -> Option<Order> {
        self.orders.get(&order_id).map(|entry| entry.clone())
    }
    
    /// Get the best bid price
    #[inline]
    pub fn best_bid(&self) -> Option<Price> {
        if self.best_bid_dirty.load(Ordering::Acquire) {
            self.update_best_bid_cache();
        }
        
        let cached = self.best_bid_cache.load(Ordering::Acquire);
        if cached == 0 {
            None
        } else {
            Some(Price::from_raw(cached as i64))
        }
    }
    
    /// Get the best ask price
    #[inline]
    pub fn best_ask(&self) -> Option<Price> {
        if self.best_ask_dirty.load(Ordering::Acquire) {
            self.update_best_ask_cache();
        }
        
        let cached = self.best_ask_cache.load(Ordering::Acquire);
        if cached == u64::MAX {
            None
        } else {
            Some(Price::from_raw(cached as i64))
        }
    }
    
    /// Calculate the spread between best bid and ask
    #[inline]
    pub fn spread(&self) -> Option<Price> {
        match (self.best_ask(), self.best_bid()) {
            (Some(ask), Some(bid)) => Some(ask - bid),
            _ => None,
        }
    }
    
    /// Calculate the mid price
    #[inline]
    pub fn mid_price(&self) -> Option<Price> {
        match (self.best_ask(), self.best_bid()) {
            (Some(ask), Some(bid)) => Some((ask + bid) / 2.0),
            _ => None,
        }
    }
    
    /// Get total volume for a side
    #[inline]
    pub fn total_volume(&self, side: Side) -> Quantity {
        match side {
            Side::Buy => self.bids.iter()
                .map(|entry| entry.value().total_quantity())
                .fold(Quantity::ZERO, |acc, qty| acc + qty),
            Side::Sell => self.asks.iter()
                .map(|entry| entry.value().total_quantity())
                .fold(Quantity::ZERO, |acc, qty| acc + qty),
        }
    }
    
    /// Get market depth snapshot
    pub fn depth(&self, levels: usize) -> LockFreeBookSnapshot {
        let mut bids = Vec::with_capacity(levels);
        let mut asks = Vec::with_capacity(levels);
        
        // Collect bids (highest prices first)
        for entry in self.bids.iter().take(levels) {
            let price_level = entry.value();
            if !price_level.is_empty() {
                bids.push((price_level.price, price_level.total_quantity()));
            }
        }
        
        // Collect asks (lowest prices first)
        for entry in self.asks.iter().take(levels) {
            let price_level = entry.value();
            if !price_level.is_empty() {
                asks.push((price_level.price, price_level.total_quantity()));
            }
        }
        
        LockFreeBookSnapshot {
            symbol: self.symbol.clone(),
            bids,
            asks,
            timestamp: self.get_last_update_time(),
            sequence: self.sequence_number.load(Ordering::Acquire),
        }
    }
    
    /// Get statistics about the order book
    pub fn stats(&self) -> LockFreeOrderBookStats {
        LockFreeOrderBookStats {
            symbol: self.symbol.clone(),
            total_orders: self.orders.len() as u64,
            total_trades: self.total_trades.load(Ordering::Acquire),
            best_bid: self.best_bid(),
            best_ask: self.best_ask(),
            spread: self.spread(),
            sequence_number: self.sequence_number.load(Ordering::Acquire),
            last_update: self.get_last_update_time(),
        }
    }
    
    // Private implementation methods
    
    #[inline]
    fn match_order(&self, order: &mut Order) -> LockFreeMatchResult {
        let mut trades = Vec::with_capacity(4); // Pre-allocate for common case
        let mut remaining_qty = order.remaining_quantity();
        
        let can_match = |order_price: Price, level_price: Price, side: Side| -> bool {
            match side {
                Side::Buy => order_price >= level_price,
                Side::Sell => order_price <= level_price,
            }
        };
        
        let mut prices_to_remove = Vec::with_capacity(2);
        
        match order.side {
            Side::Buy => {
                // Match against asks (sells)
                for entry in self.asks.iter() {
                    if remaining_qty == Quantity::ZERO {
                        break;
                    }
                    
                    let level_price = *entry.key();
                    if !can_match(order.price, level_price, order.side) {
                        break;
                    }
                    
                    let price_level = entry.value();
                    
                    // Try to match orders at this price level
                    while remaining_qty > Quantity::ZERO && !price_level.is_empty() {
                        if let Some(matching_order_id) = price_level.front_order() {
                            if let Some(mut matching_order_entry) = self.orders.get_mut(&matching_order_id) {
                                let matching_order = matching_order_entry.value_mut();
                                let trade_qty = remaining_qty.min(matching_order.remaining_quantity());
                                
                                if trade_qty == Quantity::ZERO {
                                    price_level.pop_front_order();
                                    continue;
                                }
                                
                                // Create trade
                                trades.push(Trade::new(
                                    &order.symbol,
                                    order.id,
                                    matching_order.id,
                                    level_price,
                                    trade_qty,
                                    order.client_id,
                                    matching_order.client_id,
                                ));
                                
                                // Update orders
                                order.fill(trade_qty);
                                matching_order.fill(trade_qty);
                                remaining_qty -= trade_qty;
                                
                                // Update price level
                                price_level.reduce_quantity(trade_qty);
                                
                                if matching_order.is_fully_filled() {
                                    price_level.pop_front_order();
                                }
                            } else {
                                price_level.pop_front_order();
                            }
                        } else {
                            break;
                        }
                    }
                    
                    if price_level.is_empty() {
                        prices_to_remove.push(level_price);
                    }
                }
                
                // Remove empty price levels
                for price in prices_to_remove {
                    self.asks.remove(&price);
                    self.best_ask_dirty.store(true, Ordering::Release);
                }
            },
            Side::Sell => {
                // Match against bids (buys) - iterate in reverse for highest prices first
                for entry in self.bids.iter().rev() {
                    if remaining_qty == Quantity::ZERO {
                        break;
                    }
                    
                    let level_price = entry.key().0; // Unwrap Reverse
                    if !can_match(order.price, level_price, order.side) {
                        break;
                    }
                    
                    let price_level = entry.value();
                    
                    while remaining_qty > Quantity::ZERO && !price_level.is_empty() {
                        if let Some(matching_order_id) = price_level.front_order() {
                            if let Some(mut matching_order_entry) = self.orders.get_mut(&matching_order_id) {
                                let matching_order = matching_order_entry.value_mut();
                                let trade_qty = remaining_qty.min(matching_order.remaining_quantity());
                                
                                if trade_qty == Quantity::ZERO {
                                    price_level.pop_front_order();
                                    continue;
                                }
                                
                                let trade = Trade::new(
                                    &order.symbol,
                                    matching_order.id,
                                    order.id,
                                    level_price,
                                    trade_qty,
                                    matching_order.client_id,
                                    order.client_id,
                                );
                                
                                order.fill(trade_qty);
                                matching_order.fill(trade_qty);
                                remaining_qty -= trade_qty;
                                price_level.reduce_quantity(trade_qty);
                                trades.push(trade);
                                
                                if matching_order.is_fully_filled() {
                                    price_level.pop_front_order();
                                }
                            } else {
                                price_level.pop_front_order();
                            }
                        } else {
                            break;
                        }
                    }
                    
                    if price_level.is_empty() {
                        prices_to_remove.push(level_price);
                    }
                }
                
                // Remove empty price levels
                for price in prices_to_remove {
                    self.bids.remove(&std::cmp::Reverse(price));
                    self.best_bid_dirty.store(true, Ordering::Release);
                }
            }
        }
        
        // Update trade counter
        if !trades.is_empty() {
            self.total_trades.fetch_add(trades.len() as u64, Ordering::Relaxed);
        }
        
        // Return match result
        if trades.is_empty() {
            LockFreeMatchResult::NoMatch
        } else if remaining_qty > Quantity::ZERO {
            LockFreeMatchResult::PartialMatch {
                trades,
                remaining_quantity: remaining_qty,
            }
        } else {
            LockFreeMatchResult::FullMatch { trades }
        }
    }
    
    #[inline]
    fn insert_order_to_book(&self, order: &Order) {
        match order.side {
            Side::Buy => {
                let price_level = self.bids
                    .get_or_insert_with(std::cmp::Reverse(order.price), || {
                        Arc::new(AtomicPriceLevel::new(order.price))
                    })
                    .value()
                    .clone();
                
                price_level.add_order(order.id, order.remaining_quantity());
            },
            Side::Sell => {
                let price_level = self.asks
                    .get_or_insert_with(order.price, || {
                        Arc::new(AtomicPriceLevel::new(order.price))
                    })
                    .value()
                    .clone();
                
                price_level.add_order(order.id, order.remaining_quantity());
            }
        }
        
        self.sequence_number.fetch_add(1, Ordering::Relaxed);
    }
    
    #[inline]
    fn remove_order_from_book(&self, order: &Order) {
        match order.side {
            Side::Buy => {
                if let Some(entry) = self.bids.get(&std::cmp::Reverse(order.price)) {
                    let price_level = entry.value();
                    if price_level.remove_order(order.id, order.remaining_quantity()) {
                        if price_level.is_empty() {
                            self.bids.remove(&std::cmp::Reverse(order.price));
                            self.best_bid_dirty.store(true, Ordering::Release);
                        }
                    }
                }
            },
            Side::Sell => {
                if let Some(entry) = self.asks.get(&order.price) {
                    let price_level = entry.value();
                    if price_level.remove_order(order.id, order.remaining_quantity()) {
                        if price_level.is_empty() {
                            self.asks.remove(&order.price);
                            self.best_ask_dirty.store(true, Ordering::Release);
                        }
                    }
                }
            }
        }
    }
    
    #[inline]
    fn maybe_update_best_price_cache(&self, side: Side, price: Price) {
        match side {
            Side::Buy => {
                let cached_raw = self.best_bid_cache.load(Ordering::Acquire);
                let cached_price = if cached_raw == 0 {
                    Price::MIN
                } else {
                    Price::from_raw(cached_raw as i64)
                };
                
                if price > cached_price {
                    self.best_bid_cache.store(price.to_raw() as u64, Ordering::Release);
                    self.best_bid_dirty.store(false, Ordering::Release);
                }
            },
            Side::Sell => {
                let cached_raw = self.best_ask_cache.load(Ordering::Acquire);
                let cached_price = if cached_raw == u64::MAX {
                    Price::MAX
                } else {
                    Price::from_raw(cached_raw as i64)
                };
                
                if price < cached_price {
                    self.best_ask_cache.store(price.to_raw() as u64, Ordering::Release);
                    self.best_ask_dirty.store(false, Ordering::Release);
                }
            }
        }
    }
    
    #[inline]
    fn update_best_bid_cache(&self) {
        if let Some(entry) = self.bids.front() {
            let price = entry.key().0;
            self.best_bid_cache.store(price.to_raw() as u64, Ordering::Release);
        } else {
            self.best_bid_cache.store(0, Ordering::Release);
        }
        self.best_bid_dirty.store(false, Ordering::Release);
    }
    
    #[inline]
    fn update_best_ask_cache(&self) {
        if let Some(entry) = self.asks.front() {
            let price = *entry.key();
            self.best_ask_cache.store(price.to_raw() as u64, Ordering::Release);
        } else {
            self.best_ask_cache.store(u64::MAX, Ordering::Release);
        }
        self.best_ask_dirty.store(false, Ordering::Release);
    }
    
    #[inline]
    fn update_timestamp(&self) {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos() as u64;
        self.last_update_nanos.store(now, Ordering::Release);
    }
    
    #[inline]
    fn get_last_update_time(&self) -> DateTime<Utc> {
        let nanos = self.last_update_nanos.load(Ordering::Acquire);
        let secs = nanos / 1_000_000_000;
        let nanosecs = (nanos % 1_000_000_000) as u32;
        DateTime::from_timestamp(secs as i64, nanosecs).unwrap_or_else(Utc::now)
    }
}

// Safe to send between threads
unsafe impl Send for LockFreeOrderBook {}
unsafe impl Sync for LockFreeOrderBook {}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LockFreeBookSnapshot {
    pub symbol: String,
    pub bids: Vec<(Price, Quantity)>,
    pub asks: Vec<(Price, Quantity)>,
    pub timestamp: DateTime<Utc>,
    pub sequence: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LockFreeOrderBookStats {
    pub symbol: String,
    pub total_orders: u64,
    pub total_trades: u64,
    pub best_bid: Option<Price>,
    pub best_ask: Option<Price>,
    pub spread: Option<Price>,
    pub sequence_number: u64,
    pub last_update: DateTime<Utc>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::OrderType;
    use uuid::Uuid;
    use std::thread;

    fn create_test_order(
        symbol: &str,
        side: Side,
        price: f64,
        quantity: f64,
    ) -> Order {
        Order::new(
            symbol.to_string(),
            side,
            OrderType::Limit,
            Price::new(price),
            Quantity::new(quantity),
            Uuid::new_v4(),
        )
    }

    #[test]
    fn test_lockfree_order_book_creation() {
        let book = LockFreeOrderBook::new("BTCUSD".to_string());
        assert_eq!(book.symbol(), "BTCUSD");
        assert_eq!(book.best_bid(), None);
        assert_eq!(book.best_ask(), None);
        assert_eq!(book.spread(), None);
        assert_eq!(book.mid_price(), None);
    }

    #[test]
    fn test_add_single_order() {
        let book = LockFreeOrderBook::new("BTCUSD".to_string());
        let order = create_test_order("BTCUSD", Side::Buy, 50000.0, 1.0);
        let order_id = order.id;

        let result = book.add_order(order);
        
        assert!(matches!(result, LockFreeMatchResult::NoMatch));
        assert_eq!(book.best_bid(), Some(Price::new(50000.0)));
        assert_eq!(book.best_ask(), None);
        assert!(book.get_order(order_id).is_some());
    }

    #[test]
    fn test_order_matching() {
        let book = LockFreeOrderBook::new("BTCUSD".to_string());
        
        // Add a sell order first
        let sell_order = create_test_order("BTCUSD", Side::Sell, 50000.0, 1.0);
        book.add_order(sell_order);
        
        // Add a matching buy order
        let buy_order = create_test_order("BTCUSD", Side::Buy, 50000.0, 1.0);
        let result = book.add_order(buy_order);
        
        match result {
            LockFreeMatchResult::FullMatch { trades } => {
                assert_eq!(trades.len(), 1);
                assert_eq!(trades[0].price, Price::new(50000.0));
                assert_eq!(trades[0].quantity, Quantity::new(1.0));
            },
            _ => panic!("Expected full match"),
        }
        
        // After matching, both orders should be gone from the book
        assert_eq!(book.best_bid(), None);
        assert_eq!(book.best_ask(), None);
    }

    #[test]
    fn test_concurrent_operations() {
        let book = Arc::new(LockFreeOrderBook::new("BTCUSD".to_string()));
        let num_threads = 10;
        let orders_per_thread = 100;
        
        let handles: Vec<_> = (0..num_threads).map(|i| {
            let book = book.clone();
            thread::spawn(move || {
                for j in 0..orders_per_thread {
                    let price = 50000.0 + (i * orders_per_thread + j) as f64;
                    let order = create_test_order("BTCUSD", Side::Buy, price, 1.0);
                    book.add_order(order);
                }
            })
        }).collect();
        
        for handle in handles {
            handle.join().unwrap();
        }
        
        let stats = book.stats();
        assert_eq!(stats.total_orders, (num_threads * orders_per_thread) as u64);
    }

    #[test]
    fn test_market_depth() {
        let book = LockFreeOrderBook::new("BTCUSD".to_string());
        
        // Add multiple orders on both sides
        book.add_order(create_test_order("BTCUSD", Side::Buy, 49900.0, 1.0));
        book.add_order(create_test_order("BTCUSD", Side::Buy, 49950.0, 2.0));
        book.add_order(create_test_order("BTCUSD", Side::Sell, 50050.0, 1.0));
        book.add_order(create_test_order("BTCUSD", Side::Sell, 50100.0, 2.0));
        
        let snapshot = book.depth(5);
        
        assert_eq!(snapshot.bids.len(), 2);
        assert_eq!(snapshot.asks.len(), 2);
        
        // Highest bid should be first
        assert_eq!(snapshot.bids[0].0, Price::new(49950.0));
        assert_eq!(snapshot.bids[0].1, Quantity::new(2.0));
        
        // Lowest ask should be first
        assert_eq!(snapshot.asks[0].0, Price::new(50050.0));
        assert_eq!(snapshot.asks[0].1, Quantity::new(1.0));
    }

    #[test]
    fn test_best_price_cache() {
        let book = LockFreeOrderBook::new("BTCUSD".to_string());
        
        // Add orders to establish best prices
        book.add_order(create_test_order("BTCUSD", Side::Buy, 49950.0, 1.0));
        book.add_order(create_test_order("BTCUSD", Side::Buy, 49900.0, 1.0));
        book.add_order(create_test_order("BTCUSD", Side::Sell, 50050.0, 1.0));
        book.add_order(create_test_order("BTCUSD", Side::Sell, 50100.0, 1.0));
        
        assert_eq!(book.best_bid(), Some(Price::new(49950.0)));
        assert_eq!(book.best_ask(), Some(Price::new(50050.0)));
        
        // Add better prices
        book.add_order(create_test_order("BTCUSD", Side::Buy, 49960.0, 1.0));
        book.add_order(create_test_order("BTCUSD", Side::Sell, 50040.0, 1.0));
        
        assert_eq!(book.best_bid(), Some(Price::new(49960.0)));
        assert_eq!(book.best_ask(), Some(Price::new(50040.0)));
    }
}