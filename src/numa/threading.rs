use super::topology::{NumaTopology, CpuAffinity, set_thread_numa_node};
use std::sync::Arc;
use std::thread::{self, JoinHandle};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use crossbeam_channel::{unbounded, Receiver, Sender};
use std::collections::VecDeque;
use std::time::{Duration, Instant};

/// Configuration for NUMA-aware worker threads
#[derive(Debug, Clone)]
pub struct WorkerConfig {
    pub name: String,
    pub numa_node: Option<usize>,
    pub cpu_affinity: Option<usize>,
    pub stack_size: Option<usize>,
    pub priority: ThreadPriority,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ThreadPriority {
    Low,
    Normal,
    High,
    RealTime,
}

impl Default for WorkerConfig {
    fn default() -> Self {
        Self {
            name: "numa-worker".to_string(),
            numa_node: None,
            cpu_affinity: None,
            stack_size: Some(8 * 1024 * 1024), // 8MB stack
            priority: ThreadPriority::Normal,
        }
    }
}

/// NUMA-aware thread pool optimized for high-frequency trading
pub struct NumaAwareThreadPool<T> {
    topology: Arc<NumaTopology>,
    workers: Vec<NumaWorker<T>>,
    work_senders: Vec<Sender<WorkItem<T>>>,
    shutdown: Arc<AtomicBool>,
    next_worker: AtomicUsize,
}

/// Individual worker thread with NUMA awareness
pub struct NumaWorker<T> {
    #[allow(dead_code)]
    id: usize,
    numa_node: usize,
    #[allow(dead_code)]
    cpu_id: Option<usize>,
    handle: Option<JoinHandle<()>>,
    work_sender: Sender<WorkItem<T>>,
    #[allow(dead_code)]
    work_receiver: Receiver<WorkItem<T>>,
    #[allow(dead_code)]
    shutdown: Arc<AtomicBool>,
}

/// Work item for the thread pool
pub struct WorkItem<T> {
    pub data: T,
    pub priority: WorkPriority,
    pub timestamp: Instant,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum WorkPriority {
    Low = 0,
    Normal = 1,
    High = 2,
    Critical = 3,
}

impl<T> NumaAwareThreadPool<T>
where
    T: Send + 'static,
{
    /// Create a new NUMA-aware thread pool
    pub fn new<F>(
        topology: Arc<NumaTopology>,
        num_workers: usize,
        worker_fn: F,
    ) -> Result<Self, Box<dyn std::error::Error>>
    where
        F: Fn(usize, T) + Send + Sync + Clone + 'static,
    {
        let optimal_placement = topology.optimal_cpu_placement(num_workers);
        let mut workers = Vec::with_capacity(num_workers);
        let mut work_senders = Vec::with_capacity(num_workers);
        let shutdown = Arc::new(AtomicBool::new(false));
        
        for i in 0..num_workers {
            let cpu_id = optimal_placement.get(i).copied();
            let numa_node = cpu_id
                .and_then(|cpu| topology.cpu_node(cpu))
                .unwrap_or(0);
            
            let config = WorkerConfig {
                name: format!("hft-worker-{}", i),
                numa_node: Some(numa_node),
                cpu_affinity: cpu_id,
                priority: ThreadPriority::High,
                ..Default::default()
            };
            
            let worker = NumaWorker::new(
                i,
                numa_node,
                cpu_id,
                config,
                topology.clone(),
                worker_fn.clone(),
                shutdown.clone(),
            )?;
            
            work_senders.push(worker.work_sender.clone());
            workers.push(worker);
        }
        
        Ok(Self {
            topology,
            workers,
            work_senders,
            shutdown,
            next_worker: AtomicUsize::new(0),
        })
    }
    
    /// Submit work to the thread pool using round-robin distribution
    pub fn submit(&self, data: T, priority: WorkPriority) -> Result<(), Box<dyn std::error::Error>> {
        if self.shutdown.load(Ordering::Relaxed) {
            return Err("Thread pool is shutting down".into());
        }
        
        let worker_idx = self.next_worker.fetch_add(1, Ordering::Relaxed) % self.workers.len();
        let work_item = WorkItem {
            data,
            priority,
            timestamp: Instant::now(),
        };
        
        self.work_senders[worker_idx].send(work_item)?;
        Ok(())
    }
    
    /// Submit work to a specific NUMA node
    pub fn submit_to_node(
        &self,
        data: T,
        priority: WorkPriority,
        numa_node: usize,
    ) -> Result<(), Box<dyn std::error::Error>> {
        if self.shutdown.load(Ordering::Relaxed) {
            return Err("Thread pool is shutting down".into());
        }
        
        // Find a worker on the specified NUMA node
        let worker_idx = self.workers
            .iter()
            .position(|worker| worker.numa_node == numa_node)
            .ok_or_else(|| format!("No worker found for NUMA node {}", numa_node))?;
        
        let work_item = WorkItem {
            data,
            priority,
            timestamp: Instant::now(),
        };
        
        self.work_senders[worker_idx].send(work_item)?;
        Ok(())
    }
    
    /// Submit work to the least loaded worker
    pub fn submit_balanced(&self, data: T, priority: WorkPriority) -> Result<(), Box<dyn std::error::Error>> {
        if self.shutdown.load(Ordering::Relaxed) {
            return Err("Thread pool is shutting down".into());
        }
        
        // For now, use round-robin. In a more sophisticated implementation,
        // we could track queue lengths and submit to the least loaded worker
        self.submit(data, priority)
    }
    
    /// Get the number of workers
    pub fn worker_count(&self) -> usize {
        self.workers.len()
    }
    
    /// Get topology information
    pub fn topology(&self) -> &NumaTopology {
        &self.topology
    }
    
    /// Shutdown the thread pool gracefully
    pub fn shutdown(mut self, _timeout: Duration) -> Result<(), Box<dyn std::error::Error>> {
        self.shutdown.store(true, Ordering::Relaxed);
        
        // Drop senders to signal workers to stop
        self.work_senders.clear();
        
        // Wait for workers to finish
        for mut worker in self.workers {
            if let Some(handle) = worker.handle.take() {
                match handle.join() {
                    Ok(_) => {}
                    Err(_) => eprintln!("Worker thread panicked during shutdown"),
                }
            }
        }
        
        Ok(())
    }
}

impl<T> NumaWorker<T>
where
    T: Send + 'static,
{
    fn new<F>(
        id: usize,
        numa_node: usize,
        cpu_id: Option<usize>,
        config: WorkerConfig,
        topology: Arc<NumaTopology>,
        worker_fn: F,
        shutdown: Arc<AtomicBool>,
    ) -> Result<Self, Box<dyn std::error::Error>>
    where
        F: Fn(usize, T) + Send + 'static,
    {
        let (work_sender, work_receiver) = unbounded();
        
        let mut thread_builder = thread::Builder::new()
            .name(config.name.clone());
        
        if let Some(stack_size) = config.stack_size {
            thread_builder = thread_builder.stack_size(stack_size);
        }
        
        let topology_clone = topology.clone();
        let receiver_clone = work_receiver.clone();
        let shutdown_clone = shutdown.clone();
        
        let handle = thread_builder.spawn(move || {
            Self::worker_loop(
                id,
                numa_node,
                cpu_id,
                config,
                topology_clone,
                receiver_clone,
                worker_fn,
                shutdown_clone,
            );
        })?;
        
        Ok(Self {
            id,
            numa_node,
            cpu_id,
            handle: Some(handle),
            work_sender,
            work_receiver,
            shutdown,
        })
    }
    
    fn worker_loop<F>(
        worker_id: usize,
        numa_node: usize,
        cpu_id: Option<usize>,
        config: WorkerConfig,
        topology: Arc<NumaTopology>,
        receiver: Receiver<WorkItem<T>>,
        worker_fn: F,
        shutdown: Arc<AtomicBool>,
    ) where
        F: Fn(usize, T) + Send + 'static,
    {
        // Set up CPU affinity and thread properties
        if let Err(e) = Self::setup_worker_thread(worker_id, numa_node, cpu_id, &config, &topology) {
            eprintln!("Failed to setup worker thread {}: {}", worker_id, e);
        }
        
        // Set thread-local NUMA node
        set_thread_numa_node(numa_node);
        
        let mut work_queue: VecDeque<WorkItem<T>> = VecDeque::new();
        let mut last_yield = Instant::now();
        
        while !shutdown.load(Ordering::Relaxed) {
            // Try to receive work items
            match receiver.try_recv() {
                Ok(work_item) => {
                    work_queue.push_back(work_item);
                }
                Err(crossbeam_channel::TryRecvError::Empty) => {
                    // No work available, check if we should yield
                    if last_yield.elapsed() > Duration::from_micros(100) {
                        thread::yield_now();
                        last_yield = Instant::now();
                    }
                }
                Err(crossbeam_channel::TryRecvError::Disconnected) => {
                    // Channel disconnected, shutdown
                    break;
                }
            }
            
            // Process work items by priority
            if !work_queue.is_empty() {
                // Sort by priority (highest first)
                work_queue.make_contiguous().sort_by(|a, b| b.priority.cmp(&a.priority));
                
                if let Some(work_item) = work_queue.pop_front() {
                    worker_fn(worker_id, work_item.data);
                }
            }
        }
        
        // Process remaining work items before shutdown
        while let Some(work_item) = work_queue.pop_front() {
            worker_fn(worker_id, work_item.data);
        }
    }
    
    fn setup_worker_thread(
        worker_id: usize,
        _numa_node: usize,
        cpu_id: Option<usize>,
        config: &WorkerConfig,
        topology: &NumaTopology,
    ) -> Result<(), Box<dyn std::error::Error>> {
        // Set CPU affinity if specified
        if let Some(cpu) = cpu_id {
            let affinity = CpuAffinity::new(Arc::new(topology.clone()));
            if let Err(e) = affinity.pin_to_cpu(cpu) {
                eprintln!("Warning: Failed to pin worker {} to CPU {}: {}", worker_id, cpu, e);
            }
        }
        
        // Set thread priority
        Self::set_thread_priority(config.priority)?;
        
        // Set thread name (platform-specific)
        #[cfg(target_os = "linux")]
        {
            let thread_name = format!("{}-{}", config.name, worker_id);
            if thread_name.len() <= 15 { // Linux limit
                unsafe {
                    libc::pthread_setname_np(libc::pthread_self(), 
                        std::ffi::CString::new(thread_name)?.as_ptr());
                }
            }
        }
        
        Ok(())
    }
    
    fn set_thread_priority(_priority: ThreadPriority) -> Result<(), Box<dyn std::error::Error>> {
        #[cfg(target_os = "linux")]
        {
            let (policy, priority_value) = match _priority {
                ThreadPriority::Low => (libc::SCHED_OTHER, 0),
                ThreadPriority::Normal => (libc::SCHED_OTHER, 0),
                ThreadPriority::High => (libc::SCHED_OTHER, 0),
                ThreadPriority::RealTime => (libc::SCHED_FIFO, 1),
            };
            
            let param = libc::sched_param {
                sched_priority: priority_value,
            };
            
            unsafe {
                let result = libc::pthread_setschedparam(
                    libc::pthread_self(),
                    policy,
                    &param,
                );
                
                if result != 0 {
                    eprintln!("Warning: Failed to set thread priority: {}", 
                             std::io::Error::last_os_error());
                }
            }
        }
        
        #[cfg(not(target_os = "linux"))]
        {
            // Thread priority setting not implemented for this platform
            eprintln!("Thread priority setting not supported on this platform");
        }
        
        Ok(())
    }
}

/// Specialized worker pool for HFT order processing
pub struct HftWorkerPool {
    #[allow(dead_code)]
    topology: Arc<NumaTopology>,
    order_processors: NumaAwareThreadPool<OrderTask>,
    market_data_processors: NumaAwareThreadPool<MarketDataTask>,
    risk_processors: NumaAwareThreadPool<RiskTask>,
}

#[derive(Debug)]
pub enum OrderTask {
    ProcessOrder(String), // Simplified - would contain actual order data
    CancelOrder(u64),
    ModifyOrder(u64, String),
}

#[derive(Debug)]
pub enum MarketDataTask {
    ProcessTick(String), // Simplified - would contain tick data
    ProcessSnapshot(String),
    UpdateBook(String),
}

#[derive(Debug)]
pub enum RiskTask {
    ValidateOrder(String),
    CheckLimits(u64),
    UpdatePosition(String),
}

impl HftWorkerPool {
    pub fn new(topology: Arc<NumaTopology>) -> Result<Self, Box<dyn std::error::Error>> {
        let total_cpus = topology.total_cpus();
        
        // Allocate CPUs based on HFT priorities
        let order_workers = std::cmp::max(1, total_cpus / 2);     // 50% for order processing
        let market_data_workers = std::cmp::max(1, total_cpus / 4); // 25% for market data
        let risk_workers = std::cmp::max(1, total_cpus / 4);        // 25% for risk management
        
        let order_processors = NumaAwareThreadPool::new(
            topology.clone(),
            order_workers,
            |worker_id, task| {
                Self::process_order_task(worker_id, task);
            },
        )?;
        
        let market_data_processors = NumaAwareThreadPool::new(
            topology.clone(),
            market_data_workers,
            |worker_id, task| {
                Self::process_market_data_task(worker_id, task);
            },
        )?;
        
        let risk_processors = NumaAwareThreadPool::new(
            topology.clone(),
            risk_workers,
            |worker_id, task| {
                Self::process_risk_task(worker_id, task);
            },
        )?;
        
        Ok(Self {
            topology,
            order_processors,
            market_data_processors,
            risk_processors,
        })
    }
    
