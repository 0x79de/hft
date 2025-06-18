use crate::types::{Tick, Level2Update, OrderBookSnapshot};
use crossbeam_channel::{Receiver, Sender, unbounded};
use parking_lot::RwLock;
use std::sync::Arc;
use std::thread;
use std::time::Duration;
use tokio::time::{interval, Interval};
use futures::stream::Stream;
use std::pin::Pin;
use std::task::{Context, Poll};

#[derive(Debug, Clone)]
pub enum MarketEvent {
    Tick(Tick),
    Level2Update(Level2Update),
    Snapshot(OrderBookSnapshot),
    Heartbeat,
}

#[derive(Debug)]
pub struct MarketDataStream {
    receiver: Receiver<MarketEvent>,
    sender: Sender<MarketEvent>,
}

impl MarketDataStream {
    #[inline]
    pub fn new() -> Self {
        let (sender, receiver) = unbounded();
        Self { receiver, sender }
    }
    
    #[inline]
    pub fn sender(&self) -> Sender<MarketEvent> {
        self.sender.clone()
    }
    
    #[inline]
    pub fn try_recv(&self) -> Result<MarketEvent, crossbeam_channel::TryRecvError> {
        self.receiver.try_recv()
    }
    
    #[inline]
    pub fn recv(&self) -> Result<MarketEvent, crossbeam_channel::RecvError> {
        self.receiver.recv()
    }
    
    #[inline]
    pub fn recv_timeout(&self, timeout: Duration) -> Result<MarketEvent, crossbeam_channel::RecvTimeoutError> {
        self.receiver.recv_timeout(timeout)
    }
    
    pub fn into_async_stream(self) -> AsyncMarketStream {
        AsyncMarketStream::new(self.receiver)
    }
}

impl Default for MarketDataStream {
    fn default() -> Self {
        Self::new()
    }
}

pub struct AsyncMarketStream {
    receiver: Receiver<MarketEvent>,
    heartbeat_interval: Interval,
}

impl AsyncMarketStream {
    pub fn new(receiver: Receiver<MarketEvent>) -> Self {
        Self {
            receiver,
            heartbeat_interval: interval(Duration::from_secs(1)),
        }
    }
}

impl Stream for AsyncMarketStream {
    type Item = MarketEvent;
    
    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        match self.receiver.try_recv() {
            Ok(event) => Poll::Ready(Some(event)),
            Err(crossbeam_channel::TryRecvError::Empty) => {
                if self.heartbeat_interval.poll_tick(cx).is_ready() {
                    Poll::Ready(Some(MarketEvent::Heartbeat))
                } else {
                    Poll::Pending
                }
            }
            Err(crossbeam_channel::TryRecvError::Disconnected) => Poll::Ready(None),
        }
    }
}

#[derive(Debug)]
pub struct StreamProcessor {
    streams: Vec<MarketDataStream>,
    processor_handle: Option<thread::JoinHandle<()>>,
    should_stop: Arc<RwLock<bool>>,
}

impl StreamProcessor {
    #[inline]
    pub fn new() -> Self {
        Self {
            streams: Vec::new(),
            processor_handle: None,
            should_stop: Arc::new(RwLock::new(false)),
        }
    }
    
    #[inline]
    pub fn add_stream(&mut self, stream: MarketDataStream) {
        self.streams.push(stream);
    }
    
    pub fn start<F>(&mut self, mut callback: F) 
    where
        F: FnMut(MarketEvent) + Send + 'static,
    {
        if self.processor_handle.is_some() {
            return;
        }
        
        let receivers: Vec<_> = self.streams.iter().map(|s| s.receiver.clone()).collect();
        let should_stop = Arc::clone(&self.should_stop);
        
        self.processor_handle = Some(thread::spawn(move || {
            let mut selector = crossbeam_channel::Select::new();
            for receiver in &receivers {
                selector.recv(receiver);
            }
            
            loop {
                if *should_stop.read() {
                    break;
                }
                
                let selected = selector.select_timeout(Duration::from_millis(100));
                match selected {
                    Ok(operation) => {
                        let index = operation.index();
                        if let Ok(event) = operation.recv(&receivers[index]) {
                            callback(event);
                        }
                    }
                    Err(crossbeam_channel::SelectTimeoutError) => {
                        callback(MarketEvent::Heartbeat);
                    }
                }
            }
        }));
    }
    
    #[inline]
    pub fn stop(&mut self) {
        *self.should_stop.write() = true;
        if let Some(handle) = self.processor_handle.take() {
            let _ = handle.join();
        }
    }
}

impl Drop for StreamProcessor {
    fn drop(&mut self) {
        self.stop();
    }
}

impl Default for StreamProcessor {
    fn default() -> Self {
        Self::new()
    }
}