use anyhow::Result;
use std::sync::Arc;

pub mod config;
pub mod okx;
pub mod mcp;
pub mod rag;
pub mod coordinator;
pub mod types;

pub use config::IntegrationConfig;
pub use coordinator::IntegrationCoordinator;
pub use types::*;

#[derive(Debug, Clone)]
pub struct Integrations {
    pub config: Arc<IntegrationConfig>,
    pub coordinator: Arc<IntegrationCoordinator>,
}

impl Integrations {
    pub async fn new(config: IntegrationConfig) -> Result<Self> {
        let config = Arc::new(config);
        let coordinator = Arc::new(IntegrationCoordinator::new(config.clone()).await?);
        
        Ok(Self {
            config,
            coordinator,
        })
    }
    
    pub async fn start(&self) -> Result<()> {
        self.coordinator.start().await
    }
    
    pub async fn stop(&self) -> Result<()> {
        self.coordinator.stop().await
    }
    
    pub async fn health_check(&self) -> Result<IntegrationHealth> {
        self.coordinator.health_check().await
    }
}