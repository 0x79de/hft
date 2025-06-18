use crate::types::{Price, Quantity, OrderId};
use serde::{Deserialize, Serialize};
use std::collections::VecDeque;
use std::fmt;
use std::sync::atomic::{AtomicU64, Ordering};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[repr(C, align(64))]
pub struct PriceLevel {
    pub price: Price,
    pub total_quantity: Quantity,
    pub order_count: u32,
    orders: VecDeque<OrderId>,
}

impl PriceLevel {
    #[inline]
    pub fn new(price: Price) -> Self {
        Self {
            price,
            total_quantity: Quantity::ZERO,
            order_count: 0,
            orders: VecDeque::with_capacity(16),
        }
    }
    
    #[inline]
    pub fn add_order(&mut self, order_id: OrderId, quantity: Quantity) {
        self.orders.push_back(order_id);
        self.total_quantity += quantity;
        self.order_count += 1;
    }
    
    #[inline]
    pub fn remove_order(&mut self, order_id: OrderId, quantity: Quantity) -> bool {
        if let Some(pos) = self.orders.iter().position(|&id| id == order_id) {
            self.orders.remove(pos);
            self.total_quantity -= quantity;
            self.order_count -= 1;
            true
        } else {
            false
        }
    }
    
    #[inline]
    pub fn front_order(&self) -> Option<OrderId> {
        self.orders.front().copied()
    }
    
    #[inline]
    pub fn pop_front_order(&mut self) -> Option<OrderId> {
        let order_id = self.orders.pop_front();
        if order_id.is_some() {
            self.order_count = self.order_count.saturating_sub(1);
        }
        order_id
    }
    
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.orders.is_empty()
    }
    
    #[inline]
    pub fn len(&self) -> usize {
        self.orders.len()
    }
    
    #[inline]
    pub fn reduce_quantity(&mut self, quantity: Quantity) {
        self.total_quantity -= quantity;
    }
    
    #[inline]
    pub fn orders(&self) -> &VecDeque<OrderId> {
        &self.orders
    }
}

impl fmt::Display for PriceLevel {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}@{} ({})", self.total_quantity, self.price, self.order_count)
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OrderInfo {
    pub order_id: OrderId,
    pub quantity: Quantity,
    pub timestamp: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug)]
pub struct AtomicPriceLevel {
    pub price: Price,
    pub total_quantity: AtomicU64,
    pub order_count: AtomicU64,
    orders: parking_lot::RwLock<VecDeque<OrderInfo>>,
}

impl AtomicPriceLevel {
    pub fn new(price: Price) -> Self {
        Self {
            price,
            total_quantity: AtomicU64::new(0),
            order_count: AtomicU64::new(0),
            orders: parking_lot::RwLock::new(VecDeque::with_capacity(16)),
        }
    }
    
    pub fn add_order(&self, order_id: OrderId, quantity: Quantity) {
        let order_info = OrderInfo {
            order_id,
            quantity,
            timestamp: chrono::Utc::now(),
        };
        
        self.orders.write().push_back(order_info);
        self.total_quantity.fetch_add(quantity.to_raw(), Ordering::Relaxed);
        self.order_count.fetch_add(1, Ordering::Relaxed);
    }
}