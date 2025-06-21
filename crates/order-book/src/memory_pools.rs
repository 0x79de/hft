use crate::types::{Order, Trade};
use crossbeam_queue::SegQueue;
use arrayvec::ArrayVec;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::ptr::NonNull;
use std::alloc::{alloc, dealloc, Layout};

/// High-performance memory pool for frequent allocations in hot paths
/// Uses lock-free data structures to minimize contention
pub struct MemoryPool<T> {
    pool: SegQueue<NonNull<T>>,
    current_size: AtomicUsize,
    max_size: usize,
    layout: Layout,
}

impl<T> MemoryPool<T> {
    /// Create a new memory pool with initial capacity
    pub fn new(initial_size: usize, max_size: usize) -> Self {
        let layout = Layout::new::<T>();
        let pool = SegQueue::new();
        
        // Pre-allocate initial objects
        for _ in 0..initial_size {
            unsafe {
                let ptr = alloc(layout) as *mut T;
                if !ptr.is_null() {
                    pool.push(NonNull::new_unchecked(ptr));
                }
            }
        }
        
        Self {
            pool,
            current_size: AtomicUsize::new(initial_size),
            max_size,
            layout,
        }
    }
    
    /// Acquire an object from the pool or allocate a new one
    pub fn acquire(&self) -> PooledObject<T> {
        if let Some(ptr) = self.pool.pop() {
            self.current_size.fetch_sub(1, Ordering::Relaxed);
            PooledObject::new(ptr, self)
        } else {
            // Pool is empty, allocate new
            unsafe {
                let ptr = alloc(self.layout) as *mut T;
                if ptr.is_null() {
                    panic!("Failed to allocate memory");
                }
                PooledObject::new(NonNull::new_unchecked(ptr), self)
            }
        }
    }
    
    /// Return an object to the pool
    fn release(&self, ptr: NonNull<T>) {
        let current_size = self.current_size.load(Ordering::Relaxed);
        if current_size < self.max_size {
            self.pool.push(ptr);
            self.current_size.fetch_add(1, Ordering::Relaxed);
        } else {
            // Pool is full, deallocate
            unsafe {
                dealloc(ptr.as_ptr() as *mut u8, self.layout);
            }
        }
    }
}

impl<T> Drop for MemoryPool<T> {
    fn drop(&mut self) {
        // Deallocate all remaining objects in the pool
        while let Some(ptr) = self.pool.pop() {
            unsafe {
                dealloc(ptr.as_ptr() as *mut u8, self.layout);
            }
        }
    }
}

unsafe impl<T: Send> Send for MemoryPool<T> {}
unsafe impl<T: Send> Sync for MemoryPool<T> {}

/// RAII wrapper for pooled objects
pub struct PooledObject<T> {
    ptr: Option<NonNull<T>>,
    pool: *const MemoryPool<T>,
}

impl<T> PooledObject<T> {
    fn new(ptr: NonNull<T>, pool: &MemoryPool<T>) -> Self {
        Self {
            ptr: Some(ptr),
            pool: pool as *const MemoryPool<T>,
        }
    }
    
    /// Get a mutable reference to the contained object
    pub fn as_mut(&mut self) -> &mut T {
        unsafe {
            self.ptr.as_mut().unwrap().as_mut()
        }
    }
    
    /// Get a reference to the contained object
    pub fn as_ref(&self) -> &T {
        unsafe {
            self.ptr.as_ref().unwrap().as_ref()
        }
    }
    
    /// Take ownership of the underlying pointer (prevents automatic return to pool)
    pub fn into_raw(mut self) -> NonNull<T> {
        self.ptr.take().unwrap()
    }
}

impl<T> Drop for PooledObject<T> {
    fn drop(&mut self) {
        if let Some(ptr) = self.ptr.take() {
            unsafe {
                // Return to pool
                (*self.pool).release(ptr);
            }
        }
    }
}

unsafe impl<T: Send> Send for PooledObject<T> {}

/// Stack-allocated vector for small collections (avoids heap allocation)
pub type StackVec<T, const N: usize> = ArrayVec<T, N>;

