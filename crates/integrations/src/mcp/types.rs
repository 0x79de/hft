use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use rust_decimal::prelude::ToPrimitive;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpPredictionRequest {
    pub request_id: String,
    pub symbol: String,
    pub market_context: McpMarketContext,
    pub features: HashMap<String, f64>,
    pub model_config: Option<ModelConfig>,
    pub timestamp: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpMarketContext {
    pub symbol: String,
    pub current_price: f64,
    pub bid: f64,
    pub ask: f64,
    pub volume_24h: f64,
    pub change_24h: f64,
    pub volatility: Option<f64>,
    pub rsi: Option<f64>,
    pub macd: Option<f64>,
    pub bollinger_bands: Option<BollingerBands>,
    pub order_book_imbalance: Option<f64>,
    pub timestamp: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BollingerBands {
    pub upper: f64,
    pub middle: f64,
    pub lower: f64,
    pub width: f64,
    pub position: f64, // Position relative to bands (0.0 = lower, 0.5 = middle, 1.0 = upper)
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelConfig {
    pub model_version: Option<String>,
    pub prediction_horizon: String, // "1m", "5m", "15m", etc.
    pub confidence_threshold: f64,
    pub risk_tolerance: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpPredictionResponse {
    pub request_id: String,
    pub symbol: String,
    pub prediction: McpPrediction,
    pub confidence: f64,
    pub model_version: String,
    pub processing_time_ms: u64,
    pub features_used: Vec<String>,
    pub timestamp: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpPrediction {
    pub direction: String, // "up", "down", "sideways"
    pub price_target: Option<f64>,
    pub probability: f64,
    pub risk_score: f64,
    pub strength: f64, // Signal strength 0.0 - 1.0
    pub time_horizon: String,
    pub factors: Vec<PredictionFactor>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PredictionFactor {
    pub name: String,
    pub weight: f64,
    pub value: f64,
    pub impact: String, // "positive", "negative", "neutral"
    pub description: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelInfo {
    pub model_name: String,
    pub version: String,
    pub training_date: DateTime<Utc>,
    pub accuracy: f64,
    pub supported_symbols: Vec<String>,
    pub features: Vec<FeatureInfo>,
    pub performance_metrics: PerformanceMetrics,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FeatureInfo {
    pub name: String,
    pub description: String,
    pub importance: f64,
    pub data_type: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PerformanceMetrics {
    pub accuracy: f64,
    pub precision: f64,
    pub recall: f64,
    pub f1_score: f64,
    pub sharpe_ratio: f64,
    pub max_drawdown: f64,
    pub total_trades: u64,
    pub win_rate: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpHealthResponse {
    pub status: String,
    pub timestamp: DateTime<Utc>,
    pub version: String,
    pub uptime_seconds: u64,
    pub models_loaded: u32,
    pub active_predictions: u32,
    pub avg_response_time_ms: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpErrorResponse {
    pub error: String,
    pub code: String,
    pub details: Option<HashMap<String, serde_json::Value>>,
    pub timestamp: DateTime<Utc>,
}

// Request/Response wrappers for the HFT-MCP server API
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpApiRequest<T> {
    pub data: T,
    pub metadata: HashMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpApiResponse<T> {
    pub success: bool,
    pub data: Option<T>,
    pub error: Option<String>,
    pub timestamp: DateTime<Utc>,
}

impl From<crate::types::MarketContext> for McpMarketContext {
    fn from(ctx: crate::types::MarketContext) -> Self {
        Self {
            symbol: ctx.symbol,
            current_price: ctx.current_price.to_f64().unwrap_or(0.0),
            bid: ctx.bid.to_f64().unwrap_or(0.0),
            ask: ctx.ask.to_f64().unwrap_or(0.0),
            volume_24h: ctx.volume_24h.to_f64().unwrap_or(0.0),
            change_24h: ctx.change_24h.to_f64().unwrap_or(0.0),
            volatility: ctx.volatility,
            rsi: None,
            macd: None,
            bollinger_bands: None,
            order_book_imbalance: ctx.order_book_depth.map(|d| d.imbalance),
            timestamp: ctx.timestamp,
        }
    }
}

impl From<crate::types::PredictionRequest> for McpPredictionRequest {
    fn from(req: crate::types::PredictionRequest) -> Self {
        Self {
            request_id: req.request_id.to_string(),
            symbol: req.symbol,
            market_context: req.market_context.into(),
            features: req.features,
            model_config: Some(ModelConfig {
                model_version: None,
                prediction_horizon: match req.prediction_horizon {
                    crate::types::PredictionHorizon::ShortTerm => "1m".to_string(),
                    crate::types::PredictionHorizon::MediumTerm => "5m".to_string(),
                    crate::types::PredictionHorizon::LongTerm => "15m".to_string(),
                },
                confidence_threshold: 0.7,
                risk_tolerance: 0.5,
            }),
            timestamp: req.timestamp,
        }
    }
}

impl From<McpPredictionResponse> for crate::types::PredictionResponse {
    fn from(resp: McpPredictionResponse) -> Self {
        Self {
            request_id: Uuid::parse_str(&resp.request_id).unwrap_or_else(|_| Uuid::new_v4()),
            symbol: resp.symbol,
            prediction: crate::types::TradingPrediction {
                direction: match resp.prediction.direction.as_str() {
                    "up" => crate::types::PredictionDirection::Up,
                    "down" => crate::types::PredictionDirection::Down,
                    _ => crate::types::PredictionDirection::Sideways,
                },
                price_target: resp.prediction.price_target.map(Decimal::from_f64_retain).flatten(),
                probability: resp.prediction.probability,
                risk_score: resp.prediction.risk_score,
                factors: resp.prediction.factors.into_iter().map(|f| {
                    crate::types::PredictionFactor {
                        name: f.name,
                        weight: f.weight,
                        value: f.value,
                        description: f.description,
                    }
                }).collect(),
            },
            confidence: resp.confidence,
            model_version: resp.model_version,
            processing_time_ms: resp.processing_time_ms,
            timestamp: resp.timestamp,
        }
    }
}