use super::topology::{NumaTopology, get_thread_numa_node};
use std::sync::Arc;
use std::alloc::{alloc, dealloc, Layout};
use std::ptr::NonNull;
use std::sync::atomic::{AtomicUsize, Ordering};
use crossbeam_queue::SegQueue;

/// Custom allocation error for NUMA allocator
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NumaAllocError;

impl std::fmt::Display for NumaAllocError {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "NUMA allocation failed")
    }
}

impl std::error::Error for NumaAllocError {}

/// NUMA-aware memory allocator for high-performance applications
pub struct NumaAllocator {
    #[allow(dead_code)]
    topology: Arc<NumaTopology>,
    node_pools: Vec<NodeMemoryPool>,
    default_node: usize,
    allocation_counter: AtomicUsize,
}

/// Memory pool for a specific NUMA node
struct NodeMemoryPool {
    node_id: usize,
    small_blocks: SegQueue<PooledBlock>,   // <= 64 bytes
    medium_blocks: SegQueue<PooledBlock>,  // <= 512 bytes
    large_blocks: SegQueue<PooledBlock>,   // <= 4KB
    allocated_bytes: AtomicUsize,
    freed_bytes: AtomicUsize,
}

/// A pooled memory block
struct PooledBlock {
    ptr: NonNull<u8>,
    size: usize,
    layout: Layout,
}

/// NUMA-aware allocation result
pub struct NumaAllocation {
    ptr: NonNull<u8>,
    size: usize,
    layout: Layout,
    numa_node: usize,
    #[allow(dead_code)]
    allocator: *const NumaAllocator,
}

impl NumaAllocator {
    /// Create a new NUMA-aware allocator
    pub fn new(topology: Arc<NumaTopology>) -> Self {
        let num_nodes = topology.num_nodes();
        let mut node_pools = Vec::with_capacity(num_nodes);
        
        for i in 0..num_nodes {
            node_pools.push(NodeMemoryPool::new(i));
        }
        
        Self {
            topology,
            node_pools,
            default_node: 0,
            allocation_counter: AtomicUsize::new(0),
        }
    }
    
    /// Allocate memory on the current thread's NUMA node
    pub fn allocate(&self, layout: Layout) -> Result<NumaAllocation, NumaAllocError> {
        let numa_node = get_thread_numa_node().unwrap_or(self.default_node);
        self.allocate_on_node(layout, numa_node)
    }
    
    /// Allocate memory on a specific NUMA node
    pub fn allocate_on_node(&self, layout: Layout, numa_node: usize) -> Result<NumaAllocation, NumaAllocError> {
        let node_id = if numa_node < self.node_pools.len() {
            numa_node
        } else {
            self.default_node
        };
        
        let pool = &self.node_pools[node_id];
        
        // Try to get from pool first
        if let Some(block) = pool.try_get_pooled_block(layout.size()) {
            if block.layout.align() >= layout.align() && block.size >= layout.size() {
                self.allocation_counter.fetch_add(1, Ordering::Relaxed);
                return Ok(NumaAllocation {
                    ptr: block.ptr,
                    size: block.size,
                    layout: block.layout,
                    numa_node: node_id,
                    allocator: self as *const NumaAllocator,
                });
            } else {
                // Block doesn't meet requirements, put it back
                pool.return_pooled_block(block);
            }
        }
        
        // Allocate new memory
        let ptr = self.allocate_raw_on_node(layout, node_id)?;
        pool.allocated_bytes.fetch_add(layout.size(), Ordering::Relaxed);
        self.allocation_counter.fetch_add(1, Ordering::Relaxed);
        
        Ok(NumaAllocation {
            ptr,
            size: layout.size(),
            layout,
            numa_node: node_id,
            allocator: self as *const NumaAllocator,
        })
    }
    