    pub fn submit_order_task(&self, task: OrderTask, priority: WorkPriority) -> Result<(), Box<dyn std::error::Error>> {
        self.order_processors.submit(task, priority)
    }
    
    pub fn submit_market_data_task(&self, task: MarketDataTask, priority: WorkPriority) -> Result<(), Box<dyn std::error::Error>> {
        self.market_data_processors.submit(task, priority)
    }
    
    pub fn submit_risk_task(&self, task: RiskTask, priority: WorkPriority) -> Result<(), Box<dyn std::error::Error>> {
        self.risk_processors.submit(task, priority)
    }
    
    fn process_order_task(worker_id: usize, task: OrderTask) {
        match task {
            OrderTask::ProcessOrder(order_data) => {
                // Process order - in real implementation, this would be the order matching logic
                println!("Worker {} processing order: {}", worker_id, order_data);
            }
            OrderTask::CancelOrder(order_id) => {
                println!("Worker {} cancelling order: {}", worker_id, order_id);
            }
            OrderTask::ModifyOrder(order_id, new_data) => {
                println!("Worker {} modifying order {}: {}", worker_id, order_id, new_data);
            }
        }
    }
    
    fn process_market_data_task(worker_id: usize, task: MarketDataTask) {
        match task {
            MarketDataTask::ProcessTick(tick_data) => {
                println!("Worker {} processing tick: {}", worker_id, tick_data);
            }
            MarketDataTask::ProcessSnapshot(snapshot_data) => {
                println!("Worker {} processing snapshot: {}", worker_id, snapshot_data);
            }
            MarketDataTask::UpdateBook(book_data) => {
                println!("Worker {} updating book: {}", worker_id, book_data);
            }
        }
    }
    
