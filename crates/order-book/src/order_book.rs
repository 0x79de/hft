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
        let match_result = self.match_order(&mut order);
        
        if order.remaining_quantity() > Quantity::ZERO {
            self.insert_order_to_book(&order);
            self.orders.insert(order.id, order);
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
        
        for entry in self.bids.iter().rev().take(levels) {
            let price_level = entry.value().read();
            bids.push((price_level.price, price_level.total_quantity));
        }
        
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
    
    fn match_order(&self, order: &mut Order) -> MatchResult {
        let mut trades = Vec::new();
        let mut remaining_qty = order.remaining_quantity();
        
        let can_match = |order_price: Price, level_price: Price, side: Side| -> bool {
            match side {
                Side::Buy => order_price >= level_price,
                Side::Sell => order_price <= level_price,
            }
        };
        
        let mut prices_to_remove = Vec::new();
        
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
                    
                    while remaining_qty > Quantity::ZERO && !price_level.is_empty() {
                        if let Some(matching_order_id) = price_level.front_order() {
                            if let Some(mut matching_order_entry) = self.orders.get_mut(&matching_order_id) {
                                let matching_order = matching_order_entry.value_mut();
                                let trade_qty = remaining_qty.min(matching_order.remaining_quantity());
                                
                                let trade = Trade::new(
                                    order.symbol.clone(),
                                    order.id,
                                    matching_order.id,
                                    level_price,
                                    trade_qty,
                                    order.client_id,
                                    matching_order.client_id,
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
                                
                                let trade = Trade::new(
                                    order.symbol.clone(),
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