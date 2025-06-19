use anyhow::Result;
use std::sync::Arc;
use tokio::sync::RwLock;
use tokio::time::{interval, Duration};
use tracing::{info, warn, error, debug};
use std::collections::{HashMap, VecDeque};

use super::client::RagClient;
use super::types::{MarketEvent, MarketEventType};
use crate::types::MarketContext;

#[derive(Debug)]
pub struct MarketEventIngestion {
    client: Arc<RagClient>,
    event_queue: Arc<RwLock<VecDeque<MarketEvent>>>,
    batch_size: usize,
    batch_timeout_ms: u64,
    is_running: Arc<RwLock<bool>>,
}

impl MarketEventIngestion {
    pub fn new(client: Arc<RagClient>) -> Self {
        Self {
            client,
            event_queue: Arc::new(RwLock::new(VecDeque::new())),
            batch_size: 50,
            batch_timeout_ms: 5000, // 5 seconds
            is_running: Arc::new(RwLock::new(false)),
        }
    }
    
    pub async fn start(&self) -> Result<()> {
        let mut is_running = self.is_running.write().await;
        if *is_running {
            warn!("Market event ingestion already running");
            return Ok(());
        }
        *is_running = true;
        drop(is_running);
        
        info!("Starting market event ingestion service");
        
        let client = self.client.clone();
        let event_queue = self.event_queue.clone();
        let is_running = self.is_running.clone();
        let batch_size = self.batch_size;
        let batch_timeout_ms = self.batch_timeout_ms;
        
        tokio::spawn(async move {
            let mut batch_interval = interval(Duration::from_millis(batch_timeout_ms));
            
            loop {
                let running = {
                    let running = is_running.read().await;
                    *running
                };
                
                if !running {
                    info!("Market event ingestion service stopped");
                    break;
                }
                
                batch_interval.tick().await;
                
                // Collect events for batch processing
                let events = {
                    let mut queue = event_queue.write().await;
                    let mut batch = Vec::new();
                    
                    while batch.len() < batch_size && !queue.is_empty() {
                        if let Some(event) = queue.pop_front() {
                            batch.push(event);
                        }
                    }
                    
                    batch
                };
                
                if !events.is_empty() {
                    debug!("Processing batch of {} events", events.len());
                    
                    if let Err(e) = client.batch_ingest_events(events).await {
                        error!("Failed to ingest event batch: {}", e);
                    }
                }
            }
        });
        
        Ok(())
    }
    
    pub async fn stop(&self) -> Result<()> {
        let mut is_running = self.is_running.write().await;
        *is_running = false;
        
        info!("Stopping market event ingestion service");
        
        // Process remaining events
        let remaining_events = {
            let mut queue = self.event_queue.write().await;
            let events: Vec<_> = queue.drain(..).collect();
            events
        };
        
        if !remaining_events.is_empty() {
            info!("Processing {} remaining events before shutdown", remaining_events.len());
            if let Err(e) = self.client.batch_ingest_events(remaining_events).await {
                error!("Failed to process remaining events: {}", e);
            }
        }
        
        Ok(())
    }
    
    pub async fn ingest_event(&self, event: MarketEvent) -> Result<()> {
        let mut queue = self.event_queue.write().await;
        queue.push_back(event);
        
        // If queue is getting too large, process immediately
        if queue.len() >= self.batch_size * 2 {
            warn!("Event queue size ({}) exceeding threshold, triggering immediate processing", queue.len());
            
            let events: Vec<_> = queue.drain(..self.batch_size).collect();
            drop(queue);
            
            if let Err(e) = self.client.batch_ingest_events(events).await {
                error!("Failed to process urgent event batch: {}", e);
            }
        }
        
        Ok(())
    }
    
    pub async fn ingest_market_context(&self, context: MarketContext) -> Result<()> {
        let event: MarketEvent = context.into();
        self.ingest_event(event).await
    }
    
    pub async fn ingest_trade_execution(&self, trade: TradeExecution) -> Result<()> {
        let event = MarketEvent {
            id: uuid::Uuid::new_v4().to_string(),
            timestamp: trade.timestamp,
            event_type: MarketEventType::Trade,
            symbol: trade.symbol.clone(),
            data: serde_json::to_value(&trade)?,
            metadata: {
                let mut meta = HashMap::new();
                meta.insert("trade_id".to_string(), trade.trade_id.clone());
                meta.insert("order_id".to_string(), trade.order_id.clone());
                meta.insert("execution_type".to_string(), format!("{:?}", trade.execution_type));
                meta
            },
        };
        
        self.ingest_event(event).await
    }
    
    pub async fn ingest_price_alert(&self, alert: PriceAlert) -> Result<()> {
        let event = MarketEvent {
            id: uuid::Uuid::new_v4().to_string(),
            timestamp: alert.timestamp,
            event_type: MarketEventType::Alert,
            symbol: alert.symbol.clone(),
            data: serde_json::to_value(&alert)?,
            metadata: {
                let mut meta = HashMap::new();
                meta.insert("alert_type".to_string(), format!("{:?}", alert.alert_type));
                meta.insert("threshold".to_string(), alert.threshold.to_string());
                meta
            },
        };
        
        self.ingest_event(event).await
    }
    
