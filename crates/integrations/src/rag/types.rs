use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RagQueryRequest {
    pub query: String,
    pub filters: Option<HashMap<String, String>>,
    pub top_k: Option<usize>,
    pub threshold: Option<f32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RagQueryResponse {
    pub query: String,
    pub documents: Vec<RagDocumentResponse>,
    pub metadata: serde_json::Value,
    pub processing_time_ms: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RagDocumentResponse {
    pub id: String,
    pub content: String,
    pub metadata: HashMap<String, String>,
    pub score: f32,
    pub timestamp: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MarketEvent {
    pub id: String,
    pub timestamp: DateTime<Utc>,
    pub event_type: MarketEventType,
    pub symbol: String,
    pub data: serde_json::Value,
    pub metadata: HashMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum MarketEventType {
    Trade,
    Quote,
    OrderBook,
    News,
    Signal,
    Alert,
    PriceMovement,
    VolumeSpike,
    TechnicalIndicator,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PatternSearchQuery {
    pub query_id: Uuid,
    pub symbol: String,
    pub pattern_type: PatternType,
    pub timeframe: TimeFrame,
    pub similarity_threshold: f32,
    pub context: HashMap<String, String>,
    pub timestamp: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub enum PatternType {
    #[default]
    PricePattern,
    VolumePattern,
    TechnicalIndicatorPattern,
    NewsPattern,
    MarketRegimeChange,
    VolatilityCluster,
    TrendReversal,
    BreakoutPattern,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub enum TimeFrame {
    #[serde(rename = "1m")]
    OneMinute,
    #[serde(rename = "5m")]
    #[default]
    FiveMinutes,
    #[serde(rename = "15m")]
    FifteenMinutes,
    #[serde(rename = "1h")]
    OneHour,
    #[serde(rename = "4h")]
    FourHours,
    #[serde(rename = "1d")]
    OneDay,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PatternSearchResponse {
    pub query_id: Uuid,
    pub patterns: Vec<HistoricalPattern>,
    pub total_matches: usize,
    pub confidence_score: f64,
    pub processing_time_ms: u64,
    pub timestamp: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HistoricalPattern {
    pub id: String,
    pub symbol: String,
    pub pattern_type: PatternType,
    pub start_time: DateTime<Utc>,
    pub end_time: DateTime<Utc>,
    pub similarity_score: f32,
    pub outcome: PatternOutcome,
    pub metadata: HashMap<String, String>,
    pub features: Vec<PatternFeature>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PatternOutcome {
    pub direction: String, // "bullish", "bearish", "neutral"
    pub magnitude: f64,    // Price change magnitude
    pub duration: i64,     // Duration in seconds
    pub success_rate: f64, // Historical success rate of this pattern
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PatternFeature {
    pub name: String,
    pub value: f64,
    pub importance: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NewsAnalysisRequest {
    pub news_items: Vec<NewsItem>,
    pub symbol: Option<String>,
    pub analysis_type: NewsAnalysisType,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NewsItem {
    pub id: String,
    pub title: String,
    pub content: String,
    pub source: String,
    pub timestamp: DateTime<Utc>,
    pub url: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum NewsAnalysisType {
    SentimentAnalysis,
    MarketImpact,
    PriceMovementPredictor,
    VolatilityPredictor,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NewsAnalysisResponse {
    pub analysis_id: Uuid,
    pub results: Vec<NewsAnalysisResult>,
    pub overall_sentiment: f64, // -1.0 to 1.0
    pub impact_score: f64,      // 0.0 to 1.0
    pub processing_time_ms: u64,
    pub timestamp: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NewsAnalysisResult {
    pub news_id: String,
    pub sentiment_score: f64,
    pub impact_score: f64,
    pub topics: Vec<String>,
    pub entities: Vec<String>,
    pub relevance_score: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MarketRegimeQuery {
    pub symbol: String,
    pub historical_period: HistoricalPeriod,
    pub regime_indicators: Vec<RegimeIndicator>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum HistoricalPeriod {
    #[serde(rename = "1w")]
    OneWeek,
    #[serde(rename = "1M")]
    OneMonth,
    #[serde(rename = "3M")]
    ThreeMonths,
    #[serde(rename = "6M")]
    SixMonths,
    #[serde(rename = "1Y")]
    OneYear,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum RegimeIndicator {
    Volatility,
    Trend,
    Volume,
    Correlation,
    Momentum,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MarketRegimeResponse {
    pub current_regime: MarketRegime,
    pub regime_probability: f64,
    pub regime_duration: i64, // Duration in current regime (seconds)
    pub historical_regimes: Vec<HistoricalRegime>,
    pub transition_signals: Vec<RegimeTransitionSignal>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MarketRegime {
    pub name: String,
    pub characteristics: HashMap<String, f64>,
    pub volatility_level: VolatilityLevel,
    pub trend_direction: TrendDirection,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub enum VolatilityLevel {
    Low,
    #[default]
    Medium,
    High,
    Extreme,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub enum TrendDirection {
    Bullish,
    Bearish,
    #[default]
    Sideways,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HistoricalRegime {
    pub regime: MarketRegime,
    pub start_time: DateTime<Utc>,
    pub end_time: DateTime<Utc>,
    pub performance: RegimePerformance,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RegimePerformance {
    pub total_return: f64,
    pub volatility: f64,
    pub max_drawdown: f64,
    pub sharpe_ratio: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RegimeTransitionSignal {
    pub signal_type: String,
    pub strength: f64,
    pub confidence: f64,
    pub description: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RagHealthResponse {
    pub status: String,
    pub timestamp: DateTime<Utc>,
    pub version: String,
    pub components: HashMap<String, String>,
}

// Conversion implementations
impl From<crate::types::KnowledgeQuery> for RagQueryRequest {
    fn from(query: crate::types::KnowledgeQuery) -> Self {
        Self {
            query: query.query_text,
            filters: Some(query.filters),
            top_k: Some(query.top_k),
            threshold: Some(query.threshold),
        }
    }
}

impl From<RagQueryResponse> for crate::types::KnowledgeResponse {
    fn from(response: RagQueryResponse) -> Self {
        Self {
            query_id: uuid::Uuid::new_v4(), // Generate new UUID
            results: response.documents.into_iter().map(|doc| {
                crate::types::KnowledgeResult {
                    id: doc.id,
                    content: doc.content,
                    score: doc.score,
                    metadata: doc.metadata,
                    timestamp: doc.timestamp,
                }
            }).collect(),
            total_score: 0.0, // Calculate from individual scores
            processing_time_ms: response.processing_time_ms,
            timestamp: chrono::Utc::now(),
        }
    }
}

impl From<crate::types::MarketContext> for MarketEvent {
    fn from(context: crate::types::MarketContext) -> Self {
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            timestamp: context.timestamp,
            event_type: MarketEventType::Quote,
            symbol: context.symbol,
            data: serde_json::json!({
                "current_price": context.current_price,
                "bid": context.bid,
                "ask": context.ask,
                "volume_24h": context.volume_24h,
                "change_24h": context.change_24h,
                "volatility": context.volatility,
                "order_book_depth": context.order_book_depth
            }),
            metadata: HashMap::new(),
        }
    }
}