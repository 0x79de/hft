use crate::types::{Price, Quantity, Order, OrderId, Side, Trade};
use crate::price_level::PriceLevel;
use crossbeam_skiplist::SkipMap;
use dashmap::DashMap;
use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use std::sync::atomic::AtomicU64;
use chrono::{DateTime, Utc};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum OrderBookError {
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
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrderBookStats {
    pub total_orders: u64,
    pub total_volume: Quantity,
    pub best_bid: Option<Price>,
    pub best_ask: Option<Price>,
    pub spread: Option<Price>,
    pub depth_levels: usize,
    pub last_update: DateTime<Utc>,
}

#[derive(Debug)]
pub struct OrderBook {
    symbol: String,
    bids: SkipMap<std::cmp::Reverse<Price>, Arc<RwLock<PriceLevel>>>,
    asks: SkipMap<Price, Arc<RwLock<PriceLevel>>>,
    orders: DashMap<OrderId, Order>,
    best_bid_cache: Arc<RwLock<Option<Price>>>,
    best_ask_cache: Arc<RwLock<Option<Price>>>,
    #[allow(dead_code)]
    sequence_number: AtomicU64,
    _last_update: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BookSnapshot {
    pub symbol: String,
    pub bids: Vec<(Price, Quantity)>,
    pub asks: Vec<(Price, Quantity)>,
    pub timestamp: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum MatchResult {
    NoMatch,
    PartialMatch {
        trades: Vec<Trade>,
        remaining_quantity: Quantity,
    },
    FullMatch {
        trades: Vec<Trade>,
    },
}

impl OrderBook {
    #[inline]
    pub fn new(symbol: String) -> Self {
        Self {
            symbol,
            bids: SkipMap::new(),
            asks: SkipMap::new(),
            orders: DashMap::new(),
            best_bid_cache: Arc::new(RwLock::new(None)),
            best_ask_cache: Arc::new(RwLock::new(None)),
            sequence_number: AtomicU64::new(0),
            _last_update: Utc::now(),
        }
    }
    
    #[inline]
    pub fn symbol(&self) -> &str {
        &self.symbol
    }
    
    #[inline]
    pub fn add_order(&self, mut order: Order) -> MatchResult {
        // Fast path for market orders that will likely match completely
        let match_result = self.match_order(&mut order);
        
        if order.remaining_quantity() > Quantity::ZERO {
            self.insert_order_to_book(&order);
            self.orders.insert(order.id, order);
            // Only update cache if we added to book
            self.update_best_price_cache();
        }
        
        match_result
    }
    
    #[inline]
    pub fn cancel_order(&self, order_id: OrderId) -> Option<Order> {
        if let Some((_, mut order)) = self.orders.remove(&order_id) {
            order.cancel();
            self.remove_order_from_book(&order);
            Some(order)
        } else {
            None
        }
    }
    
    #[inline]
    pub fn get_order(&self, order_id: OrderId) -> Option<Order> {
        self.orders.get(&order_id).map(|entry| entry.clone())
    }
    
    #[inline]
    pub fn best_bid(&self) -> Option<Price> {
        if let Some(cached) = *self.best_bid_cache.read() {
            Some(cached)
        } else {
            self.bids.front().map(|entry| entry.key().0)
        }
    }
    
    #[inline]
    pub fn best_ask(&self) -> Option<Price> {
        if let Some(cached) = *self.best_ask_cache.read() {
            Some(cached)
        } else {
            self.asks.front().map(|entry| *entry.key())
        }
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
    
    #[inline]
    pub fn depth(&self, levels: usize) -> BookSnapshot {
        let mut bids = Vec::with_capacity(levels);
        let mut asks = Vec::with_capacity(levels);
        
        // For bids, we want highest prices first (bids are stored as Reverse(Price))
        for entry in self.bids.iter().take(levels) {
            let price_level = entry.value().read();
            bids.push((price_level.price, price_level.total_quantity));
        }
        
        // For asks, we want lowest prices first
        for entry in self.asks.iter().take(levels) {
            let price_level = entry.value().read();
            asks.push((price_level.price, price_level.total_quantity));
        }
        
        BookSnapshot {
            symbol: self.symbol.clone(),
            bids,
            asks,
            timestamp: Utc::now(),
        }
    }
    
    #[inline]
    pub fn total_volume(&self, side: Side) -> Quantity {
        match side {
            Side::Buy => self.bids.iter()
                .map(|entry| entry.value().read().total_quantity)
                .fold(Quantity::ZERO, |acc, qty| acc + qty),
            Side::Sell => self.asks.iter()
                .map(|entry| entry.value().read().total_quantity)
                .fold(Quantity::ZERO, |acc, qty| acc + qty),
        }
    }
    
    #[inline]
    fn update_best_price_cache(&self) {
        // Batch cache updates to reduce lock contention
        let best_bid = self.bids.front().map(|entry| entry.key().0);
        let best_ask = self.asks.front().map(|entry| *entry.key());
        
        // Single write lock for both updates
        *self.best_bid_cache.write() = best_bid;
        *self.best_ask_cache.write() = best_ask;
    }

    fn match_order(&self, order: &mut Order) -> MatchResult {
        let mut trades = Vec::with_capacity(4); // Pre-allocate for common case
        let mut remaining_qty = order.remaining_quantity();
        
        let can_match = |order_price: Price, level_price: Price, side: Side| -> bool {
            match side {
                Side::Buy => order_price >= level_price,
                Side::Sell => order_price <= level_price,
            }
        };
        
        let mut prices_to_remove = Vec::with_capacity(2); // Pre-allocate for common case
        
        match order.side {
            Side::Buy => {
                // For buy orders, match against asks (sells)
                for entry in self.asks.iter() {
                    if remaining_qty == Quantity::ZERO {
                        break;
                    }
                    
                    let level_price = *entry.key();
                    if !can_match(order.price, level_price, order.side) {
                        break;
                    }
                    
                    let mut price_level = entry.value().write();
                    
                    // Optimized matching loop - minimize allocations and checks
                    while remaining_qty > Quantity::ZERO && !price_level.is_empty() {
                        if let Some(matching_order_id) = price_level.front_order() {
                            if let Some(mut matching_order_entry) = self.orders.get_mut(&matching_order_id) {
                                let matching_order = matching_order_entry.value_mut();
                                let trade_qty = remaining_qty.min(matching_order.remaining_quantity());
                                
                                // Skip zero-quantity trades
                                if trade_qty == Quantity::ZERO {
                                    price_level.pop_front_order();
                                    continue;
                                }
                                
                                // Create trade with minimal allocations
                                trades.push(Trade::new(
                                    &order.symbol,
                                    order.id,
                                    matching_order.id,
                                    level_price,
                                    trade_qty,
                                    order.client_id,
                                    matching_order.client_id,
                                ));
                                
                                // Batch updates
                                order.fill(trade_qty);
                                matching_order.fill(trade_qty);
                                remaining_qty -= trade_qty;
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
                
                for price in prices_to_remove {
                    self.asks.remove(&price);
                }
            },
            Side::Sell => {
                // For sell orders, match against bids (buys)
                for entry in self.bids.iter().rev() {
                    if remaining_qty == Quantity::ZERO {
                        break;
                    }
                    
                    let level_price = entry.key().0; // Unwrap Reverse
                    if !can_match(order.price, level_price, order.side) {
                        break;
                    }
                    
                    let mut price_level = entry.value().write();
                    
                    while remaining_qty > Quantity::ZERO && !price_level.is_empty() {
                        if let Some(matching_order_id) = price_level.front_order() {
                            if let Some(mut matching_order_entry) = self.orders.get_mut(&matching_order_id) {
                                let matching_order = matching_order_entry.value_mut();
                                let trade_qty = remaining_qty.min(matching_order.remaining_quantity());
                                
                                // Skip zero-quantity trades
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
                
                for price in prices_to_remove {
                    self.bids.remove(&std::cmp::Reverse(price));
                }
            }
        }
        
        // Update cache after matching
        self.update_best_price_cache();
        
        if trades.is_empty() {
            MatchResult::NoMatch
        } else if remaining_qty > Quantity::ZERO {
            MatchResult::PartialMatch {
                trades,
                remaining_quantity: remaining_qty,
            }
        } else {
            MatchResult::FullMatch { trades }
        }
    }
    
    fn insert_order_to_book(&self, order: &Order) {
        match order.side {
            Side::Buy => {
                let price_level = self.bids
                    .get_or_insert_with(std::cmp::Reverse(order.price), || Arc::new(RwLock::new(PriceLevel::new(order.price))))
                    .value()
                    .clone();
                
                price_level.write().add_order(order.id, order.remaining_quantity());
            },
            Side::Sell => {
                let price_level = self.asks
                    .get_or_insert_with(order.price, || Arc::new(RwLock::new(PriceLevel::new(order.price))))
                    .value()
                    .clone();
                
                price_level.write().add_order(order.id, order.remaining_quantity());
            }
        }
    }
    
    fn remove_order_from_book(&self, order: &Order) {
        match order.side {
            Side::Buy => {
                if let Some(entry) = self.bids.get(&std::cmp::Reverse(order.price)) {
                    let mut price_level = entry.value().write();
                    if price_level.remove_order(order.id, order.remaining_quantity()) && price_level.is_empty() {
                        drop(price_level);
                        self.bids.remove(&std::cmp::Reverse(order.price));
                    }
                }
            },
            Side::Sell => {
                if let Some(entry) = self.asks.get(&order.price) {
                    let mut price_level = entry.value().write();
                    if price_level.remove_order(order.id, order.remaining_quantity()) && price_level.is_empty() {
                        drop(price_level);
                        self.asks.remove(&order.price);
                    }
                }
            }
        }
        
        // Update cache after removing order from book
        self.update_best_price_cache();
    }
}

impl Clone for OrderBook {
    fn clone(&self) -> Self {
        let new_book = Self::new(self.symbol.clone());
        
        for entry in self.orders.iter() {
            let order = entry.value().clone();
            new_book.orders.insert(*entry.key(), order.clone());
            new_book.insert_order_to_book(&order);
        }
        
        new_book
    }
}

unsafe impl Send for OrderBook {}
unsafe impl Sync for OrderBook {}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{OrderType, OrderStatus};
    use uuid::Uuid;

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
    fn test_order_book_creation() {
        let book = OrderBook::new("BTCUSD".to_string());
        assert_eq!(book.symbol(), "BTCUSD");
        assert_eq!(book.best_bid(), None);
        assert_eq!(book.best_ask(), None);
        assert_eq!(book.spread(), None);
        assert_eq!(book.mid_price(), None);
    }

    #[test]
    fn test_add_single_order() {
        let book = OrderBook::new("BTCUSD".to_string());
        let order = create_test_order("BTCUSD", Side::Buy, 50000.0, 1.0);
        let order_id = order.id;

        let result = book.add_order(order);
        
        assert!(matches!(result, MatchResult::NoMatch));
        assert_eq!(book.best_bid(), Some(Price::new(50000.0)));
        assert_eq!(book.best_ask(), None);
        assert!(book.get_order(order_id).is_some());
    }

    #[test]
    fn test_add_orders_same_side() {
        let book = OrderBook::new("BTCUSD".to_string());
        
        let order1 = create_test_order("BTCUSD", Side::Buy, 50000.0, 1.0);
        let order2 = create_test_order("BTCUSD", Side::Buy, 50100.0, 1.0);
        let order3 = create_test_order("BTCUSD", Side::Buy, 49900.0, 1.0);

        book.add_order(order1);
        book.add_order(order2);
        book.add_order(order3);

        // Best bid should be the highest price
        assert_eq!(book.best_bid(), Some(Price::new(50100.0)));
        assert_eq!(book.total_volume(Side::Buy), Quantity::new(3.0));
    }

    #[test]
    fn test_order_matching_full() {
        let book = OrderBook::new("BTCUSD".to_string());
        
        // Add a sell order first
        let sell_order = create_test_order("BTCUSD", Side::Sell, 50000.0, 1.0);
        book.add_order(sell_order);
        
        // Add a matching buy order
        let buy_order = create_test_order("BTCUSD", Side::Buy, 50000.0, 1.0);
        let result = book.add_order(buy_order);
        
        match result {
            MatchResult::FullMatch { trades } => {
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
    fn test_order_matching_partial() {
        let book = OrderBook::new("BTCUSD".to_string());
        
        // Add a larger sell order first
        let sell_order = create_test_order("BTCUSD", Side::Sell, 50000.0, 2.0);
        book.add_order(sell_order);
        
        // Add a smaller matching buy order
        let buy_order = create_test_order("BTCUSD", Side::Buy, 50000.0, 1.0);
        let result = book.add_order(buy_order);
        
        match result {
            MatchResult::FullMatch { trades } => {
                assert_eq!(trades.len(), 1);
                assert_eq!(trades[0].quantity, Quantity::new(1.0));
            },
            _ => panic!("Expected full match for buy order"),
        }
        
        // The sell order should still be in the book with reduced quantity
        assert_eq!(book.best_ask(), Some(Price::new(50000.0)));
        assert_eq!(book.total_volume(Side::Sell), Quantity::new(1.0));
    }

    #[test]
    fn test_order_matching_aggressive_buy() {
        let book = OrderBook::new("BTCUSD".to_string());
        
        // Add multiple sell orders at different prices
        let sell1 = create_test_order("BTCUSD", Side::Sell, 50000.0, 0.5);
        let sell2 = create_test_order("BTCUSD", Side::Sell, 50100.0, 0.5);
        let sell3 = create_test_order("BTCUSD", Side::Sell, 50200.0, 1.0);
        
        book.add_order(sell1);
        book.add_order(sell2);
        book.add_order(sell3);
        
        // Add aggressive buy order that matches exactly the first two levels
        let buy_order = create_test_order("BTCUSD", Side::Buy, 50150.0, 1.0);
        let result = book.add_order(buy_order);
        
        match result {
            MatchResult::FullMatch { trades } => {
                assert_eq!(trades.len(), 2); // Should match first two levels
                assert_eq!(trades[0].price, Price::new(50000.0));
                assert_eq!(trades[1].price, Price::new(50100.0));
                assert_eq!(trades[0].quantity, Quantity::new(0.5));
                assert_eq!(trades[1].quantity, Quantity::new(0.5));
            },
            _ => panic!("Expected full match"),
        }
        
        // Only the third sell order should remain
        assert_eq!(book.best_ask(), Some(Price::new(50200.0)));
    }

    #[test]
    fn test_order_cancellation() {
        let book = OrderBook::new("BTCUSD".to_string());
        
        let order = create_test_order("BTCUSD", Side::Buy, 50000.0, 1.0);
        let order_id = order.id;
        
        book.add_order(order);
        assert_eq!(book.best_bid(), Some(Price::new(50000.0)));
        
        let cancelled_order = book.cancel_order(order_id);
        assert!(cancelled_order.is_some());
        assert_eq!(cancelled_order.unwrap().status, OrderStatus::Cancelled);
        assert_eq!(book.best_bid(), None);
    }

    #[test]
    fn test_market_depth() {
        let book = OrderBook::new("BTCUSD".to_string());
        
        // Add multiple orders on both sides
        book.add_order(create_test_order("BTCUSD", Side::Buy, 49900.0, 1.0));
        book.add_order(create_test_order("BTCUSD", Side::Buy, 49950.0, 2.0));
        book.add_order(create_test_order("BTCUSD", Side::Buy, 49900.0, 1.5)); // Same price
        
        book.add_order(create_test_order("BTCUSD", Side::Sell, 50050.0, 1.0));
        book.add_order(create_test_order("BTCUSD", Side::Sell, 50100.0, 2.0));
        
        let snapshot = book.depth(5);
        
        assert_eq!(snapshot.bids.len(), 2); // Two price levels
        assert_eq!(snapshot.asks.len(), 2);
        
        // Highest bid should be first
        assert_eq!(snapshot.bids[0].0, Price::new(49950.0));
        assert_eq!(snapshot.bids[0].1, Quantity::new(2.0));
        
        assert_eq!(snapshot.bids[1].0, Price::new(49900.0));
        assert_eq!(snapshot.bids[1].1, Quantity::new(2.5)); // Combined quantity
        
        // Lowest ask should be first
        assert_eq!(snapshot.asks[0].0, Price::new(50050.0));
        assert_eq!(snapshot.asks[0].1, Quantity::new(1.0));
    }

    #[test]
    fn test_spread_and_mid_price() {
        let book = OrderBook::new("BTCUSD".to_string());
        
        book.add_order(create_test_order("BTCUSD", Side::Buy, 49950.0, 1.0));
        book.add_order(create_test_order("BTCUSD", Side::Sell, 50050.0, 1.0));
        
        assert_eq!(book.spread(), Some(Price::new(100.0)));
        assert_eq!(book.mid_price(), Some(Price::new(50000.0)));
    }

    #[test]
    fn test_volume_calculation() {
        let book = OrderBook::new("BTCUSD".to_string());
        
        book.add_order(create_test_order("BTCUSD", Side::Buy, 49950.0, 1.0));
        book.add_order(create_test_order("BTCUSD", Side::Buy, 49950.0, 2.0));
        book.add_order(create_test_order("BTCUSD", Side::Buy, 49900.0, 1.5));
        
        book.add_order(create_test_order("BTCUSD", Side::Sell, 50050.0, 0.5));
        book.add_order(create_test_order("BTCUSD", Side::Sell, 50100.0, 2.5));
        
        assert_eq!(book.total_volume(Side::Buy), Quantity::new(4.5));
        assert_eq!(book.total_volume(Side::Sell), Quantity::new(3.0));
    }

    #[test]
    fn test_order_book_clone() {
        let book = OrderBook::new("BTCUSD".to_string());
        
        book.add_order(create_test_order("BTCUSD", Side::Buy, 49950.0, 1.0));
        book.add_order(create_test_order("BTCUSD", Side::Sell, 50050.0, 1.0));
        
        let cloned_book = book.clone();
        
        assert_eq!(cloned_book.symbol(), book.symbol());
        assert_eq!(cloned_book.best_bid(), book.best_bid());
        assert_eq!(cloned_book.best_ask(), book.best_ask());
        assert_eq!(cloned_book.total_volume(Side::Buy), book.total_volume(Side::Buy));
        assert_eq!(cloned_book.total_volume(Side::Sell), book.total_volume(Side::Sell));
    }

    #[test]
    fn test_no_self_matching() {
        let book = OrderBook::new("BTCUSD".to_string());
        let client_id = Uuid::new_v4();
        
        // Add a sell order
        let sell_order = Order::new(
            "BTCUSD".to_string(),
            Side::Sell,
            OrderType::Limit,
            Price::new(50000.0),
            Quantity::new(1.0),
            client_id,
        );
        book.add_order(sell_order);
        
        // Try to add a buy order from the same client - this should work in our simple implementation
        // In a real system, you might want to prevent self-matching
        let buy_order = Order::new(
            "BTCUSD".to_string(),
            Side::Buy,
            OrderType::Limit,
            Price::new(50000.0),
            Quantity::new(1.0),
            client_id,
        );
        
        let result = book.add_order(buy_order);
        
        // This will match in our implementation - we're not preventing self-matching
        assert!(matches!(result, MatchResult::FullMatch { .. }));
    }

    #[test]
    fn test_price_priority() {
        let book = OrderBook::new("BTCUSD".to_string());
        
        // Add sell orders at different prices
        let sell1 = create_test_order("BTCUSD", Side::Sell, 50100.0, 1.0);
        let sell2 = create_test_order("BTCUSD", Side::Sell, 50000.0, 1.0); // Better price
        let sell3 = create_test_order("BTCUSD", Side::Sell, 50200.0, 1.0);
        
        book.add_order(sell1);
        book.add_order(sell2);
        book.add_order(sell3);
        
        // Best ask should be the lowest price
        assert_eq!(book.best_ask(), Some(Price::new(50000.0)));
        
        // Add a buy order that matches
        let buy_order = create_test_order("BTCUSD", Side::Buy, 50000.0, 1.0);
        let result = book.add_order(buy_order);
        
        match result {
            MatchResult::FullMatch { trades } => {
                assert_eq!(trades.len(), 1);
                assert_eq!(trades[0].price, Price::new(50000.0)); // Should match best price
            },
            _ => panic!("Expected full match"),
        }
        
        // After matching, next best should be 50100
        assert_eq!(book.best_ask(), Some(Price::new(50100.0)));
    }

    #[test]
    fn test_empty_book_operations() {
        let book = OrderBook::new("BTCUSD".to_string());
        
        assert_eq!(book.cancel_order(OrderId::from_raw(999)), None);
        assert_eq!(book.get_order(OrderId::from_raw(999)), None);
        assert_eq!(book.total_volume(Side::Buy), Quantity::ZERO);
        assert_eq!(book.total_volume(Side::Sell), Quantity::ZERO);
        
        let snapshot = book.depth(10);
        assert!(snapshot.bids.is_empty());
        assert!(snapshot.asks.is_empty());
    }
}