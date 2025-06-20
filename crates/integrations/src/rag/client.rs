use anyhow::{Result, anyhow};
use reqwest::Client;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::time::sleep;
use tracing::{info, warn, error, debug};

use crate::config::RagConfig;
use crate::types::{KnowledgeQuery, KnowledgeResponse, HealthStatus};
use super::types::*;

#[derive(Debug, Clone)]
pub struct RagClient {
    client: Client,
    base_url: String,
    config: Arc<RagConfig>,
}

impl RagClient {
    pub async fn new(config: Arc<RagConfig>) -> Result<Self> {
        let client = Client::builder()
            .timeout(Duration::from_millis(config.timeout_ms))
            .user_agent("HFT-Integrations/1.0")
            .build()?;
        
        let base_url = config.server_url.trim_end_matches('/').to_string();
        
        info!("Initializing RAG client for server: {}", base_url);
        
        Ok(Self {
            client,
            base_url,
            config,
        })
    }
    
    async fn make_request<T, R>(&self, endpoint: &str, request_data: T) -> Result<R>
    where
        T: serde::Serialize,
        R: for<'de> serde::Deserialize<'de>,
    {
        let url = format!("{}/{}", self.base_url, endpoint.trim_start_matches('/'));
        
        let mut request = self.client.post(&url)
            .header("Content-Type", "application/json");
        
        // Add API key if configured
        if let Some(ref api_key) = self.config.api_key {
            request = request.header("Authorization", format!("Bearer {}", api_key));
        }
        
        let body = serde_json::to_string(&request_data)?;
        debug!("Sending RAG request to {}: {}", url, body);
        
        let response = request.body(body).send().await?;
        let status = response.status();
        
        if !status.is_success() {
            let error_text = response.text().await.unwrap_or_default();
            return Err(anyhow!("RAG API error {}: {}", status, error_text));
        }
        
        let response_text = response.text().await?;
        debug!("Received RAG response: {}", response_text);
        
        let result: R = serde_json::from_str(&response_text)
            .map_err(|e| anyhow!("Failed to parse RAG response: {}", e))?;
        
        Ok(result)
    }
    
    async fn make_get_request<R>(&self, endpoint: &str) -> Result<R>
    where
        R: for<'de> serde::Deserialize<'de>,
    {
        let url = format!("{}/{}", self.base_url, endpoint.trim_start_matches('/'));
        
        let mut request = self.client.get(&url);
        
        // Add API key if configured
        if let Some(ref api_key) = self.config.api_key {
            request = request.header("Authorization", format!("Bearer {}", api_key));
        }
        
        debug!("Sending RAG GET request to {}", url);
        
        let response = request.send().await?;
        let status = response.status();
        
        if !status.is_success() {
            let error_text = response.text().await.unwrap_or_default();
            return Err(anyhow!("RAG API error {}: {}", status, error_text));
        }
        
        let response_text = response.text().await?;
        debug!("Received RAG GET response: {}", response_text);
        
        let result: R = serde_json::from_str(&response_text)
            .map_err(|e| anyhow!("Failed to parse RAG GET response: {}", e))?;
        
        Ok(result)
    }
    
    pub async fn query_documents(&self, query: KnowledgeQuery) -> Result<KnowledgeResponse> {
        let start_time = Instant::now();
        
        let rag_request: RagQueryRequest = query.into();
        
        info!("Querying RAG for: {}", rag_request.query);
        
        let mut attempts = 0;
        let max_retries = self.config.max_retries;
        
        loop {
            match self.make_request::<RagQueryRequest, RagQueryResponse>(
                "/query", 
                rag_request.clone()
            ).await {
                Ok(rag_response) => {
                    let processing_time = start_time.elapsed().as_millis() as u64;
                    
                    info!("RAG query completed in {}ms, found {} documents", 
                        processing_time, rag_response.documents.len());
                    
                    let response: KnowledgeResponse = rag_response.into();
                    return Ok(response);
                }
                Err(e) => {
                    attempts += 1;
                    if attempts >= max_retries {
                        error!("RAG query failed after {} attempts: {}", attempts, e);
                        return Err(e);
                    }
                    
                    warn!("RAG query attempt {} failed: {}, retrying...", attempts, e);
                    let delay = Duration::from_millis(100 * attempts as u64);
                    sleep(delay).await;
                }
            }
        }
    }
    