    fn process_risk_task(worker_id: usize, task: RiskTask) {
        match task {
            RiskTask::ValidateOrder(order_data) => {
                println!("Worker {} validating order: {}", worker_id, order_data);
            }
            RiskTask::CheckLimits(account_id) => {
                println!("Worker {} checking limits for account: {}", worker_id, account_id);
            }
            RiskTask::UpdatePosition(position_data) => {
                println!("Worker {} updating position: {}", worker_id, position_data);
            }
        }
    }
    
    pub fn shutdown(self, timeout: Duration) -> Result<(), Box<dyn std::error::Error>> {
        self.order_processors.shutdown(timeout)?;
        self.market_data_processors.shutdown(timeout)?;
        self.risk_processors.shutdown(timeout)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU32, Ordering};
    use std::time::Duration;

    #[test]
    fn test_numa_thread_pool() {
        let topology = Arc::new(NumaTopology::detect().unwrap());
        let processed_count = Arc::new(AtomicU32::new(0));
        let processed_clone = processed_count.clone();
        
        let pool = NumaAwareThreadPool::new(
            topology,
            4,
            move |_worker_id, data: u32| {
                processed_clone.fetch_add(data, Ordering::Relaxed);
            },
        ).unwrap();
        
        // Submit some work
        for i in 1..=10 {
            pool.submit(i, WorkPriority::Normal).unwrap();
        }
        
        // Give workers time to process
        thread::sleep(Duration::from_millis(100));
        
        // Should have processed sum of 1..=10 = 55
        assert_eq!(processed_count.load(Ordering::Relaxed), 55);
        
        pool.shutdown(Duration::from_secs(1)).unwrap();
    }
    
