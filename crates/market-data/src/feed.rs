use crate::types::{Tick, Level2Update, OrderBookSnapshot, MarketSummary};
use crate::stream::{MarketDataStream, MarketEvent};
use crate::snapshot::SnapshotManager;
use crossbeam_channel::Sender;
use parking_lot::RwLock;
use std::sync::Arc;
use std::collections::HashMap;
use chrono::Utc;
use tokio::task;

#[derive(Debug)]
pub struct MarketDataFeed {
    streams: HashMap<String, MarketDataStream>,
    snapshot_manager: Arc<RwLock<SnapshotManager>>,
    global_sender: Option<Sender<MarketEvent>>,
}

impl MarketDataFeed {
    #[inline]
    pub fn new() -> Self {
        Self {
            streams: HashMap::new(),
            snapshot_manager: Arc::new(RwLock::new(SnapshotManager::new())),
            global_sender: None,
        }
    }
    
    #[inline]
    pub fn add_symbol(&mut self, symbol: String) -> Sender<MarketEvent> {
        let stream = MarketDataStream::new();
        let sender = stream.sender();
        self.streams.insert(symbol, stream);
        sender
    }
    
    #[inline]
    pub fn get_stream(&self, symbol: &str) -> Option<&MarketDataStream> {
        self.streams.get(symbol)
    }
    
    #[inline]
    pub fn set_global_sender(&mut self, sender: Sender<MarketEvent>) {
        self.global_sender = Some(sender);
    }
    
    #[inline]
    pub fn publish_tick(&self, tick: Tick) {
        let event = MarketEvent::Tick(tick.clone());
        
        if let Some(stream) = self.streams.get(&tick.symbol) {
            let _ = stream.sender().send(event.clone());
        }
        
        if let Some(global_sender) = &self.global_sender {
            let _ = global_sender.send(event);
        }
        
        let mut manager = self.snapshot_manager.write();
        let summary = manager.get_or_create_summary(&tick.symbol, tick.price);
        summary.update_trade(tick.price, tick.quantity);
    }
    
    #[inline]
    pub fn publish_level2_update(&self, update: Level2Update) {
        let event = MarketEvent::Level2Update(update.clone());
        
        if let Some(stream) = self.streams.get(&update.symbol) {
            let _ = stream.sender().send(event.clone());
        }
        
        if let Some(global_sender) = &self.global_sender {
            let _ = global_sender.send(event);
        }
        
        self.snapshot_manager.write().apply_update(update);
    }
    
    #[inline]
    pub fn publish_snapshot(&self, snapshot: OrderBookSnapshot) {
        let event = MarketEvent::Snapshot(snapshot.clone());
        
        if let Some(stream) = self.streams.get(&snapshot.symbol) {
            let _ = stream.sender().send(event.clone());
        }
        
        if let Some(global_sender) = &self.global_sender {
            let _ = global_sender.send(event);
        }
        
        self.snapshot_manager.write().update_snapshot(snapshot.symbol.clone(), snapshot);
    }
    
    #[inline]
    pub fn get_snapshot(&self, symbol: &str) -> Option<OrderBookSnapshot> {
        self.snapshot_manager.read().get_snapshot(symbol).cloned()
    }
    
    #[inline]
    pub fn get_summary(&self, symbol: &str) -> Option<MarketSummary> {
        self.snapshot_manager.read().get_summary(symbol).cloned()
    }
    
    #[inline]
    pub fn symbols(&self) -> Vec<String> {
        self.streams.keys().cloned().collect()
    }
    
    pub async fn start_heartbeat(&self) {
        if let Some(global_sender) = &self.global_sender {
            let sender = global_sender.clone();
            task::spawn(async move {
                let mut interval = tokio::time::interval(std::time::Duration::from_secs(1));
                loop {
                    interval.tick().await;
                    if sender.send(MarketEvent::Heartbeat).is_err() {
                        break;
                    }
                }
            });
        }
    }
    
    #[inline]
    pub fn cleanup_old_data(&self) {
        let cutoff = Utc::now() - chrono::Duration::hours(24);
        self.snapshot_manager.write().clear_old_snapshots(cutoff);
    }
}

impl Default for MarketDataFeed {
    fn default() -> Self {
        Self::new()
    }
}