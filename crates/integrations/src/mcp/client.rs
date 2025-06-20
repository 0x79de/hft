use anyhow::{Result, anyhow};
use reqwest::Client;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::time::sleep;
use tracing::{info, warn, error, debug};
use base64::{Engine as _, engine::general_purpose};

use crate::config::McpConfig;
use crate::types::{PredictionRequest, PredictionResponse, HealthStatus};
use super::types::{
    McpPredictionRequest, McpPredictionResponse, McpHealthResponse, 
    McpApiRequest, McpApiResponse, ModelInfo, McpErrorResponse
};

#[derive(Debug, Clone)]
pub struct McpClient {
    client: Client,
    base_url: String,
    config: Arc<McpConfig>,
}

impl McpClient {
    pub async fn new(config: Arc<McpConfig>) -> Result<Self> {
        let client = Client::builder()
            .timeout(Duration::from_millis(config.timeout_ms))
            .user_agent("HFT-Integrations/1.0")
            .build()?;
        
        let base_url = config.server_url.trim_end_matches('/').to_string();
        
        info!("Initializing MCP client for server: {}", base_url);
        
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
        
        let api_request = McpApiRequest {
            data: request_data,
            metadata: std::collections::HashMap::new(),
        };
        
        let mut request = self.client.post(&url)
            .header("Content-Type", "application/json");
        
        // Add API key if configured
        if let Some(ref api_key) = self.config.api_key {
            request = request.header("Authorization", format!("Bearer {}", api_key));
        }
        
        let body = serde_json::to_string(&api_request)?;
        debug!("Sending MCP request to {}: {}", url, body);
        
        let response = request.body(body).send().await?;
        let status = response.status();
        
        if !status.is_success() {
            let error_text = response.text().await.unwrap_or_default();
            
            // Try to parse as MCP error response
            if let Ok(error_response) = serde_json::from_str::<McpErrorResponse>(&error_text) {
                return Err(anyhow!("MCP API error {}: {} - {}", 
                    status, error_response.code, error_response.error));
            }
            
            return Err(anyhow!("MCP API error {}: {}", status, error_text));
        }
        
        let response_text = response.text().await?;
        debug!("Received MCP response: {}", response_text);
        
        let api_response: McpApiResponse<R> = serde_json::from_str(&response_text)
            .map_err(|e| anyhow!("Failed to parse MCP response: {}", e))?;
        
        if !api_response.success {
            return Err(anyhow!("MCP request failed: {}", 
                api_response.error.unwrap_or_else(|| "Unknown error".to_string())));
        }
        
        api_response.data
            .ok_or_else(|| anyhow!("MCP response missing data"))
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
        
        debug!("Sending MCP GET request to {}", url);
        
        let response = request.send().await?;
        let status = response.status();
        
        if !status.is_success() {
            let error_text = response.text().await.unwrap_or_default();
            return Err(anyhow!("MCP API error {}: {}", status, error_text));
        }
        
        let response_text = response.text().await?;
        debug!("Received MCP GET response: {}", response_text);
        
        let result: R = serde_json::from_str(&response_text)
            .map_err(|e| anyhow!("Failed to parse MCP GET response: {}", e))?;
        
        Ok(result)
    }
    
    pub async fn get_prediction(&self, request: PredictionRequest) -> Result<PredictionResponse> {
        let start_time = Instant::now();
        
        let mcp_request: McpPredictionRequest = request.into();
        
        info!("Requesting prediction for symbol: {}", mcp_request.symbol);
        
        let mut attempts = 0;
        let max_retries = self.config.max_retries;
        
        loop {
            match self.make_request::<McpPredictionRequest, McpPredictionResponse>(
                "/api/predict", 
                mcp_request.clone()
            ).await {
                Ok(mcp_response) => {
                    let processing_time = start_time.elapsed().as_millis() as u64;
                    
                    info!("Received prediction for {} in {}ms with confidence {:.2}", 
                        mcp_response.symbol, processing_time, mcp_response.confidence);
                    
                    // Check if prediction meets threshold
                    if mcp_response.confidence < self.config.prediction_threshold {
                        warn!("Prediction confidence {:.2} below threshold {:.2}", 
                            mcp_response.confidence, self.config.prediction_threshold);
                    }
                    
                    let response: PredictionResponse = mcp_response.into();
                    return Ok(response);
                }
                Err(e) => {
                    attempts += 1;
                    if attempts >= max_retries {
                        error!("MCP prediction failed after {} attempts: {}", attempts, e);
                        return Err(e);
                    }
                    
                    warn!("MCP prediction attempt {} failed: {}, retrying...", attempts, e);
                    let delay = Duration::from_millis(100 * attempts as u64);
                    sleep(delay).await;
                }
            }
        }
    }
    
    pub async fn get_model_info(&self) -> Result<ModelInfo> {
        debug!("Fetching MCP model information");
        
        let model_info = self.make_get_request::<ModelInfo>("/api/model/info").await?;
        
        info!("Retrieved model info: {} v{} (accuracy: {:.2}%)", 
            model_info.model_name, model_info.version, model_info.accuracy * 100.0);
        
        Ok(model_info)
    }
    
