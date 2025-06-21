use anyhow::{Result, anyhow};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::{mpsc, RwLock, Mutex};
use tokio::time::interval;
use tracing::{info, warn, error, debug};
use std::collections::HashMap;
use uuid::Uuid;
use rust_decimal::prelude::ToPrimitive;

use crate::config::IntegrationConfig;
#[cfg(test)]
use crate::config::CoordinatorConfig;
use crate::types::*;
use crate::okx::OkxIntegration;
use crate::mcp::McpIntegration;
use crate::rag::RagIntegration;

#[derive(Debug)]
pub struct IntegrationCoordinator {
    config: Arc<IntegrationConfig>,
    okx: Arc<OkxIntegration>,
    mcp: Arc<McpIntegration>,
    rag: Arc<RagIntegration>,
    signal_tx: mpsc::UnboundedSender<TradingSignal>,
    signal_rx: Arc<Mutex<Option<mpsc::UnboundedReceiver<TradingSignal>>>>,
    is_running: Arc<RwLock<bool>>,
    metrics: Arc<RwLock<IntegrationMetrics>>,
    active_requests: Arc<RwLock<HashMap<Uuid, ActiveRequest>>>,
}

#[derive(Debug, Clone)]
struct ActiveRequest {
    request_id: Uuid,
    symbol: String,
    start_time: Instant,
    request_type: RequestType,
}

#[derive(Debug, Clone)]
enum RequestType {
    MarketData,
    Prediction,
    KnowledgeQuery,
    Trading,
}

impl IntegrationCoordinator {
    pub async fn new(config: Arc<IntegrationConfig>) -> Result<Self> {
        info!("Initializing Integration Coordinator");
        
        // Initialize all integrations
        let okx = Arc::new(OkxIntegration::new(config.okx.clone()).await?);
        let mcp = Arc::new(McpIntegration::new(config.mcp.clone()).await?);
        let rag = Arc::new(RagIntegration::new(config.rag.clone()).await?);
        
        let (signal_tx, signal_rx) = mpsc::unbounded_channel();
        
        let metrics = Arc::new(RwLock::new(IntegrationMetrics {
            requests_per_second: 0.0,
            success_rate: 0.0,
            avg_latency_ms: 0.0,
            p95_latency_ms: 0.0,
            p99_latency_ms: 0.0,
            error_count: 0,
            active_connections: 0,
            timestamp: chrono::Utc::now(),
        }));
        
        Ok(Self {
            config,
            okx,
            mcp,
            rag,
            signal_tx,
            signal_rx: Arc::new(Mutex::new(Some(signal_rx))),
            is_running: Arc::new(RwLock::new(false)),
            metrics,
            active_requests: Arc::new(RwLock::new(HashMap::new())),
        })
    }
    
    pub async fn start(&self) -> Result<()> {
        let mut is_running = self.is_running.write().await;
        if *is_running {
            warn!("Integration coordinator already running");
            return Ok(());
        }
        *is_running = true;
        drop(is_running);
        
        info!("Starting Integration Coordinator");
        
        // Start all integrations
        self.okx.start().await?;
        self.rag.ingestion.start().await?;
        
        // Start coordinator services
        self.start_signal_processor().await?;
        self.start_health_monitor().await?;
        self.start_metrics_collector().await?;
        
        info!("Integration Coordinator started successfully");
        Ok(())
    }
    
    pub async fn stop(&self) -> Result<()> {
        let mut is_running = self.is_running.write().await;
        *is_running = false;
        
        info!("Stopping Integration Coordinator");
        
        // Stop all integrations
        self.okx.stop().await?;
        self.rag.ingestion.stop().await?;
        
        info!("Integration Coordinator stopped");
        Ok(())
    }
    
    async fn start_signal_processor(&self) -> Result<()> {
        let signal_rx = {
            let mut rx_option = self.signal_rx.lock().await;
            rx_option.take()
        };
        
        if let Some(mut rx) = signal_rx {
            let coordinator = self.clone();
            
            tokio::spawn(async move {
                info!("Signal processor started");
                
                while let Some(signal) = rx.recv().await {
                    if let Err(e) = coordinator.process_trading_signal(signal).await {
                        error!("Failed to process trading signal: {}", e);
                    }
                }
                
                info!("Signal processor stopped");
            });
        }
        
        Ok(())
    }
    