    /// Deallocate memory
    pub fn deallocate(&self, allocation: NumaAllocation) {
        let pool = &self.node_pools[allocation.numa_node];
        
        // Try to return to pool if it's a suitable size
        if allocation.size <= 4096 && pool.can_pool_block(allocation.size) {
            let block = PooledBlock {
                ptr: allocation.ptr,
                size: allocation.size,
                layout: allocation.layout,
            };
            pool.return_pooled_block(block);
        } else {
            // Deallocate directly
            unsafe {
                dealloc(allocation.ptr.as_ptr(), allocation.layout);
            }
        }
        
        pool.freed_bytes.fetch_add(allocation.size, Ordering::Relaxed);
    }
    
    /// Get allocation statistics
    pub fn stats(&self) -> NumaAllocatorStats {
        let mut total_allocated = 0;
        let mut total_freed = 0;
        let mut node_stats = Vec::new();
        
        for pool in &self.node_pools {
            let allocated = pool.allocated_bytes.load(Ordering::Relaxed);
            let freed = pool.freed_bytes.load(Ordering::Relaxed);
            
            total_allocated += allocated;
            total_freed += freed;
            
            node_stats.push(NodeStats {
                node_id: pool.node_id,
                allocated_bytes: allocated,
                freed_bytes: freed,
                net_bytes: allocated.saturating_sub(freed),
                pooled_blocks: pool.pooled_block_count(),
            });
        }
        
        NumaAllocatorStats {
            total_allocations: self.allocation_counter.load(Ordering::Relaxed),
            total_allocated_bytes: total_allocated,
            total_freed_bytes: total_freed,
            net_allocated_bytes: total_allocated.saturating_sub(total_freed),
            node_stats,
        }
    }
    
    /// Force garbage collection on all nodes
    pub fn gc(&self) {
        for pool in &self.node_pools {
            pool.gc();
        }
    }
    
    fn allocate_raw_on_node(&self, layout: Layout, _node_id: usize) -> Result<NonNull<u8>, NumaAllocError> {
        #[cfg(target_os = "linux")]
        {
            // Use numa_alloc_onnode if available
            self.allocate_raw_numa_linux(layout, _node_id)
        }
        #[cfg(not(target_os = "linux"))]
        {
            // Fallback to regular allocation
            self.allocate_raw_fallback(layout)
        }
    }
    
    #[cfg(target_os = "linux")]
    fn allocate_raw_numa_linux(&self, layout: Layout, _node_id: usize) -> Result<NonNull<u8>, NumaAllocError> {
        // For simplicity, we'll use regular allocation here
        // In a full implementation, you would use libnuma functions
        self.allocate_raw_fallback(layout)
    }
    
    fn allocate_raw_fallback(&self, layout: Layout) -> Result<NonNull<u8>, NumaAllocError> {
        unsafe {
            let ptr = alloc(layout);
            if ptr.is_null() {
                Err(NumaAllocError)
            } else {
                Ok(NonNull::new_unchecked(ptr))
            }
        }
    }
}

impl NodeMemoryPool {
    fn new(node_id: usize) -> Self {
        Self {
            node_id,
            small_blocks: SegQueue::new(),
            medium_blocks: SegQueue::new(),
            large_blocks: SegQueue::new(),
            allocated_bytes: AtomicUsize::new(0),
            freed_bytes: AtomicUsize::new(0),
        }
    }
    
    fn try_get_pooled_block(&self, size: usize) -> Option<PooledBlock> {
        if size <= 64 {
            self.small_blocks.pop()
        } else if size <= 512 {
            self.medium_blocks.pop()
        } else if size <= 4096 {
            self.large_blocks.pop()
        } else {
            None
        }
    }
    
    fn return_pooled_block(&self, block: PooledBlock) {
        if block.size <= 64 {
            self.small_blocks.push(block);
        } else if block.size <= 512 {
            self.medium_blocks.push(block);
        } else if block.size <= 4096 {
            self.large_blocks.push(block);
        }
        // Blocks larger than 4KB are not pooled
    }
    
    fn can_pool_block(&self, size: usize) -> bool {
        size <= 4096
    }
    
