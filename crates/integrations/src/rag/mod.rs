pub mod client;
pub mod types;
pub mod ingestion;

pub use client::RagClient;
pub use types::*;
pub use ingestion::MarketEventIngestion;

use anyhow::Result;
use crate::config::RagConfig;
use crate::types::{KnowledgeQuery, KnowledgeResponse, HealthStatus};
use std::sync::Arc;

#[derive(Debug, Clone)]
pub struct RagIntegration {
    pub client: Arc<RagClient>,
    pub ingestion: Arc<MarketEventIngestion>,
    config: Arc<RagConfig>,
}

impl RagIntegration {
    pub async fn new(config: RagConfig) -> Result<Self> {
        let config = Arc::new(config);
        let client = Arc::new(RagClient::new(config.clone()).await?);
        let ingestion = Arc::new(MarketEventIngestion::new(client.clone()));
        
        Ok(Self {
            client,
            ingestion,
            config,
        })
    }
    
    pub async fn query_knowledge(&self, query: KnowledgeQuery) -> Result<KnowledgeResponse> {
        self.client.query_documents(query).await
    }
    
    pub async fn ingest_market_event(&self, event: MarketEvent) -> Result<()> {
        self.ingestion.ingest_event(event).await
    }
    
    pub async fn health_check(&self) -> Result<HealthStatus> {
        self.client.health_check().await
    }
    
    pub async fn search_patterns(&self, pattern_query: PatternSearchQuery) -> Result<PatternSearchResponse> {
        self.client.search_patterns(pattern_query).await
    }
    
    pub fn get_config(&self) -> &RagConfig {
        &self.config
    }
}