    async fn start_health_monitor(&self) -> Result<()> {
        let coordinator = self.clone();
        let health_check_interval = coordinator.config.coordinator.health_check_interval_ms;
        
        tokio::spawn(async move {
            let mut interval = interval(Duration::from_millis(health_check_interval));
            
            loop {
                let is_running = {
                    let running = coordinator.is_running.read().await;
                    *running
                };
                
                if !is_running {
                    break;
                }
                
                interval.tick().await;
                
                if let Err(e) = coordinator.perform_health_checks().await {
                    error!("Health check failed: {}", e);
                }
            }
        });
        
        Ok(())
    }
    
    async fn start_metrics_collector(&self) -> Result<()> {
        let coordinator = self.clone();
        
        tokio::spawn(async move {
            let mut interval = interval(Duration::from_secs(10)); // Update metrics every 10 seconds
            
            loop {
                let is_running = {
                    let running = coordinator.is_running.read().await;
                    *running
                };
                
                if !is_running {
                    break;
                }
                
                interval.tick().await;
                
                if let Err(e) = coordinator.update_metrics().await {
                    error!("Failed to update metrics: {}", e);
                }
            }
        });
        
        Ok(())
    }
    
    pub async fn generate_trading_signal(&self, symbol: &str) -> Result<TradingSignal> {
        let request_id = Uuid::new_v4();
        let start_time = Instant::now();
        
        debug!("Generating trading signal for {}", symbol);
        
        // Track active request
        self.track_request(ActiveRequest {
            request_id,
            symbol: symbol.to_string(),
            start_time,
            request_type: RequestType::Trading,
        }).await;
        
        // Track market data request
        self.track_request(ActiveRequest {
            request_id: Uuid::new_v4(),
            symbol: symbol.to_string(),
            start_time: Instant::now(),
            request_type: RequestType::MarketData,
        }).await;
        
        // Get market context from OKX
        let market_context = match self.okx.get_market_context(symbol).await {
            Ok(context) => context,
            Err(e) => {
                self.untrack_request(request_id).await;
                return Err(anyhow!("Failed to get market context: {}", e));
            }
        };
        
        // Extract features for MCP  
        let features = {
            let mut extractor = crate::mcp::FeatureExtractor::new();
            let mut features = extractor.extract_features(&market_context);
            extractor.normalize_features(&mut features);
            features
        };
        
        // Create prediction request
        let prediction_request = PredictionRequest {
            request_id,
            symbol: symbol.to_string(),
            market_context: market_context.clone(),
            features,
            prediction_horizon: PredictionHorizon::ShortTerm,
            timestamp: chrono::Utc::now(),
        };
        
        // Track prediction request
        self.track_request(ActiveRequest {
            request_id: Uuid::new_v4(),
            symbol: symbol.to_string(),
            start_time: Instant::now(),
            request_type: RequestType::Prediction,
        }).await;
        
        // Get AI prediction from MCP
        let prediction_response = self.mcp.get_prediction(prediction_request).await.ok();
        
        // Query knowledge base from RAG
        let knowledge_query = KnowledgeQuery {
            query_id: Uuid::new_v4(),
            query_text: format!("trading patterns similar to current {} market conditions", symbol),
            symbol: Some(symbol.to_string()),
            context: {
                let mut ctx = HashMap::new();
                ctx.insert("current_price".to_string(), market_context.current_price.to_string());
                ctx.insert("volume".to_string(), market_context.volume_24h.to_string());
                ctx
            },
            filters: HashMap::new(),
            top_k: 5,
            threshold: 0.7,
            timestamp: chrono::Utc::now(),
        };
        
        // Track knowledge query request
        self.track_request(ActiveRequest {
            request_id: Uuid::new_v4(),
            symbol: symbol.to_string(),
            start_time: Instant::now(),
            request_type: RequestType::KnowledgeQuery,
        }).await;
        
        let knowledge_response = self.rag.query_knowledge(knowledge_query).await.ok();
        
        // Create decision context
        let decision_context = DecisionContext {
            signal_id: Uuid::new_v4(),
            symbol: symbol.to_string(),
            market_context: market_context.clone(),
            prediction: prediction_response,
            knowledge: knowledge_response,
            risk_assessment: self.assess_risk(&market_context).await,
            timestamp: chrono::Utc::now(),
        };
        
        // Generate consensus-based signal
        let signal = self.generate_consensus_signal(decision_context).await?;
        
        // Ingest the signal into RAG for future learning
        let market_event = crate::rag::types::MarketEvent {
            id: signal.id.to_string(),
            timestamp: signal.timestamp,
            event_type: crate::rag::types::MarketEventType::Signal,
            symbol: signal.symbol.clone(),
            data: serde_json::to_value(&signal)?,
            metadata: {
                let mut meta = HashMap::new();
                meta.insert("signal_strength".to_string(), signal.strength.to_string());
                meta.insert("confidence".to_string(), signal.confidence.to_string());
                meta.insert("source".to_string(), "coordinator".to_string());
                meta
            },
        };
        
        if let Err(e) = self.rag.ingest_market_event(market_event).await {
            warn!("Failed to ingest signal into RAG: {}", e);
        }
        
        self.untrack_request(request_id).await;
        
        let processing_time = start_time.elapsed();
        info!("Generated trading signal for {} in {:?}", symbol, processing_time);
        
        Ok(signal)
    }
    