/// Common stack-allocated arrays for HFT use cases
pub type TradeArray = StackVec<Trade, 8>;  // Most orders generate 0-2 trades
pub type OrderArray = StackVec<Order, 16>; // Small batches of orders
pub type PriceArray = StackVec<f64, 32>;   // Price levels for market depth

/// Pre-allocated vector pool for dynamic sizing when stack arrays aren't enough
pub struct VecPool<T> {
    pools: [SegQueue<Vec<T>>; 8], // Different size buckets
    bucket_sizes: [usize; 8],
    max_vecs_per_bucket: usize,
    current_counts: [AtomicUsize; 8],
}

impl<T> VecPool<T> {
    pub fn new(max_vecs_per_bucket: usize) -> Self {
        Self {
            pools: [
                SegQueue::new(), // 8 elements
                SegQueue::new(), // 16 elements
                SegQueue::new(), // 32 elements
                SegQueue::new(), // 64 elements
                SegQueue::new(), // 128 elements
                SegQueue::new(), // 256 elements
                SegQueue::new(), // 512 elements
                SegQueue::new(), // 1024 elements
            ],
            bucket_sizes: [8, 16, 32, 64, 128, 256, 512, 1024],
            max_vecs_per_bucket,
            current_counts: [
                AtomicUsize::new(0), AtomicUsize::new(0), AtomicUsize::new(0), AtomicUsize::new(0),
                AtomicUsize::new(0), AtomicUsize::new(0), AtomicUsize::new(0), AtomicUsize::new(0),
            ],
        }
    }
    
    /// Acquire a vector with at least the specified capacity
    pub fn acquire(&self, min_capacity: usize) -> PooledVec<T> {
        let bucket = self.find_bucket(min_capacity);
        
        if let Some(mut vec) = self.pools[bucket].pop() {
            vec.clear();
            self.current_counts[bucket].fetch_sub(1, Ordering::Relaxed);
            PooledVec::new(vec, bucket, self)
        } else {
            // Create new vector with appropriate capacity
            let capacity = self.bucket_sizes[bucket];
            PooledVec::new(Vec::with_capacity(capacity), bucket, self)
        }
    }
    
    fn find_bucket(&self, min_capacity: usize) -> usize {
        for (i, &size) in self.bucket_sizes.iter().enumerate() {
            if size >= min_capacity {
                return i;
            }
        }
        // If larger than largest bucket, use the largest
        self.bucket_sizes.len() - 1
    }
    
    fn release(&self, mut vec: Vec<T>, bucket: usize) {
        if self.current_counts[bucket].load(Ordering::Relaxed) < self.max_vecs_per_bucket {
            vec.clear();
            self.pools[bucket].push(vec);
            self.current_counts[bucket].fetch_add(1, Ordering::Relaxed);
        }
        // If pool is full, let the vector drop naturally
    }
}

impl<T> Default for VecPool<T> {
    fn default() -> Self {
        Self::new(100) // Default max of 100 vectors per bucket
    }
}

unsafe impl<T: Send> Send for VecPool<T> {}
unsafe impl<T: Send> Sync for VecPool<T> {}

/// RAII wrapper for pooled vectors
pub struct PooledVec<T> {
    vec: Option<Vec<T>>,
    bucket: usize,
    pool: *const VecPool<T>,
}

impl<T> PooledVec<T> {
    fn new(vec: Vec<T>, bucket: usize, pool: &VecPool<T>) -> Self {
        Self {
            vec: Some(vec),
            bucket,
            pool: pool as *const VecPool<T>,
        }
    }
    
    /// Get a mutable reference to the vector
    pub fn as_mut(&mut self) -> &mut Vec<T> {
        self.vec.as_mut().unwrap()
    }
    
    /// Get a reference to the vector
    pub fn as_ref(&self) -> &Vec<T> {
        self.vec.as_ref().unwrap()
    }
    
    /// Push an element to the vector
    pub fn push(&mut self, value: T) {
        self.vec.as_mut().unwrap().push(value);
    }
    
    /// Pop an element from the vector
    pub fn pop(&mut self) -> Option<T> {
        self.vec.as_mut().unwrap().pop()
    }
    
