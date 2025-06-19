pub mod auth;
pub mod client;
pub mod websocket;
pub mod types;

pub use auth::OkxAuth;
pub use client::OkxClient;
pub use websocket::OkxWebSocket;
pub use types::*;

use anyhow::Result;
use crate::config::OkxConfig;
use crate::types::{MarketContext, TradingSignal, HealthStatus};
use std::sync::Arc;

#[derive(Debug, Clone)]
pub struct OkxIntegration {
    pub client: Arc<OkxClient>,
    pub websocket: Arc<OkxWebSocket>,
    config: Arc<OkxConfig>,
}

impl OkxIntegration {
    pub async fn new(config: OkxConfig) -> Result<Self> {
        let config = Arc::new(config);
        let client = Arc::new(OkxClient::new(config.clone()).await?);
        let websocket = Arc::new(OkxWebSocket::new(config.clone()).await?);
        
        Ok(Self {
            client,
            websocket,
            config,
        })
    }
    
    pub async fn start(&self) -> Result<()> {
        self.websocket.connect().await?;
        Ok(())
    }
    
    pub async fn stop(&self) -> Result<()> {
        self.websocket.disconnect().await?;
        Ok(())
    }
    
    pub fn get_config(&self) -> &OkxConfig {
        &self.config
    }
    
    pub async fn get_market_context(&self, symbol: &str) -> Result<MarketContext> {
        self.client.get_market_context(symbol).await
    }
    
    pub async fn place_order(&self, signal: &TradingSignal) -> Result<OkxOrderResponse> {
        self.client.place_order(signal).await
    }
    
    pub async fn health_check(&self) -> Result<HealthStatus> {
        self.client.health_check().await
    }
}