    async fn generate_consensus_signal(&self, context: DecisionContext) -> Result<TradingSignal> {
        let mut signal_strength = 0.0;
        let mut signal_confidence = 0.0;
        let mut contributing_factors = 0;
        
        // Weight predictions from MCP
        if let Some(ref prediction) = context.prediction {
            contributing_factors += 1;
            
            match prediction.prediction.direction {
                PredictionDirection::Up => {
                    signal_strength += prediction.confidence * 0.4; // 40% weight
                }
                PredictionDirection::Down => {
                    signal_strength -= prediction.confidence * 0.4;
                }
                PredictionDirection::Sideways => {
                    // Neutral prediction, no strength adjustment
                }
            }
            
            signal_confidence += prediction.confidence * 0.4;
        }
        
        // Weight knowledge from RAG
        if let Some(ref knowledge) = context.knowledge {
            if !knowledge.results.is_empty() {
                contributing_factors += 1;
                
                // Analyze historical patterns
                let avg_score = knowledge.results.iter().map(|r| r.score as f64).sum::<f64>() / knowledge.results.len() as f64;
                
                // Simple heuristic: higher scores suggest similar successful patterns
                if avg_score > 0.8 {
                    signal_strength += 0.3; // 30% weight for positive patterns
                } else if avg_score < 0.3 {
                    signal_strength -= 0.3; // Negative patterns
                }
                
                signal_confidence += (avg_score / 100.0) * 0.3;
            }
        }
        
        // Weight market conditions
        let market_score = self.analyze_market_conditions(&context.market_context).await;
        signal_strength += market_score * 0.3; // 30% weight
        signal_confidence += market_score.abs() * 0.3;
        contributing_factors += 1;
        
        // Normalize confidence
        if contributing_factors > 0 {
            signal_confidence /= contributing_factors as f64;
        }
        
        // Determine final signal type based on strength and confidence
        let signal_type = if signal_confidence < self.config.coordinator.consensus_threshold {
            SignalType::Hold
        } else if signal_strength > 0.7 {
            SignalType::StrongBuy
        } else if signal_strength > 0.3 {
            SignalType::Buy
        } else if signal_strength < -0.7 {
            SignalType::StrongSell
        } else if signal_strength < -0.3 {
            SignalType::Sell
        } else {
            SignalType::Hold
        };
        
        Ok(TradingSignal {
            id: context.signal_id,
            symbol: context.symbol,
            signal_type,
            strength: signal_strength.abs(),
            confidence: signal_confidence.clamp(0.0, 1.0),
            price_target: context.prediction.as_ref()
                .and_then(|p| p.prediction.price_target),
            stop_loss: context.prediction.as_ref()
                .and_then(|p| p.prediction.price_target)
                .map(|target| target * rust_decimal::Decimal::new(98, 2)), // 2% stop loss
            take_profit: context.prediction.as_ref()
                .and_then(|p| p.prediction.price_target)
                .map(|target| target * rust_decimal::Decimal::new(105, 2)), // 5% take profit
            metadata: {
                let mut meta = HashMap::new();
                meta.insert("okx_connected".to_string(), serde_json::Value::Bool(true));
                meta.insert("mcp_prediction".to_string(), 
                    serde_json::Value::Bool(context.prediction.is_some()));
                meta.insert("rag_knowledge".to_string(), 
                    serde_json::Value::Bool(context.knowledge.is_some()));
                meta.insert("contributing_factors".to_string(), 
                    serde_json::Value::Number(contributing_factors.into()));
                meta
            },
            timestamp: context.timestamp,
            source: SignalSource::Coordinator,
        })
    }
    
