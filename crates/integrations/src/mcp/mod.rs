pub mod client;
pub mod types;
pub mod features;

pub use client::McpClient;
pub use types::*;
pub use features::FeatureExtractor;

use anyhow::Result;
use crate::config::McpConfig;
use crate::types::{PredictionRequest, PredictionResponse, HealthStatus};
use std::sync::Arc;

#[derive(Debug, Clone)]
pub struct McpIntegration {
    pub client: Arc<McpClient>,
    pub feature_extractor: Arc<FeatureExtractor>,
    config: Arc<McpConfig>,
}

impl McpIntegration {
    pub async fn new(config: McpConfig) -> Result<Self> {
        let config = Arc::new(config);
        let client = Arc::new(McpClient::new(config.clone()).await?);
        let feature_extractor = Arc::new(FeatureExtractor::new());
        
        Ok(Self {
            client,
            feature_extractor,
            config,
        })
    }
    
    pub async fn get_prediction(&self, request: PredictionRequest) -> Result<PredictionResponse> {
        self.client.get_prediction(request).await
    }
    
    pub async fn health_check(&self) -> Result<HealthStatus> {
        self.client.health_check().await
    }
    
    pub async fn get_model_info(&self) -> Result<ModelInfo> {
        self.client.get_model_info().await
    }
    
    pub fn get_config(&self) -> &McpConfig {
        &self.config
    }
}