    /// Get the length of the vector
    pub fn len(&self) -> usize {
        self.vec.as_ref().unwrap().len()
    }
    
    /// Check if the vector is empty
    pub fn is_empty(&self) -> bool {
        self.vec.as_ref().unwrap().is_empty()
    }
    
    /// Clear the vector
    pub fn clear(&mut self) {
        self.vec.as_mut().unwrap().clear();
    }
}

impl<T> Drop for PooledVec<T> {
    fn drop(&mut self) {
        if let Some(vec) = self.vec.take() {
            unsafe {
                (*self.pool).release(vec, self.bucket);
            }
        }
    }
}

impl<T> std::ops::Deref for PooledVec<T> {
    type Target = Vec<T>;
    
    fn deref(&self) -> &Self::Target {
        self.vec.as_ref().unwrap()
    }
}

impl<T> std::ops::DerefMut for PooledVec<T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.vec.as_mut().unwrap()
    }
}

unsafe impl<T: Send> Send for PooledVec<T> {}

/// Global memory pools for common HFT objects
pub struct GlobalPools {
    pub trade_pool: MemoryPool<Trade>,
    pub order_pool: MemoryPool<Order>,
    pub trade_vec_pool: VecPool<Trade>,
    pub order_vec_pool: VecPool<Order>,
}

impl GlobalPools {
    pub fn new() -> Self {
        Self {
            trade_pool: MemoryPool::new(1000, 10000),     // Pre-allocate 1K trades, max 10K
            order_pool: MemoryPool::new(5000, 50000),     // Pre-allocate 5K orders, max 50K
            trade_vec_pool: VecPool::new(100),            // Max 100 vectors per bucket
            order_vec_pool: VecPool::new(100),
        }
    }
}

impl Default for GlobalPools {
    fn default() -> Self {
        Self::new()
    }
}

// Global instance for easy access
lazy_static::lazy_static! {
    pub static ref GLOBAL_POOLS: GlobalPools = GlobalPools::new();
}

/// Optimized allocation functions for hot paths
pub mod allocators {
    use super::*;
    
    /// Acquire a trade object from the global pool
    #[inline]
    pub fn acquire_trade() -> PooledObject<Trade> {
        GLOBAL_POOLS.trade_pool.acquire()
    }
    
    /// Acquire an order object from the global pool
    #[inline]
    pub fn acquire_order() -> PooledObject<Order> {
        GLOBAL_POOLS.order_pool.acquire()
    }
    
    /// Acquire a vector for trades with specified minimum capacity
    #[inline]
    pub fn acquire_trade_vec(min_capacity: usize) -> PooledVec<Trade> {
        GLOBAL_POOLS.trade_vec_pool.acquire(min_capacity)
    }
    
    /// Acquire a vector for orders with specified minimum capacity
    #[inline]
    pub fn acquire_order_vec(min_capacity: usize) -> PooledVec<Order> {
        GLOBAL_POOLS.order_vec_pool.acquire(min_capacity)
    }
    
    /// Create a stack-allocated trade array for small collections
    #[inline]
    pub fn create_trade_array() -> TradeArray {
        ArrayVec::new()
    }
    