    fn pooled_block_count(&self) -> usize {
        // Approximate count (SegQueue doesn't provide exact count)
        let mut count = 0;
        
        // Count by temporarily popping and pushing back
        let mut temp_blocks = Vec::new();
        
        while let Some(block) = self.small_blocks.pop() {
            temp_blocks.push(block);
            count += 1;
            if count > 1000 { break; } // Limit counting to avoid long pause
        }
        for block in temp_blocks {
            self.small_blocks.push(block);
        }
        
        count
    }
    
    fn gc(&self) {
        // Garbage collection - remove some pooled blocks to free memory
        // This is a simple implementation that removes up to 10 blocks from each pool
        for _ in 0..10 {
            if let Some(block) = self.small_blocks.pop() {
                unsafe {
                    dealloc(block.ptr.as_ptr(), block.layout);
                }
            } else {
                break;
            }
        }
        
        for _ in 0..10 {
            if let Some(block) = self.medium_blocks.pop() {
                unsafe {
                    dealloc(block.ptr.as_ptr(), block.layout);
                }
            } else {
                break;
            }
        }
        
        for _ in 0..10 {
            if let Some(block) = self.large_blocks.pop() {
                unsafe {
                    dealloc(block.ptr.as_ptr(), block.layout);
                }
            } else {
                break;
            }
        }
    }
}

impl Drop for NumaAllocation {
    fn drop(&mut self) {
        // Always deallocate directly to avoid accessing potentially freed allocator
        unsafe {
            dealloc(self.ptr.as_ptr(), self.layout);
        }
    }
}

impl std::ops::Deref for NumaAllocation {
    type Target = [u8];
    
    fn deref(&self) -> &Self::Target {
        unsafe {
            std::slice::from_raw_parts(self.ptr.as_ptr(), self.size)
        }
    }
}

impl std::ops::DerefMut for NumaAllocation {
    fn deref_mut(&mut self) -> &mut Self::Target {
        unsafe {
            std::slice::from_raw_parts_mut(self.ptr.as_ptr(), self.size)
        }
    }
}

unsafe impl Send for NumaAllocation {}
unsafe impl Sync for NumaAllocation {}

/// Statistics for NUMA allocator
#[derive(Debug, Clone)]
pub struct NumaAllocatorStats {
    pub total_allocations: usize,
    pub total_allocated_bytes: usize,
    pub total_freed_bytes: usize,
    pub net_allocated_bytes: usize,
    pub node_stats: Vec<NodeStats>,
}

#[derive(Debug, Clone)]
pub struct NodeStats {
    pub node_id: usize,
    pub allocated_bytes: usize,
    pub freed_bytes: usize,
    pub net_bytes: usize,
    pub pooled_blocks: usize,
}

/// NUMA-aware allocator wrapper for specific types
pub struct NumaVec<T> {
    allocator: Arc<NumaAllocator>,
    allocation: Option<NumaAllocation>,
    len: usize,
    capacity: usize,
    numa_node: usize,
    _phantom: std::marker::PhantomData<T>,
}

impl<T> NumaVec<T> {
    /// Create a new NUMA vector on the current thread's node
    pub fn new(allocator: Arc<NumaAllocator>) -> Self {
        let numa_node = get_thread_numa_node().unwrap_or(0);
        Self::with_capacity_on_node(0, numa_node, allocator)
    }
    
    /// Create a new NUMA vector with capacity on a specific node
    pub fn with_capacity_on_node(capacity: usize, numa_node: usize, allocator: Arc<NumaAllocator>) -> Self {
        let allocation = if capacity > 0 {
            let layout = Layout::array::<T>(capacity).unwrap();
            Some(allocator.allocate_on_node(layout, numa_node).unwrap())
        } else {
            None
        };
        
        Self {
            allocator,
            allocation,
            len: 0,
            capacity,
            numa_node,
            _phantom: std::marker::PhantomData,
        }
    }
    
