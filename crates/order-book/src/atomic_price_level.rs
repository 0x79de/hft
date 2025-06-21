use crate::types::{Price, Quantity, OrderId};
use crossbeam_queue::SegQueue;
use std::sync::atomic::{AtomicU64, AtomicU32, AtomicBool, Ordering};

/// Cache-aligned atomic price level for lock-free order book operations
#[derive(Debug)]
#[repr(C, align(64))]
pub struct AtomicPriceLevel {
    /// Fixed price for this level
    pub price: Price,
    /// Total quantity at this price level
    total_quantity: AtomicU64,
    /// Number of orders at this price level
    order_count: AtomicU32,
    /// Lock-free queue for order IDs (FIFO ordering)
    orders: SegQueue<OrderId>,
    /// Flag to indicate if level is being modified
    modification_flag: AtomicBool,
    /// Padding to prevent false sharing
    _padding: [u8; 32],
}

impl AtomicPriceLevel {
    /// Create a new atomic price level
    #[inline]
    pub fn new(price: Price) -> Self {
        Self {
            price,
            total_quantity: AtomicU64::new(0),
            order_count: AtomicU32::new(0),
            orders: SegQueue::new(),
            modification_flag: AtomicBool::new(false),
            _padding: [0; 32],
        }
    }
    
    /// Add an order to this price level atomically
    #[inline]
    pub fn add_order(&self, order_id: OrderId, quantity: Quantity) -> bool {
        // Mark level as being modified
        self.modification_flag.store(true, Ordering::Release);
        
        // Add order to queue first (this is lock-free)
        self.orders.push(order_id);
        
        // Update counters atomically
        self.total_quantity.fetch_add(quantity.to_raw(), Ordering::AcqRel);
        self.order_count.fetch_add(1, Ordering::AcqRel);
        
        // Clear modification flag
        self.modification_flag.store(false, Ordering::Release);
        
        true
    }
    
    /// Remove a specific order from this price level atomically
    #[inline]
    pub fn remove_order(&self, _order_id: OrderId, quantity: Quantity) -> bool {
        // This is more complex for SegQueue as it doesn't support arbitrary removal
        // For now, we'll use a different approach - mark orders as removed
        // In practice, we'd use a different data structure like a lock-free list
        
        // Optimistically update counters
        let current_qty = self.total_quantity.load(Ordering::Acquire);
        if current_qty < quantity.to_raw() {
            return false; // Insufficient quantity
        }
        
        loop {
            let current_qty = self.total_quantity.load(Ordering::Acquire);
            if current_qty < quantity.to_raw() {
                return false;
            }
            
            match self.total_quantity.compare_exchange_weak(
                current_qty,
                current_qty - quantity.to_raw(),
                Ordering::AcqRel,
                Ordering::Relaxed
            ) {
                Ok(_) => {
                    self.order_count.fetch_sub(1, Ordering::AcqRel);
                    return true;
                }
                Err(_) => continue, // Retry CAS
            }
        }
    }
    
    /// Get the front order ID without removing it
    #[inline]
    pub fn front_order(&self) -> Option<OrderId> {
        // SegQueue doesn't have a peek operation, so we simulate it
        // by popping and immediately pushing back
        if let Some(order_id) = self.orders.pop() {
            // Push it back to maintain FIFO order
            // Note: This is not perfect as another thread could intervene
            self.orders.push(order_id);
            Some(order_id)
        } else {
            None
        }
    }
    
    /// Remove and return the front order
    #[inline]
    pub fn pop_front_order(&self) -> Option<OrderId> {
        if let Some(order_id) = self.orders.pop() {
            self.order_count.fetch_sub(1, Ordering::AcqRel);
            Some(order_id)
        } else {
            None
        }
    }
    