    async fn analyze_market_conditions(&self, context: &MarketContext) -> f64 {
        let mut score: f64 = 0.0;
        
        // Analyze spread
        let spread = context.ask - context.bid;
        let mid_price = (context.bid + context.ask) / rust_decimal::Decimal::from(2);
        if !mid_price.is_zero() {
            let spread_pct = (spread / mid_price).to_f64().unwrap_or(0.0);
            
            // Tighter spreads are generally better for trading
            if spread_pct < 0.001 { // < 0.1%
                score += 0.2;
            } else if spread_pct > 0.01 { // > 1%
                score -= 0.2;
            }
        }
        
        // Analyze order book depth
        if let Some(ref depth) = context.order_book_depth {
            // Balanced order book is good
            if depth.imbalance.abs() < 0.2 {
                score += 0.1;
            } else if depth.imbalance.abs() > 0.5 {
                score -= 0.1;
            }
        }
        
        // Analyze volatility if available
        if let Some(volatility) = context.volatility {
            if volatility > 0.5 { // High volatility
                score -= 0.1; // Generally riskier
            } else if volatility < 0.1 { // Very low volatility
                score += 0.1; // More predictable
            }
        }
        
        score.clamp(-1.0, 1.0)
    }
    
    async fn assess_risk(&self, context: &MarketContext) -> RiskAssessment {
        let volatility_risk = context.volatility.unwrap_or(0.25);
        
        // Calculate liquidity risk from order book
        let liquidity_risk = if let Some(ref depth) = context.order_book_depth {
            let total_depth = depth.bid_depth + depth.ask_depth;
            if total_depth.to_f64().unwrap_or(0.0) > 100.0 {
                0.1 // Low liquidity risk
            } else {
                0.8 // High liquidity risk
            }
        } else {
            0.5 // Unknown liquidity
        };
        
        let risk_score = (volatility_risk + liquidity_risk) / 2.0;
        
        RiskAssessment {
            risk_score,
            max_position_size: rust_decimal::Decimal::new(1000, 0), // $1000 max
            recommended_stop_loss: Some(context.current_price * rust_decimal::Decimal::new(95, 2)), // 5% stop loss
            position_limit_used: 0.0,
            volatility_risk,
            liquidity_risk,
            correlation_risk: volatility_risk * 0.3, // Use volatility as proxy for correlation risk
        }
    }
    
    async fn process_trading_signal(&self, signal: TradingSignal) -> Result<()> {
        info!("Processing trading signal: {:?} for {}", signal.signal_type, signal.symbol);
        
        // Place order through OKX if not a HOLD signal
        if !matches!(signal.signal_type, SignalType::Hold) {
            match self.okx.place_order(&signal).await {
                Ok(order_response) => {
                    info!("Order placed successfully: {:?}", order_response);
                }
                Err(e) => {
                    error!("Failed to place order: {}", e);
                }
            }
        }
        
        Ok(())
    }
    
    async fn perform_health_checks(&self) -> Result<()> {
        debug!("Performing health checks");
        
        let okx_health = self.okx.health_check().await.unwrap_or(HealthStatus::Unknown);
        let mcp_health = self.mcp.health_check().await.unwrap_or(HealthStatus::Unknown);
        let rag_health = self.rag.health_check().await.unwrap_or(HealthStatus::Unknown);
        
        let overall_status = match (okx_health, mcp_health, rag_health) {
            (HealthStatus::Healthy, HealthStatus::Healthy, HealthStatus::Healthy) => HealthStatus::Healthy,
            (HealthStatus::Unhealthy, _, _) | (_, HealthStatus::Unhealthy, _) | (_, _, HealthStatus::Unhealthy) => HealthStatus::Unhealthy,
            _ => HealthStatus::Degraded,
        };
        
        debug!("Health check completed: {:?}", overall_status);
        Ok(())
    }
    
    async fn update_metrics(&self) -> Result<()> {
        let active_count = {
            let requests = self.active_requests.read().await;
            requests.len() as u32
        };
        
        let mut metrics = self.metrics.write().await;
        metrics.active_connections = active_count;
        metrics.timestamp = chrono::Utc::now();
        
        Ok(())
    }
    
