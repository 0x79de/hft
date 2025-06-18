use crate::events::Event;
use std::collections::VecDeque;
use std::time::{Duration, Instant};
use parking_lot::Mutex;
use std::sync::Arc;

#[derive(Debug, Clone)]
pub struct BatchConfig {
    pub max_batch_size: usize,
    pub max_batch_delay: Duration,
    pub max_memory_usage: usize,
}

impl Default for BatchConfig {
    fn default() -> Self {
        Self {
            max_batch_size: 1000,
            max_batch_delay: Duration::from_millis(10),
            max_memory_usage: 1024 * 1024, // 1MB
        }
    }
}

#[derive(Debug)]
pub struct EventBatch {
    events: Vec<Event>,
    created_at: Instant,
    estimated_size: usize,
}

impl EventBatch {
    #[inline]
    pub fn new() -> Self {
        Self {
            events: Vec::new(),
            created_at: Instant::now(),
            estimated_size: 0,
        }
    }
    
    #[inline]
    pub fn with_capacity(capacity: usize) -> Self {
        Self {
            events: Vec::with_capacity(capacity),
            created_at: Instant::now(),
            estimated_size: 0,
        }
    }
    
    #[inline]
    pub fn add_event(&mut self, event: Event) {
        self.estimated_size += std::mem::size_of::<Event>();
        self.events.push(event);
    }
    
    #[inline]
    pub fn len(&self) -> usize {
        self.events.len()
    }
    
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.events.is_empty()
    }
    
    #[inline]
    pub fn size(&self) -> usize {
        self.estimated_size
    }
    
    #[inline]
    pub fn age(&self) -> Duration {
        self.created_at.elapsed()
    }
    
    #[inline]
    pub fn events(&self) -> &[Event] {
        &self.events
    }
    
    #[inline]
    pub fn into_events(self) -> Vec<Event> {
        self.events
    }
    
    #[inline]
    pub fn should_flush(&self, config: &BatchConfig) -> bool {
        self.len() >= config.max_batch_size 
            || self.age() >= config.max_batch_delay 
            || self.size() >= config.max_memory_usage
    }
}

impl Default for EventBatch {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, Clone)]
pub struct BatchProcessor {
    batch: Arc<Mutex<EventBatch>>,
    config: BatchConfig,
    processed_batches: Arc<Mutex<u64>>,
    processed_events: Arc<Mutex<u64>>,
}

impl BatchProcessor {
    #[inline]
    pub fn new(config: BatchConfig) -> Self {
        Self {
            batch: Arc::new(Mutex::new(EventBatch::with_capacity(config.max_batch_size))),
            config,
            processed_batches: Arc::new(Mutex::new(0)),
            processed_events: Arc::new(Mutex::new(0)),
        }
    }
    
    #[inline]
    pub fn add_event(&self, event: Event) -> Option<EventBatch> {
        let mut batch = self.batch.lock();
        batch.add_event(event);
        
        if batch.should_flush(&self.config) {
            let old_batch = std::mem::replace(&mut *batch, EventBatch::with_capacity(self.config.max_batch_size));
            Some(old_batch)
        } else {
            None
        }
    }
    
    #[inline]
    pub fn flush(&self) -> Option<EventBatch> {
        let mut batch = self.batch.lock();
        if batch.is_empty() {
            None
        } else {
            let old_batch = std::mem::replace(&mut *batch, EventBatch::with_capacity(self.config.max_batch_size));
            Some(old_batch)
        }
    }
    
    #[inline]
    pub fn mark_batch_processed(&self, batch: &EventBatch) {
        let mut batches = self.processed_batches.lock();
        *batches += 1;
        
        let mut events = self.processed_events.lock();
        *events += batch.len() as u64;
    }
    
    #[inline]
    pub fn stats(&self) -> BatchStats {
        let current_batch = self.batch.lock();
        BatchStats {
            processed_batches: *self.processed_batches.lock(),
            processed_events: *self.processed_events.lock(),
            pending_events: current_batch.len(),
            current_batch_age: current_batch.age(),
            current_batch_size: current_batch.size(),
        }
    }
    
    #[inline]
    pub fn config(&self) -> &BatchConfig {
        &self.config
    }
}

impl Default for BatchProcessor {
    fn default() -> Self {
        Self::new(BatchConfig::default())
    }
}

#[derive(Debug, Clone)]
pub struct BatchStats {
    pub processed_batches: u64,
    pub processed_events: u64,
    pub pending_events: usize,
    pub current_batch_age: Duration,
    pub current_batch_size: usize,
}

#[derive(Debug)]
pub struct BatchQueue {
    queue: Arc<Mutex<VecDeque<EventBatch>>>,
    _config: BatchConfig,
}

impl BatchQueue {
    #[inline]
    pub fn new(config: BatchConfig) -> Self {
        Self {
            queue: Arc::new(Mutex::new(VecDeque::new())),
            _config: config,
        }
    }
    
    #[inline]
    pub fn enqueue(&self, batch: EventBatch) {
        self.queue.lock().push_back(batch);
    }
    
    #[inline]
    pub fn dequeue(&self) -> Option<EventBatch> {
        self.queue.lock().pop_front()
    }
    
    #[inline]
    pub fn len(&self) -> usize {
        self.queue.lock().len()
    }
    
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.queue.lock().is_empty()
    }
    
    #[inline]
    pub fn clear(&self) {
        self.queue.lock().clear();
    }
    
    #[inline]
    pub fn total_events(&self) -> usize {
        self.queue.lock().iter().map(|batch| batch.len()).sum()
    }
}

impl Default for BatchQueue {
    fn default() -> Self {
        Self::new(BatchConfig::default())
    }
}