    /// Check if this price level is empty
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.order_count.load(Ordering::Acquire) == 0
    }
    
    /// Get the total quantity at this price level
    #[inline]
    pub fn total_quantity(&self) -> Quantity {
        Quantity::from_raw(self.total_quantity.load(Ordering::Acquire))
    }
    
    /// Get the number of orders at this price level
    #[inline]
    pub fn order_count(&self) -> u32 {
        self.order_count.load(Ordering::Acquire)
    }
    
    /// Reduce the total quantity by the specified amount
    #[inline]
    pub fn reduce_quantity(&self, quantity: Quantity) -> bool {
        loop {
            let current = self.total_quantity.load(Ordering::Acquire);
            let quantity_raw = quantity.to_raw();
            
            if current < quantity_raw {
                return false; // Insufficient quantity
            }
            
            match self.total_quantity.compare_exchange_weak(
                current,
                current - quantity_raw,
                Ordering::AcqRel,
                Ordering::Relaxed
            ) {
                Ok(_) => return true,
                Err(_) => continue, // Retry
            }
        }
    }
    
    /// Check if the level is currently being modified
    #[inline]
    pub fn is_being_modified(&self) -> bool {
        self.modification_flag.load(Ordering::Acquire)
    }
}

impl Clone for AtomicPriceLevel {
    fn clone(&self) -> Self {
        let new_level = Self::new(self.price);
        
        // Copy atomic values
        new_level.total_quantity.store(
            self.total_quantity.load(Ordering::Acquire),
            Ordering::Release
        );
        new_level.order_count.store(
            self.order_count.load(Ordering::Acquire),
            Ordering::Release
        );
        
        // Copy orders (this is a snapshot, not exact due to concurrent access)
        let mut orders_to_copy = Vec::new();
        while let Some(order_id) = self.orders.pop() {
            orders_to_copy.push(order_id);
        }
        
        // Push orders back to original and add to new level
        for order_id in orders_to_copy.iter().rev() {
            self.orders.push(*order_id);
        }
        for order_id in orders_to_copy {
            new_level.orders.push(order_id);
        }
        
        new_level
    }
}

// Safe to send between threads
unsafe impl Send for AtomicPriceLevel {}
unsafe impl Sync for AtomicPriceLevel {}

/// Enhanced lock-free order queue optimized for FIFO operations
pub struct LockFreeOrderQueue {
    queue: SegQueue<OrderInfo>,
    total_quantity: AtomicU64,
    order_count: AtomicU32,
}

#[derive(Debug, Clone, PartialEq)]
pub struct OrderInfo {
    pub order_id: OrderId,
    pub quantity: Quantity,
    pub timestamp_nanos: u64, // Using nanoseconds for better precision
}

impl LockFreeOrderQueue {
    pub fn new() -> Self {
        Self {
            queue: SegQueue::new(),
            total_quantity: AtomicU64::new(0),
            order_count: AtomicU32::new(0),
        }
    }
    
    pub fn push(&self, order_info: OrderInfo) {
        self.total_quantity.fetch_add(order_info.quantity.to_raw(), Ordering::AcqRel);
        self.order_count.fetch_add(1, Ordering::AcqRel);
        self.queue.push(order_info);
    }
    
    pub fn pop(&self) -> Option<OrderInfo> {
        if let Some(order_info) = self.queue.pop() {
            self.total_quantity.fetch_sub(order_info.quantity.to_raw(), Ordering::AcqRel);
            self.order_count.fetch_sub(1, Ordering::AcqRel);
            Some(order_info)
        } else {
            None
        }
    }
    
    pub fn is_empty(&self) -> bool {
        self.order_count.load(Ordering::Acquire) == 0
    }
    
    pub fn total_quantity(&self) -> Quantity {
        Quantity::from_raw(self.total_quantity.load(Ordering::Acquire))
    }
    
    pub fn len(&self) -> u32 {
        self.order_count.load(Ordering::Acquire)
    }
}

impl Default for LockFreeOrderQueue {
    fn default() -> Self {
        Self::new()
    }
}