    pub async fn ingest_volume_spike(&self, spike: VolumeSpike) -> Result<()> {
        let event = MarketEvent {
            id: uuid::Uuid::new_v4().to_string(),
            timestamp: spike.timestamp,
            event_type: MarketEventType::VolumeSpike,
            symbol: spike.symbol.clone(),
            data: serde_json::to_value(&spike)?,
            metadata: {
                let mut meta = HashMap::new();
                meta.insert("spike_ratio".to_string(), spike.spike_ratio.to_string());
                meta.insert("duration".to_string(), spike.duration_seconds.to_string());
                meta
            },
        };
        
        self.ingest_event(event).await
    }
    
    pub async fn ingest_technical_signal(&self, signal: TechnicalSignal) -> Result<()> {
        let event = MarketEvent {
            id: uuid::Uuid::new_v4().to_string(),
            timestamp: signal.timestamp,
            event_type: MarketEventType::TechnicalIndicator,
            symbol: signal.symbol.clone(),
            data: serde_json::to_value(&signal)?,
            metadata: {
                let mut meta = HashMap::new();
                meta.insert("indicator".to_string(), signal.indicator.clone());
                meta.insert("signal_type".to_string(), format!("{:?}", signal.signal_type));
                meta.insert("strength".to_string(), signal.strength.to_string());
                meta
            },
        };
        
        self.ingest_event(event).await
    }
    
    pub async fn get_queue_stats(&self) -> QueueStats {
        let queue = self.event_queue.read().await;
        let is_running = {
            let running = self.is_running.read().await;
            *running
        };
        
        QueueStats {
            queue_size: queue.len(),
            is_running,
            batch_size: self.batch_size,
            batch_timeout_ms: self.batch_timeout_ms,
        }
    }
}

// Supporting types for different event ingestion
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct TradeExecution {
    pub trade_id: String,
    pub order_id: String,
    pub symbol: String,
    pub side: String, // "buy" or "sell"
    pub quantity: rust_decimal::Decimal,
    pub price: rust_decimal::Decimal,
    pub fee: rust_decimal::Decimal,
    pub execution_type: ExecutionType,
    pub timestamp: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub enum ExecutionType {
    Market,
    Limit,
    Stop,
    StopLimit,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct PriceAlert {
    pub alert_id: String,
    pub symbol: String,
    pub alert_type: AlertType,
    pub threshold: rust_decimal::Decimal,
    pub current_price: rust_decimal::Decimal,
    pub message: String,
    pub timestamp: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub enum AlertType {
    PriceAbove,
    PriceBelow,
    PercentageChange,
    VolumeThreshold,
    TechnicalIndicator,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct VolumeSpike {
    pub spike_id: String,
    pub symbol: String,
    pub normal_volume: rust_decimal::Decimal,
    pub spike_volume: rust_decimal::Decimal,
    pub spike_ratio: f64,
    pub duration_seconds: u64,
    pub timestamp: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct TechnicalSignal {
    pub signal_id: String,
    pub symbol: String,
    pub indicator: String, // "RSI", "MACD", "Bollinger", etc.
    pub signal_type: TechnicalSignalType,
    pub value: f64,
    pub strength: f64, // 0.0 to 1.0
    pub description: String,
    pub timestamp: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub enum TechnicalSignalType {
    Buy,
    Sell,
    Hold,
    Overbought,
    Oversold,
    Bullish,
    Bearish,
    Breakout,
    Breakdown,
}

#[derive(Debug, Clone)]
pub struct QueueStats {
    pub queue_size: usize,
    pub is_running: bool,
    pub batch_size: usize,
    pub batch_timeout_ms: u64,
}

impl Clone for MarketEventIngestion {
    fn clone(&self) -> Self {
        Self {
            client: self.client.clone(),
            event_queue: Arc::new(RwLock::new(VecDeque::new())),
            batch_size: self.batch_size,
            batch_timeout_ms: self.batch_timeout_ms,
            is_running: Arc::new(RwLock::new(false)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::RagConfig;
    
    async fn create_test_ingestion() -> MarketEventIngestion {
        let config = Arc::new(RagConfig {
            server_url: "http://localhost:8001".to_string(),
            api_key: None,
            timeout_ms: 5000,
            max_retries: 3,
            query_threshold: 0.6,
            top_k: 10,
        });
        
        let client = Arc::new(RagClient::new(config).await.unwrap());
        MarketEventIngestion::new(client)
    }
    
    #[tokio::test]
    async fn test_ingestion_creation() {
        let ingestion = create_test_ingestion().await;
        let stats = ingestion.get_queue_stats().await;
        assert_eq!(stats.queue_size, 0);
        assert!(!stats.is_running);
    }
    
    #[tokio::test]
    async fn test_event_queuing() {
        let ingestion = create_test_ingestion().await;
        
        let event = MarketEvent {
            id: "test-1".to_string(),
            timestamp: chrono::Utc::now(),
            event_type: MarketEventType::Trade,
            symbol: "BTC-USDT".to_string(),
            data: serde_json::json!({"test": "data"}),
            metadata: HashMap::new(),
        };
        
        ingestion.ingest_event(event).await.unwrap();
        
        let stats = ingestion.get_queue_stats().await;
        assert_eq!(stats.queue_size, 1);
    }
}