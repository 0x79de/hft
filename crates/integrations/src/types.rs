use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TradingSignal {
    pub id: Uuid,
    pub symbol: String,
    pub signal_type: SignalType,
    pub strength: f64,
    pub confidence: f64,
    pub price_target: Option<Decimal>,
    pub stop_loss: Option<Decimal>,
    pub take_profit: Option<Decimal>,
    pub metadata: HashMap<String, serde_json::Value>,
    pub timestamp: DateTime<Utc>,
    pub source: SignalSource,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SignalType {
    Buy,
    Sell,
    Hold,
    StrongBuy,
    StrongSell,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SignalSource {
    OKX,
    MCP,
    RAG,
    Coordinator,
    Combined,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MarketContext {
    pub symbol: String,
    pub current_price: Decimal,
    pub bid: Decimal,
    pub ask: Decimal,
    pub volume_24h: Decimal,
    pub change_24h: Decimal,
    pub volatility: Option<f64>,
    pub order_book_depth: Option<OrderBookDepth>,
    pub timestamp: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrderBookDepth {
    pub bid_depth: Decimal,
    pub ask_depth: Decimal,
    pub spread: Decimal,
    pub imbalance: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PredictionRequest {
    pub request_id: Uuid,
    pub symbol: String,
    pub market_context: MarketContext,
    pub features: HashMap<String, f64>,
    pub prediction_horizon: PredictionHorizon,
    pub timestamp: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub enum PredictionHorizon {
    #[default]
    ShortTerm,  // < 1 minute
    MediumTerm, // 1-5 minutes
    LongTerm,   // > 5 minutes
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PredictionResponse {
    pub request_id: Uuid,
    pub symbol: String,
    pub prediction: TradingPrediction,
    pub confidence: f64,
    pub model_version: String,
    pub processing_time_ms: u64,
    pub timestamp: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TradingPrediction {
    pub direction: PredictionDirection,
    pub price_target: Option<Decimal>,
    pub probability: f64,
    pub risk_score: f64,
    pub factors: Vec<PredictionFactor>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum PredictionDirection {
    Up,
    Down,
    Sideways,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PredictionFactor {
    pub name: String,
    pub weight: f64,
    pub value: f64,
    pub description: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KnowledgeQuery {
    pub query_id: Uuid,
    pub query_text: String,
    pub symbol: Option<String>,
    pub context: HashMap<String, String>,
    pub filters: HashMap<String, String>,
    pub top_k: usize,
    pub threshold: f32,
    pub timestamp: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KnowledgeResponse {
    pub query_id: Uuid,
    pub results: Vec<KnowledgeResult>,
    pub total_score: f64,
    pub processing_time_ms: u64,
    pub timestamp: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KnowledgeResult {
    pub id: String,
    pub content: String,
    pub score: f32,
    pub metadata: HashMap<String, String>,
    pub timestamp: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IntegrationHealth {
    pub overall_status: HealthStatus,
    pub okx_status: HealthStatus,
    pub mcp_status: HealthStatus,
    pub rag_status: HealthStatus,
    pub last_check: DateTime<Utc>,
    pub response_times: ResponseTimes,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub enum HealthStatus {
    Healthy,
    Degraded,
    Unhealthy,
    #[default]
    Unknown,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResponseTimes {
    pub okx_avg_ms: f64,
    pub mcp_avg_ms: f64,
    pub rag_avg_ms: f64,
    pub coordinator_avg_ms: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DecisionContext {
    pub signal_id: Uuid,
    pub symbol: String,
    pub market_context: MarketContext,
    pub prediction: Option<PredictionResponse>,
    pub knowledge: Option<KnowledgeResponse>,
    pub risk_assessment: RiskAssessment,
    pub timestamp: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RiskAssessment {
    pub risk_score: f64,
    pub max_position_size: Decimal,
    pub recommended_stop_loss: Option<Decimal>,
    pub position_limit_used: f64,
    pub volatility_risk: f64,
    pub liquidity_risk: f64,
    pub correlation_risk: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IntegrationMetrics {
    pub requests_per_second: f64,
    pub success_rate: f64,
    pub avg_latency_ms: f64,
    pub p95_latency_ms: f64,
    pub p99_latency_ms: f64,
    pub error_count: u64,
    pub active_connections: u32,
    pub timestamp: DateTime<Utc>,
}

