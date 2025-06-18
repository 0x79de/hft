//! Trading engine implementation

use order_book::OrderBook;
use event_processor::EventProcessor;
use latency_profiler::LatencyProfiler;
use anyhow::Result;
use std::collections::HashMap;
use tokio::sync::RwLock;
use std::sync::Arc;

pub struct TradingEngine {
    order_books: HashMap<String, Arc<RwLock<OrderBook>>>,
    event_processor: EventProcessor,
    latency_profiler: LatencyProfiler,
    running: bool,
}

impl TradingEngine {
    pub async fn new() -> Result<Self> {
        Ok(Self {
            order_books: HashMap::new(),
            event_processor: EventProcessor::new(),
            latency_profiler: LatencyProfiler::new(),
            running: false,
        })
    }

    pub async fn start(&mut self) -> Result<()> {
        self.running = true;
        self.event_processor.start().await?;
        Ok(())
    }

    pub fn add_symbol(&mut self, symbol: String) {
        let order_book = Arc::new(RwLock::new(OrderBook::new(symbol.clone())));
        self.order_books.insert(symbol, order_book);
    }

    pub fn is_running(&self) -> bool {
        self.running
    }
}