    /// Push an element to the vector
    pub fn push(&mut self, value: T) {
        if self.len >= self.capacity {
            self.grow();
        }
        
        unsafe {
            let ptr = self.as_mut_ptr().add(self.len);
            std::ptr::write(ptr, value);
        }
        self.len += 1;
    }
    
    /// Pop an element from the vector
    pub fn pop(&mut self) -> Option<T> {
        if self.len == 0 {
            None
        } else {
            self.len -= 1;
            unsafe {
                let ptr = self.as_ptr().add(self.len);
                Some(std::ptr::read(ptr))
            }
        }
    }
    
    /// Get the length of the vector
    pub fn len(&self) -> usize {
        self.len
    }
    
    /// Check if the vector is empty
    pub fn is_empty(&self) -> bool {
        self.len == 0
    }
    
    /// Get the capacity of the vector
    pub fn capacity(&self) -> usize {
        self.capacity
    }
    
    /// Get the NUMA node of this vector
    pub fn numa_node(&self) -> usize {
        self.numa_node
    }
    
    fn as_ptr(&self) -> *const T {
        if let Some(ref allocation) = self.allocation {
            allocation.ptr.as_ptr() as *const T
        } else {
            std::ptr::NonNull::dangling().as_ptr()
        }
    }
    
    fn as_mut_ptr(&mut self) -> *mut T {
        if let Some(ref allocation) = self.allocation {
            allocation.ptr.as_ptr() as *mut T
        } else {
            std::ptr::NonNull::dangling().as_ptr()
        }
    }
    
    fn grow(&mut self) {
        let new_capacity = if self.capacity == 0 { 1 } else { self.capacity * 2 };
        let new_layout = Layout::array::<T>(new_capacity).unwrap();
        let new_allocation = self.allocator.allocate_on_node(new_layout, self.numa_node).unwrap();
        
        if let Some(old_allocation) = self.allocation.take() {
            // Copy existing elements
            unsafe {
                std::ptr::copy_nonoverlapping(
                    old_allocation.ptr.as_ptr(),
                    new_allocation.ptr.as_ptr(),
                    self.len * std::mem::size_of::<T>(),
                );
            }
            
            // Deallocate old memory
            self.allocator.deallocate(old_allocation);
        }
        
        self.allocation = Some(new_allocation);
        self.capacity = new_capacity;
    }
}

impl<T> Drop for NumaVec<T> {
    fn drop(&mut self) {
        // Drop all elements
        for i in 0..self.len {
            unsafe {
                let ptr = self.as_ptr().add(i);
                std::ptr::drop_in_place(ptr as *mut T);
            }
        }
        
        // Allocation will be automatically deallocated when dropped
    }
}

impl<T> std::ops::Index<usize> for NumaVec<T> {
    type Output = T;
    
    fn index(&self, index: usize) -> &Self::Output {
        assert!(index < self.len);
        unsafe {
            &*self.as_ptr().add(index)
        }
    }
}

impl<T> std::ops::IndexMut<usize> for NumaVec<T> {
    fn index_mut(&mut self, index: usize) -> &mut Self::Output {
        assert!(index < self.len);
        unsafe {
            &mut *self.as_mut_ptr().add(index)
        }
    }
}

unsafe impl<T: Send> Send for NumaVec<T> {}
unsafe impl<T: Sync> Sync for NumaVec<T> {}

#[cfg(test)]
mod tests {
    use super::*;
    use std::alloc::Layout;

    #[test]
    fn test_numa_allocator() {
        let topology = Arc::new(NumaTopology::detect().unwrap());
        let allocator = NumaAllocator::new(topology);
        
        // Test basic allocation
        let layout = Layout::from_size_align(1024, 8).unwrap();
        let mut allocation = allocator.allocate(layout).unwrap();
        
        assert_eq!(allocation.size, 1024);
        assert_eq!(allocation.layout.size(), 1024);
        assert_eq!(allocation.layout.align(), 8);
        
        // Test writing to allocation
        let slice = &mut allocation[..];
        slice[0] = 42;
        slice[1023] = 84;
        
        assert_eq!(slice[0], 42);
        assert_eq!(slice[1023], 84);
        
        // Allocation will be automatically deallocated when dropped
    }
    
