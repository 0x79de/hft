use crate::types::{OrderBookSnapshot, MarketSummary, Level2Update};
use order_book::{Price, Side};
use std::collections::BTreeMap;
use chrono::{DateTime, Utc};

#[derive(Debug, Clone)]
pub struct SnapshotManager {
    snapshots: BTreeMap<String, OrderBookSnapshot>,
    summaries: BTreeMap<String, MarketSummary>,
}

impl SnapshotManager {
    #[inline]
    pub fn new() -> Self {
        Self {
            snapshots: BTreeMap::new(),
            summaries: BTreeMap::new(),
        }
    }
    
    #[inline]
    pub fn update_snapshot(&mut self, symbol: String, snapshot: OrderBookSnapshot) {
        self.snapshots.insert(symbol, snapshot);
    }
    
    #[inline]
    pub fn get_snapshot(&self, symbol: &str) -> Option<&OrderBookSnapshot> {
        self.snapshots.get(symbol)
    }
    
    #[inline]
    pub fn apply_update(&mut self, update: Level2Update) {
        if let Some(snapshot) = self.snapshots.get_mut(&update.symbol) {
            let levels = match update.side {
                Side::Buy => &mut snapshot.bids,
                Side::Sell => &mut snapshot.asks,
            };
            
            match update.update_type {
                crate::types::UpdateType::Add | crate::types::UpdateType::Update => {
                    if let Some(pos) = levels.iter().position(|(price, _)| *price == update.price) {
                        levels[pos].1 = update.quantity;
                    } else {
                        levels.push((update.price, update.quantity));
                        levels.sort_by(|a, b| {
                            match update.side {
                                Side::Buy => b.0.cmp(&a.0),
                                Side::Sell => a.0.cmp(&b.0),
                            }
                        });
                    }
                }
                crate::types::UpdateType::Delete => {
                    levels.retain(|(price, _)| *price != update.price);
                }
            }
            
            snapshot.timestamp = update.timestamp;
        }
    }
    
    #[inline]
    pub fn update_summary(&mut self, symbol: String, summary: MarketSummary) {
        self.summaries.insert(symbol, summary);
    }
    
    #[inline]
    pub fn get_summary(&self, symbol: &str) -> Option<&MarketSummary> {
        self.summaries.get(symbol)
    }
    
    #[inline]
    pub fn get_or_create_summary(&mut self, symbol: &str, open_price: Price) -> &mut MarketSummary {
        self.summaries
            .entry(symbol.to_string())
            .or_insert_with(|| MarketSummary::new(symbol.to_string(), open_price))
    }
    
    #[inline]
    pub fn symbols(&self) -> impl Iterator<Item = &String> {
        self.snapshots.keys()
    }
    
    #[inline]
    pub fn snapshot_count(&self) -> usize {
        self.snapshots.len()
    }
    
    #[inline]
    pub fn clear_old_snapshots(&mut self, cutoff_time: DateTime<Utc>) {
        self.snapshots.retain(|_, snapshot| snapshot.timestamp > cutoff_time);
        self.summaries.retain(|_, summary| summary.timestamp > cutoff_time);
    }
}

impl Default for SnapshotManager {
    fn default() -> Self {
        Self::new()
    }
}