    async fn track_request(&self, request: ActiveRequest) {
        debug!("Tracking {} request for symbol {} with ID {}", 
               format!("{:?}", request.request_type), request.symbol, request.request_id);
        let mut requests = self.active_requests.write().await;
        requests.insert(request.request_id, request);
    }
    
    async fn untrack_request(&self, request_id: Uuid) {
        let _request_info = {
            let requests = self.active_requests.read().await;
            requests.get(&request_id).map(|req| {
                let duration = req.start_time.elapsed();
                debug!("Completed {} request for symbol {} in {:?}", 
                       format!("{:?}", req.request_type), req.symbol, duration);
                req.clone()
            })
        };
        
        let mut requests = self.active_requests.write().await;
        requests.remove(&request_id);
    }
    
    pub async fn health_check(&self) -> Result<IntegrationHealth> {
        let okx_status = self.okx.health_check().await.unwrap_or(HealthStatus::Unknown);
        let mcp_status = self.mcp.health_check().await.unwrap_or(HealthStatus::Unknown);
        let rag_status = self.rag.health_check().await.unwrap_or(HealthStatus::Unknown);
        
        let overall_status = match (okx_status.clone(), mcp_status.clone(), rag_status.clone()) {
            (HealthStatus::Healthy, HealthStatus::Healthy, HealthStatus::Healthy) => HealthStatus::Healthy,
            (HealthStatus::Unhealthy, _, _) | (_, HealthStatus::Unhealthy, _) | (_, _, HealthStatus::Unhealthy) => HealthStatus::Unhealthy,
            _ => HealthStatus::Degraded,
        };
        
        Ok(IntegrationHealth {
            overall_status,
            okx_status,
            mcp_status,
            rag_status,
            last_check: chrono::Utc::now(),
            response_times: ResponseTimes {
                okx_avg_ms: 10.0, // TODO: Calculate from metrics
                mcp_avg_ms: 50.0,
                rag_avg_ms: 100.0,
                coordinator_avg_ms: 25.0,
            },
        })
    }
    
    pub async fn get_metrics(&self) -> IntegrationMetrics {
        let metrics = self.metrics.read().await;
        metrics.clone()
    }
    
    pub fn get_signal_sender(&self) -> mpsc::UnboundedSender<TradingSignal> {
        self.signal_tx.clone()
    }
}

impl Clone for IntegrationCoordinator {
    fn clone(&self) -> Self {
        let (signal_tx, signal_rx) = mpsc::unbounded_channel();
        
        Self {
            config: self.config.clone(),
            okx: self.okx.clone(),
            mcp: self.mcp.clone(),
            rag: self.rag.clone(),
            signal_tx,
            signal_rx: Arc::new(Mutex::new(Some(signal_rx))),
            is_running: Arc::new(RwLock::new(false)),
            metrics: self.metrics.clone(),
            active_requests: Arc::new(RwLock::new(HashMap::new())),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{OkxConfig, McpConfig, RagConfig};
    
    async fn create_test_coordinator() -> Result<IntegrationCoordinator> {
        let config = IntegrationConfig {
            okx: OkxConfig {
                api_key: "test_key".to_string(),
                secret_key: "dGVzdF9zZWNyZXQ=".to_string(),
                passphrase: "test_passphrase".to_string(),
                sandbox: true,
                base_url: None,
                timeout_ms: 5000,
                rate_limit_requests_per_second: 10,
            },
            mcp: McpConfig {
                server_url: "http://localhost:8000".to_string(),
                api_key: None,
                timeout_ms: 1000,
                max_retries: 3,
                prediction_threshold: 0.7,
            },
            rag: RagConfig {
                server_url: "http://localhost:8001".to_string(),
                api_key: None,
                timeout_ms: 500,
                max_retries: 2,
                query_threshold: 0.6,
                top_k: 10,
            },
            coordinator: CoordinatorConfig::default(),
        };
        
        IntegrationCoordinator::new(Arc::new(config)).await
    }
    
    #[tokio::test]
    async fn test_coordinator_creation() {
        let coordinator = create_test_coordinator().await;
        assert!(coordinator.is_ok());
    }
    
    #[tokio::test]
    async fn test_health_check() {
        let coordinator = create_test_coordinator().await.unwrap();
        let health = coordinator.health_check().await;
        assert!(health.is_ok());
    }
}