unsafe impl Send for LockFreeOrderQueue {}
unsafe impl Sync for LockFreeOrderQueue {}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use std::thread;

    #[test]
    fn test_atomic_price_level_creation() {
        let price = Price::new(100.0);
        let level = AtomicPriceLevel::new(price);
        
        assert_eq!(level.price, price);
        assert_eq!(level.total_quantity(), Quantity::ZERO);
        assert_eq!(level.order_count(), 0);
        assert!(level.is_empty());
    }
    
    #[test]
    fn test_add_order() {
        let level = AtomicPriceLevel::new(Price::new(100.0));
        let order_id = OrderId::new();
        let quantity = Quantity::new(10.0);
        
        assert!(level.add_order(order_id, quantity));
        assert_eq!(level.total_quantity(), quantity);
        assert_eq!(level.order_count(), 1);
        assert!(!level.is_empty());
    }
    
    #[test]
    fn test_remove_order() {
        let level = AtomicPriceLevel::new(Price::new(100.0));
        let order_id = OrderId::new();
        let quantity = Quantity::new(10.0);
        
        level.add_order(order_id, quantity);
        assert!(level.remove_order(order_id, quantity));
        assert_eq!(level.total_quantity(), Quantity::ZERO);
        assert_eq!(level.order_count(), 0);
    }
    
    #[test]
    fn test_reduce_quantity() {
        let level = AtomicPriceLevel::new(Price::new(100.0));
        let order_id = OrderId::new();
        let quantity = Quantity::new(10.0);
        
        level.add_order(order_id, quantity);
        assert!(level.reduce_quantity(Quantity::new(5.0)));
        assert_eq!(level.total_quantity(), Quantity::new(5.0));
        assert_eq!(level.order_count(), 1); // Order count doesn't change
    }
    
    #[test]
    fn test_concurrent_operations() {
        let level = Arc::new(AtomicPriceLevel::new(Price::new(100.0)));
        let num_threads = 10;
        let orders_per_thread = 100;
        
        let handles: Vec<_> = (0..num_threads).map(|_| {
            let level = level.clone();
            thread::spawn(move || {
                for _ in 0..orders_per_thread {
                    let order_id = OrderId::new();
                    let quantity = Quantity::new(1.0);
                    level.add_order(order_id, quantity);
                }
            })
        }).collect();
        
        for handle in handles {
            handle.join().unwrap();
        }
        
        let expected_quantity = Quantity::new((num_threads * orders_per_thread) as f64);
        assert_eq!(level.total_quantity(), expected_quantity);
        assert_eq!(level.order_count(), num_threads * orders_per_thread);
    }
    
    #[test]
    fn test_lockfree_order_queue() {
        let queue = LockFreeOrderQueue::new();
        
        let order_info = OrderInfo {
            order_id: OrderId::new(),
            quantity: Quantity::new(10.0),
            timestamp_nanos: 123456789,
        };
        
        queue.push(order_info.clone());
        assert_eq!(queue.len(), 1);
        assert_eq!(queue.total_quantity(), Quantity::new(10.0));
        assert!(!queue.is_empty());
        
        let popped = queue.pop().unwrap();
        assert_eq!(popped.order_id, order_info.order_id);
        assert_eq!(popped.quantity, order_info.quantity);
        assert_eq!(queue.len(), 0);
        assert!(queue.is_empty());
    }
    
    #[test]
    fn test_concurrent_queue_operations() {
        let queue = Arc::new(LockFreeOrderQueue::new());
        let num_threads = 10;
        let operations_per_thread = 100;
        
        // Spawn producer threads
        let producer_handles: Vec<_> = (0..num_threads).map(|_| {
            let queue = queue.clone();
            thread::spawn(move || {
                for _ in 0..operations_per_thread {
                    let order_info = OrderInfo {
                        order_id: OrderId::new(),
                        quantity: Quantity::new(1.0),
                        timestamp_nanos: 123456789,
                    };
                    queue.push(order_info);
                }
            })
        }).collect();
        
        // Wait for producers to finish
        for handle in producer_handles {
            handle.join().unwrap();
        }
        
        // Verify all items were added
        let expected_count = num_threads * operations_per_thread;
        assert_eq!(queue.len(), expected_count);
        
        // Spawn consumer threads
        let consumer_handles: Vec<_> = (0..num_threads).map(|_| {
            let queue = queue.clone();
            thread::spawn(move || {
                let mut consumed = 0;
                for _ in 0..operations_per_thread {
                    if queue.pop().is_some() {
                        consumed += 1;
                    }
                }
                consumed
            })
        }).collect();
        
        // Collect results
        let mut total_consumed = 0;
        for handle in consumer_handles {
            total_consumed += handle.join().unwrap();
        }
        
        assert_eq!(total_consumed, expected_count);
        assert!(queue.is_empty());
    }
}