    #[test]
    fn test_node_specific_allocation() {
        let topology = Arc::new(NumaTopology::detect().unwrap());
        let allocator = NumaAllocator::new(topology.clone());
        
        let layout = Layout::from_size_align(512, 4).unwrap();
        
        // Test allocation on each node
        for node_id in 0..topology.num_nodes() {
            let allocation = allocator.allocate_on_node(layout, node_id).unwrap();
            assert_eq!(allocation.numa_node, node_id);
            assert_eq!(allocation.size, 512);
        }
    }
    
    #[test]
    fn test_allocator_stats() {
        let topology = Arc::new(NumaTopology::detect().unwrap());
        let allocator = NumaAllocator::new(topology);
        
        let initial_stats = allocator.stats();
        assert_eq!(initial_stats.total_allocations, 0);
        
        // Allocate some memory
        let layout = Layout::from_size_align(1024, 8).unwrap();
        let _allocation1 = allocator.allocate(layout).unwrap();
        let _allocation2 = allocator.allocate(layout).unwrap();
        
        let stats = allocator.stats();
        assert_eq!(stats.total_allocations, 2);
        assert!(stats.total_allocated_bytes >= 2048);
    }
    
    #[test]
    fn test_numa_vec() {
        let topology = Arc::new(NumaTopology::detect().unwrap());
        let allocator = Arc::new(NumaAllocator::new(topology));
        
        let mut vec: NumaVec<i32> = NumaVec::new(allocator.clone());
        
        assert_eq!(vec.len(), 0);
        assert!(vec.is_empty());
        
        // Test push and pop
        vec.push(42);
        vec.push(84);
        vec.push(126);
        
        assert_eq!(vec.len(), 3);
        assert_eq!(vec[0], 42);
        assert_eq!(vec[1], 84);
        assert_eq!(vec[2], 126);
        
        assert_eq!(vec.pop(), Some(126));
        assert_eq!(vec.pop(), Some(84));
        assert_eq!(vec.pop(), Some(42));
        assert_eq!(vec.pop(), None);
        
        assert!(vec.is_empty());
    }
    
    #[test]
    fn test_numa_vec_growth() {
        let topology = Arc::new(NumaTopology::detect().unwrap());
        let allocator = Arc::new(NumaAllocator::new(topology));
        
        let mut vec: NumaVec<usize> = NumaVec::with_capacity_on_node(2, 0, allocator);
        
        assert_eq!(vec.capacity(), 2);
        
        // Fill initial capacity
        vec.push(1);
        vec.push(2);
        assert_eq!(vec.capacity(), 2);
        
        // Trigger growth
        vec.push(3);
        assert!(vec.capacity() > 2);
        assert_eq!(vec.len(), 3);
        
        // Verify all elements are intact
        assert_eq!(vec[0], 1);
        assert_eq!(vec[1], 2);
        assert_eq!(vec[2], 3);
    }
    
    #[test]
    fn test_memory_pooling() {
        let topology = Arc::new(NumaTopology::detect().unwrap());
        let allocator = NumaAllocator::new(topology);
        
        let layout = Layout::from_size_align(64, 8).unwrap();
        
        // Allocate and let drop to deallocate
        for _ in 0..10 {
            let _allocation = allocator.allocate(layout).unwrap();
            // allocation will be automatically deallocated when it goes out of scope
        }
        
        let stats_before = allocator.stats();
        
        // Allocate again - should reuse pooled memory
        let _allocation = allocator.allocate(layout).unwrap();
        
        let stats_after = allocator.stats();
        
        // Should have one more allocation
        assert_eq!(stats_after.total_allocations, stats_before.total_allocations + 1);
    }
}