    /// Create a stack-allocated order array for small collections
    #[inline]
    pub fn create_order_array() -> OrderArray {
        ArrayVec::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{Price, Quantity, Side, OrderType};
    use crate::OrderId;
    use uuid::Uuid;
    use std::thread;
    use std::sync::Arc;

    #[test]
    fn test_memory_pool_basic_operations() {
        let pool: MemoryPool<i32> = MemoryPool::new(10, 100);
        
        let mut obj = pool.acquire();
        unsafe {
            std::ptr::write(obj.as_mut(), 42);
        }
        
        assert_eq!(*obj.as_ref(), 42);
        
        // Object should be returned to pool when dropped
        drop(obj);
        
        // Acquire again should reuse the object
        let mut obj2 = pool.acquire();
        // Initialize the memory before reading
        unsafe {
            std::ptr::write(obj2.as_mut(), 84);
        }
        assert_eq!(*obj2.as_ref(), 84);
    }
    
    #[test]
    fn test_vec_pool_operations() {
        let pool: VecPool<i32> = VecPool::new(10);
        
        let mut vec = pool.acquire(16);
        vec.push(1);
        vec.push(2);
        vec.push(3);
        
        assert_eq!(vec.len(), 3);
        assert_eq!(vec[0], 1);
        
        // Should be returned to appropriate bucket
        drop(vec);
        
        // Acquire again should get a cleared vector
        let vec2 = pool.acquire(16);
        assert_eq!(vec2.len(), 0);
        assert!(vec2.capacity() >= 16);
    }
    
    #[test]
    fn test_stack_arrays() {
        let mut trades = TradeArray::new();
        
        // Create a test trade
        let trade = Trade::new(
            "BTCUSD",
            OrderId::new(),
            OrderId::new(),
            Price::new(50000.0),
            Quantity::new(1.0),
            Uuid::new_v4(),
            Uuid::new_v4(),
        );
        
        trades.push(trade);
        assert_eq!(trades.len(), 1);
        
        // Should not allocate on heap for small collections
        assert!(trades.capacity() <= 8);
    }
    
    #[test]
    fn test_global_allocators() {
        let mut trade = allocators::acquire_trade();
        unsafe {
            std::ptr::write(trade.as_mut(), Trade::new(
                "BTCUSD",
                OrderId::new(),
                OrderId::new(),
                Price::new(50000.0),
                Quantity::new(1.0),
                Uuid::new_v4(),
                Uuid::new_v4(),
            ));
        }
        
        assert_eq!(trade.as_ref().symbol, "BTCUSD");
        
        let mut order = allocators::acquire_order();
        unsafe {
            std::ptr::write(order.as_mut(), Order::new(
                "BTCUSD".to_string(),
                Side::Buy,
                OrderType::Limit,
                Price::new(50000.0),
                Quantity::new(1.0),
                Uuid::new_v4(),
            ));
        }
        
        assert_eq!(order.as_ref().symbol, "BTCUSD");
    }
    
    #[test]
    fn test_concurrent_pool_access() {
        let pool = Arc::new(MemoryPool::<i32>::new(100, 1000));
        let num_threads = 10;
        let operations_per_thread = 100;
        
        let handles: Vec<_> = (0..num_threads).map(|i| {
            let pool = pool.clone();
            thread::spawn(move || {
                for j in 0..operations_per_thread {
                    let mut obj = pool.acquire();
                    unsafe {
                        std::ptr::write(obj.as_mut(), i * operations_per_thread + j);
                    }
                    // Object automatically returned to pool when dropped
                }
            })
        }).collect();
        
        for handle in handles {
            handle.join().unwrap();
        }
        
        // Pool should still be functional after concurrent access
        let mut obj = pool.acquire();
        unsafe {
            std::ptr::write(obj.as_mut(), 999);
        }
        assert_eq!(*obj.as_ref(), 999);
    }
    
    #[test]
    fn test_vec_pool_bucket_selection() {
        let pool: VecPool<i32> = VecPool::new(10);
        
        // Test different capacity requests
        let vec8 = pool.acquire(5);   // Should use 8-element bucket
        let vec16 = pool.acquire(10); // Should use 16-element bucket
        let vec32 = pool.acquire(25); // Should use 32-element bucket
        
        assert!(vec8.capacity() >= 5);
        assert!(vec16.capacity() >= 10);
        assert!(vec32.capacity() >= 25);
        
        // Typically, capacities should match bucket sizes
        assert!(vec8.capacity() >= 8);
        assert!(vec16.capacity() >= 16);
        assert!(vec32.capacity() >= 32);
    }
    
    #[test]
    fn test_memory_pool_overflow() {
        let pool: MemoryPool<i32> = MemoryPool::new(2, 2); // Very small pool
        
        let obj1 = pool.acquire();
        let obj2 = pool.acquire();
        let obj3 = pool.acquire(); // Should allocate new since pool is empty
        
        // All should work fine
        drop(obj1);
        drop(obj2);
        drop(obj3); // One of these should be deallocated since pool is full
        
        // Pool should still be functional
        let obj4 = pool.acquire();
        drop(obj4);
    }
}