    pub async fn search_patterns(&self, pattern_query: PatternSearchQuery) -> Result<PatternSearchResponse> {
        let start_time = Instant::now();
        
        info!("Searching for {} patterns in symbol {}", 
            format!("{:?}", pattern_query.pattern_type), pattern_query.symbol);
        
        let response = self.make_request::<PatternSearchQuery, PatternSearchResponse>(
            "/patterns/search", 
            pattern_query
        ).await?;
        
        let processing_time = start_time.elapsed().as_millis() as u64;
        info!("Pattern search completed in {}ms, found {} patterns", 
            processing_time, response.patterns.len());
        
        Ok(response)
    }
    
    pub async fn analyze_news(&self, news_request: NewsAnalysisRequest) -> Result<NewsAnalysisResponse> {
        let start_time = Instant::now();
        
        info!("Analyzing {} news items", news_request.news_items.len());
        
        let response = self.make_request::<NewsAnalysisRequest, NewsAnalysisResponse>(
            "/news/analyze", 
            news_request
        ).await?;
        
        let processing_time = start_time.elapsed().as_millis() as u64;
        info!("News analysis completed in {}ms, overall sentiment: {:.2}", 
            processing_time, response.overall_sentiment);
        
        Ok(response)
    }
    
    pub async fn get_market_regime(&self, regime_query: MarketRegimeQuery) -> Result<MarketRegimeResponse> {
        let start_time = Instant::now();
        
        info!("Analyzing market regime for {}", regime_query.symbol);
        
        let response = self.make_request::<MarketRegimeQuery, MarketRegimeResponse>(
            "/regime/analyze", 
            regime_query
        ).await?;
        
        let processing_time = start_time.elapsed().as_millis() as u64;
        info!("Market regime analysis completed in {}ms, current regime: {}", 
            processing_time, response.current_regime.name);
        
        Ok(response)
    }
    
    pub async fn ingest_document(&self, content: String, metadata: std::collections::HashMap<String, String>) -> Result<String> {
        debug!("Ingesting document with {} characters", content.len());
        
        #[derive(serde::Serialize)]
        struct DocumentRequest {
            content: String,
            metadata: Option<std::collections::HashMap<String, String>>,
        }
        
        #[derive(serde::Deserialize)]
        struct DocumentResponse {
            id: String,
            status: String,
        }
        
        let request = DocumentRequest {
            content,
            metadata: Some(metadata),
        };
        
        let response = self.make_request::<DocumentRequest, DocumentResponse>(
            "/documents", 
            request
        ).await?;
        
        // Check if document was accepted for indexing
        if response.status != "accepted" && response.status != "success" {
            return Err(anyhow!("Document indexing failed with status: {}", response.status));
        }
        
        info!("Document ingested with ID: {} (status: {})", response.id, response.status);
        Ok(response.id)
    }
    
    pub async fn ingest_market_event(&self, event: MarketEvent) -> Result<()> {
        debug!("Ingesting market event: {:?} for {}", event.event_type, event.symbol);
        
        #[derive(serde::Serialize)]
        struct EventIngestionRequest {
            events: Vec<MarketEvent>,
        }
        
        let request = EventIngestionRequest {
            events: vec![event],
        };
        
        self.make_request::<EventIngestionRequest, serde_json::Value>(
            "/events/ingest", 
            request
        ).await?;
        
        debug!("Market event ingested successfully");
        Ok(())
    }
    