    pub async fn health_check(&self) -> Result<HealthStatus> {
        let start_time = Instant::now();
        
        debug!("Performing MCP health check");
        
        match self.make_get_request::<McpHealthResponse>("/health").await {
            Ok(health_response) => {
                let response_time = start_time.elapsed();
                
                if health_response.status == "healthy" {
                    if response_time < Duration::from_millis(500) {
                        info!("MCP health check passed in {:?}", response_time);
                        Ok(HealthStatus::Healthy)
                    } else {
                        warn!("MCP health check slow: {:?}", response_time);
                        Ok(HealthStatus::Degraded)
                    }
                } else {
                    warn!("MCP reports unhealthy status: {}", health_response.status);
                    Ok(HealthStatus::Degraded)
                }
            }
            Err(e) => {
                error!("MCP health check failed: {}", e);
                Ok(HealthStatus::Unhealthy)
            }
        }
    }
    
    pub async fn get_supported_symbols(&self) -> Result<Vec<String>> {
        debug!("Fetching supported symbols from MCP");
        
        #[derive(serde::Deserialize)]
        struct SymbolsResponse {
            symbols: Vec<String>,
        }
        
        let response = self.make_get_request::<SymbolsResponse>("/api/symbols").await?;
        
        info!("MCP supports {} symbols", response.symbols.len());
        Ok(response.symbols)
    }
    
    pub async fn update_model(&self, model_data: Vec<u8>) -> Result<()> {
        debug!("Updating MCP model");
        
        #[derive(serde::Serialize)]
        struct ModelUpdateRequest {
            model_data: String, // Base64 encoded
        }
        
        let request = ModelUpdateRequest {
            model_data: general_purpose::STANDARD.encode(model_data),
        };
        
        self.make_request::<ModelUpdateRequest, serde_json::Value>(
            "/api/model/update", 
            request
        ).await?;
        
        info!("MCP model updated successfully");
        Ok(())
    }
    
    pub async fn get_prediction_history(&self, symbol: &str, limit: Option<u32>) -> Result<Vec<McpPredictionResponse>> {
        debug!("Fetching prediction history for {}", symbol);
        
        let endpoint = if let Some(limit) = limit {
            format!("/api/predictions/history?symbol={}&limit={}", symbol, limit)
        } else {
            format!("/api/predictions/history?symbol={}", symbol)
        };
        
        #[derive(serde::Deserialize)]
        struct HistoryResponse {
            predictions: Vec<McpPredictionResponse>,
        }
        
        let response = self.make_get_request::<HistoryResponse>(&endpoint).await?;
        
        info!("Retrieved {} historical predictions for {}", 
            response.predictions.len(), symbol);
        
        Ok(response.predictions)
    }
    
    pub async fn submit_feedback(&self, prediction_id: &str, actual_outcome: f64, profit_loss: f64) -> Result<()> {
        debug!("Submitting feedback for prediction {}", prediction_id);
        
        #[derive(serde::Serialize)]
        struct FeedbackRequest {
            prediction_id: String,
            actual_outcome: f64,
            profit_loss: f64,
            timestamp: chrono::DateTime<chrono::Utc>,
        }
        
        let request = FeedbackRequest {
            prediction_id: prediction_id.to_string(),
            actual_outcome,
            profit_loss,
            timestamp: chrono::Utc::now(),
        };
        
        self.make_request::<FeedbackRequest, serde_json::Value>(
            "/api/feedback", 
            request
        ).await?;
        
        info!("Feedback submitted for prediction {}", prediction_id);
        Ok(())
    }
    
    pub async fn get_performance_metrics(&self, symbol: Option<&str>) -> Result<super::types::PerformanceMetrics> {
        debug!("Fetching performance metrics");
        
        let endpoint = if let Some(symbol) = symbol {
            format!("/api/metrics?symbol={}", symbol)
        } else {
            "/api/metrics".to_string()
        };
        
        let metrics = self.make_get_request::<super::types::PerformanceMetrics>(&endpoint).await?;
        
        info!("Retrieved performance metrics: accuracy {:.2}%, win rate {:.2}%", 
            metrics.accuracy * 100.0, metrics.win_rate * 100.0);
        
        Ok(metrics)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::McpConfig;
    
    fn create_test_config() -> McpConfig {
        McpConfig {
            server_url: "http://localhost:8000".to_string(),
            api_key: Some("test_key".to_string()),
            timeout_ms: 5000,
            max_retries: 3,
            prediction_threshold: 0.7,
        }
    }
    
    #[tokio::test]
    async fn test_client_creation() {
        let config = Arc::new(create_test_config());
        let client = McpClient::new(config).await;
        assert!(client.is_ok());
    }
    
    #[test]
    fn test_url_formatting() {
        let base_url = "http://localhost:8000/";
        let endpoint = "/api/predict";
        let url = format!("{}/{}", base_url.trim_end_matches('/'), endpoint.trim_start_matches('/'));
        assert_eq!(url, "http://localhost:8000/api/predict");
    }
}