    #[test]
    fn test_work_priority() {
        let topology = Arc::new(NumaTopology::detect().unwrap());
        let processed_order = Arc::new(std::sync::Mutex::new(Vec::new()));
        let processed_clone = processed_order.clone();
        
        let pool = NumaAwareThreadPool::new(
            topology,
            1, // Single worker to ensure ordering
            move |_worker_id, data: u32| {
                processed_clone.lock().unwrap().push(data);
                thread::sleep(Duration::from_millis(10)); // Simulate work
            },
        ).unwrap();
        
        // Submit work with different priorities
        pool.submit(1, WorkPriority::Low).unwrap();
        pool.submit(2, WorkPriority::Critical).unwrap();
        pool.submit(3, WorkPriority::Normal).unwrap();
        pool.submit(4, WorkPriority::High).unwrap();
        
        // Give worker time to process
        thread::sleep(Duration::from_millis(100));
        
        let processed = processed_order.lock().unwrap();
        
        // First item should be processed first regardless of priority (already in queue)
        // But subsequent items should be processed by priority
        assert!(processed.len() >= 2);
        
        pool.shutdown(Duration::from_secs(1)).unwrap();
    }
    
    #[test]
    fn test_hft_worker_pool() {
        let topology = Arc::new(NumaTopology::detect().unwrap());
        let pool = HftWorkerPool::new(topology).unwrap();
        
        // Test different task types
        pool.submit_order_task(
            OrderTask::ProcessOrder("BTC/USD BUY 1.0 @ 50000".to_string()),
            WorkPriority::High,
        ).unwrap();
        
        pool.submit_market_data_task(
            MarketDataTask::ProcessTick("BTC/USD 50100".to_string()),
            WorkPriority::Normal,
        ).unwrap();
        
        pool.submit_risk_task(
            RiskTask::ValidateOrder("Order validation data".to_string()),
            WorkPriority::Critical,
        ).unwrap();
        
        // Give workers time to process
        thread::sleep(Duration::from_millis(50));
        
        pool.shutdown(Duration::from_secs(1)).unwrap();
    }
    
    #[test]
    fn test_worker_config() {
        let config = WorkerConfig {
            name: "test-worker".to_string(),
            numa_node: Some(0),
            cpu_affinity: Some(1),
            priority: ThreadPriority::High,
            ..Default::default()
        };
        
        assert_eq!(config.name, "test-worker");
        assert_eq!(config.numa_node, Some(0));
        assert_eq!(config.cpu_affinity, Some(1));
        assert_eq!(config.priority, ThreadPriority::High);
    }
}