    pub async fn batch_ingest_events(&self, events: Vec<MarketEvent>) -> Result<()> {
        info!("Batch ingesting {} market events", events.len());
        
        #[derive(serde::Serialize)]
        struct BatchEventRequest {
            events: Vec<MarketEvent>,
        }
        
        let request = BatchEventRequest { events };
        
        self.make_request::<BatchEventRequest, serde_json::Value>(
            "/events/batch", 
            request
        ).await?;
        
        info!("Batch ingestion completed");
        Ok(())
    }
    
    pub async fn health_check(&self) -> Result<HealthStatus> {
        let start_time = Instant::now();
        
        debug!("Performing RAG health check");
        
        match self.make_get_request::<RagHealthResponse>("/health").await {
            Ok(health_response) => {
                let response_time = start_time.elapsed();
                
                if health_response.status == "healthy" {
                    if response_time < Duration::from_millis(1000) {
                        info!("RAG health check passed in {:?}", response_time);
                        Ok(HealthStatus::Healthy)
                    } else {
                        warn!("RAG health check slow: {:?}", response_time);
                        Ok(HealthStatus::Degraded)
                    }
                } else {
                    warn!("RAG reports unhealthy status: {}", health_response.status);
                    Ok(HealthStatus::Degraded)
                }
            }
            Err(e) => {
                error!("RAG health check failed: {}", e);
                Ok(HealthStatus::Unhealthy)
            }
        }
    }
    
    pub async fn get_system_status(&self) -> Result<serde_json::Value> {
        debug!("Fetching RAG system status");
        self.make_get_request::<serde_json::Value>("/status").await
    }
    
    pub async fn search_similar_events(&self, reference_event: &MarketEvent, limit: usize) -> Result<Vec<MarketEvent>> {
        debug!("Searching for events similar to {:?}", reference_event.event_type);
        
        #[derive(serde::Serialize)]
        struct SimilaritySearchRequest {
            reference_event: MarketEvent,
            limit: usize,
            similarity_threshold: f32,
        }
        
        #[derive(serde::Deserialize)]
        struct SimilaritySearchResponse {
            events: Vec<MarketEvent>,
        }
        
        let request = SimilaritySearchRequest {
            reference_event: reference_event.clone(),
            limit,
            similarity_threshold: self.config.query_threshold,
        };
        
        let response = self.make_request::<SimilaritySearchRequest, SimilaritySearchResponse>(
            "/events/similar", 
            request
        ).await?;
        
        info!("Found {} similar events", response.events.len());
        Ok(response.events)
    }
    
    pub async fn get_knowledge_stats(&self) -> Result<KnowledgeStats> {
        debug!("Fetching knowledge base statistics");
        self.make_get_request::<KnowledgeStats>("/stats").await
    }
}

#[derive(Debug, Clone, serde::Deserialize)]
pub struct KnowledgeStats {
    pub total_documents: u64,
    pub total_events: u64,
    pub total_patterns: u64,
    pub storage_size_mb: f64,
    pub last_update: chrono::DateTime<chrono::Utc>,
    pub symbols_covered: Vec<String>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::RagConfig;
    
    fn create_test_config() -> RagConfig {
        RagConfig {
            server_url: "http://localhost:8001".to_string(),
            api_key: Some("test_key".to_string()),
            timeout_ms: 5000,
            max_retries: 3,
            query_threshold: 0.6,
            top_k: 10,
        }
    }
    
    #[tokio::test]
    async fn test_client_creation() {
        let config = Arc::new(create_test_config());
        let client = RagClient::new(config).await;
        assert!(client.is_ok());
    }
    
    #[test]
    fn test_url_formatting() {
        let base_url = "http://localhost:8001/";
        let endpoint = "/query";
        let url = format!("{}/{}", base_url.trim_end_matches('/'), endpoint.trim_start_matches('/'));
        assert_eq!(url, "http://localhost:8001/query");
    }
}