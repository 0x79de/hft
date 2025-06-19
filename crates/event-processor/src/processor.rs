use crate::events::Event;
use crate::channels::{EventChannels, PriorityQueue};
use crate::batch::{BatchProcessor, BatchConfig, EventBatch};
use std::sync::Arc;
use std::time::Duration;
use parking_lot::RwLock;
use tokio::task::JoinHandle;
use tokio::time::interval;
use crossbeam_channel::select;
use anyhow::Result;

pub type EventHandler = Arc<dyn Fn(&Event) -> Result<()> + Send + Sync>;
pub type BatchHandler = Arc<dyn Fn(&EventBatch) -> Result<()> + Send + Sync>;

#[derive(Debug)]
pub struct ProcessorConfig {
    pub batch_config: BatchConfig,
    pub worker_threads: usize,
    pub buffer_size: usize,
    pub flush_interval: Duration,
    pub enable_priority_queue: bool,
}

impl Default for ProcessorConfig {
    fn default() -> Self {
        Self {
            batch_config: BatchConfig::default(),
            worker_threads: num_cpus::get(),
            buffer_size: 10000,
            flush_interval: Duration::from_millis(5),
            enable_priority_queue: true,
        }
    }
}

pub struct EventProcessor {
    config: ProcessorConfig,
    channels: EventChannels,
    priority_queue: PriorityQueue,
    batch_processor: BatchProcessor,
    event_handlers: Arc<RwLock<Vec<EventHandler>>>,
    batch_handlers: Arc<RwLock<Vec<BatchHandler>>>,
    worker_handles: Arc<RwLock<Vec<JoinHandle<()>>>>,
    running: Arc<RwLock<bool>>,
}

impl EventProcessor {
    #[inline]
    pub fn new() -> Self {
        Self::with_config(ProcessorConfig::default())
    }
    
    #[inline]
    pub fn with_config(config: ProcessorConfig) -> Self {
        let channels = EventChannels::new(config.buffer_size);
        let batch_processor = BatchProcessor::new(config.batch_config.clone());
        
        Self {
            config,
            channels,
            priority_queue: PriorityQueue::new(),
            batch_processor,
            event_handlers: Arc::new(RwLock::new(Vec::new())),
            batch_handlers: Arc::new(RwLock::new(Vec::new())),
            worker_handles: Arc::new(RwLock::new(Vec::new())),
            running: Arc::new(RwLock::new(false)),
        }
    }
    
    #[inline]
    pub fn add_event_handler(&self, handler: EventHandler) {
        self.event_handlers.write().push(handler);
    }
    
    #[inline]
    pub fn add_batch_handler(&self, handler: BatchHandler) {
        self.batch_handlers.write().push(handler);
    }
    
    #[inline]
    pub fn send_event(&self, event: Event) -> Result<()> {
        if self.config.enable_priority_queue {
            self.priority_queue.push(event);
        } else {
            self.channels.send_event(event)?;
        }
        Ok(())
    }
    
    pub async fn start(&self) -> Result<()> {
        if *self.running.read() {
            return Ok(());
        }
        
        *self.running.write() = true;
        
        let mut handles = Vec::new();
        
        for i in 0..self.config.worker_threads {
            let handle = self.spawn_worker(i).await?;
            handles.push(handle);
        }
        
        let flush_handle = self.spawn_flush_worker().await?;
        handles.push(flush_handle);
        
        *self.worker_handles.write() = handles;
        
        tracing::info!("Event processor started with {} worker threads", self.config.worker_threads);
        Ok(())
    }
    
    pub async fn stop(&self) -> Result<()> {
        *self.running.write() = false;
        
        let handles = {
            let mut worker_handles = self.worker_handles.write();
            std::mem::take(&mut *worker_handles)
        };
        
        for handle in handles {
            handle.abort();
        }
        
        tracing::info!("Event processor stopped");
        Ok(())
    }
    
    #[inline]
    pub fn is_running(&self) -> bool {
        *self.running.read()
    }
    
    #[inline]
    pub fn channels(&self) -> &EventChannels {
        &self.channels
    }
    
    #[inline]
    pub fn priority_queue(&self) -> &PriorityQueue {
        &self.priority_queue
    }
    
    #[inline]
    pub fn batch_processor(&self) -> &BatchProcessor {
        &self.batch_processor
    }
    
    async fn spawn_worker(&self, worker_id: usize) -> Result<JoinHandle<()>> {
        let channels = self.channels.clone();
        let priority_queue = self.priority_queue.clone();
        let batch_processor = self.batch_processor.clone();
        let event_handlers = Arc::clone(&self.event_handlers);
        let batch_handlers = Arc::clone(&self.batch_handlers);
        let running = Arc::clone(&self.running);
        let enable_priority = self.config.enable_priority_queue;
        
        let handle = tokio::spawn(async move {
            tracing::debug!("Worker {} started", worker_id);
            
            while *running.read() {
                let event = if enable_priority {
                    priority_queue.pop()
                } else {
                    select! {
                        recv(channels.order_receiver()) -> result => result.ok(),
                        recv(channels.trade_receiver()) -> result => result.ok(),
                        recv(channels.system_receiver()) -> result => result.ok(),
                        default(Duration::from_millis(10)) => None,
                    }
                };
                
                if let Some(event) = event {
                    let handlers = event_handlers.read();
                    for handler in handlers.iter() {
                        if let Err(e) = handler(&event) {
                            tracing::error!("Event handler error: {}", e);
                        }
                    }
                    
                    if let Some(batch) = batch_processor.add_event(event) {
                        let batch_handlers = batch_handlers.read();
                        for handler in batch_handlers.iter() {
                            if let Err(e) = handler(&batch) {
                                tracing::error!("Batch handler error: {}", e);
                            }
                        }
                        batch_processor.mark_batch_processed(&batch);
                    }
                } else {
                    tokio::task::yield_now().await;
                }
            }
            
            tracing::debug!("Worker {} stopped", worker_id);
        });
        
        Ok(handle)
    }
    
    async fn spawn_flush_worker(&self) -> Result<JoinHandle<()>> {
        let batch_processor = self.batch_processor.clone();
        let batch_handlers = Arc::clone(&self.batch_handlers);
        let running = Arc::clone(&self.running);
        let flush_interval = self.config.flush_interval;
        
        let handle = tokio::spawn(async move {
            let mut interval = interval(flush_interval);
            
            while *running.read() {
                interval.tick().await;
                
                if let Some(batch) = batch_processor.flush() {
                    let handlers = batch_handlers.read();
                    for handler in handlers.iter() {
                        if let Err(e) = handler(&batch) {
                            tracing::error!("Batch flush handler error: {}", e);
                        }
                    }
                    batch_processor.mark_batch_processed(&batch);
                }
            }
        });
        
        Ok(handle)
    }
}

impl Default for EventProcessor {
    fn default() -> Self {
        Self::new()
    }
}

impl Drop for EventProcessor {
    fn drop(&mut self) {
        // Try to stop the processor gracefully
        if let Ok(rt) = tokio::runtime::Handle::try_current() {
            // Check if we're in a test environment to avoid nested runtime issues
            if std::env::var("CARGO_TEST").is_ok() || std::thread::current().name().unwrap_or("").contains("test") {
                // In test mode, just set the running flag to false without blocking
                *self.running.write() = false;
                return;
            }
            
            // In production, try to spawn a task to stop gracefully
            let running = self.running.clone();
            let _ = rt.spawn(async move {
                *running.write() = false;
            });
        } else {
            // If no runtime is available, just set the flag
            *self.running.write() = false;
        }
    }
}