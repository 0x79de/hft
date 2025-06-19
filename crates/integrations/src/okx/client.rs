use anyhow::{Result, anyhow};
use reqwest::Client;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::time::sleep;
use tracing::{info, warn, error};
use rust_decimal::prelude::ToPrimitive;

use crate::config::OkxConfig;
use crate::types::{MarketContext, TradingSignal, SignalType, HealthStatus};
use super::auth::OkxAuth;
use super::types::*;

#[derive(Debug, Clone)]
pub struct OkxClient {
    client: Client,
    auth: OkxAuth,
    base_url: String,
    config: Arc<OkxConfig>,
}

impl OkxClient {
    pub async fn new(config: Arc<OkxConfig>) -> Result<Self> {
        let auth = OkxAuth::new(
            config.api_key.clone(),
            config.secret_key.clone(),
            config.passphrase.clone(),
        );
        
        let client = Client::builder()
            .timeout(Duration::from_millis(config.timeout_ms))
            .user_agent("HFT-Rust/1.0")
            .build()?;
        
        let base_url = if config.sandbox {
            "https://www.okx.com".to_string()
        } else {
            config.base_url.clone().unwrap_or_else(|| "https://www.okx.com".to_string())
        };
        
        Ok(Self {
            client,
            auth,
            base_url,
            config,
        })
    }
    
    async fn make_request<T>(&self, method: &str, path: &str, body: &str) -> Result<OkxApiResponse<T>>
    where
        T: for<'de> serde::Deserialize<'de>,
    {
        let url = format!("{}{}", self.base_url, path);
        let headers = self.auth.get_headers(method, path, body)?;
        
        let mut request = match method {
            "GET" => self.client.get(&url),
            "POST" => self.client.post(&url),
            "PUT" => self.client.put(&url),
            "DELETE" => self.client.delete(&url),
            _ => return Err(anyhow!("Unsupported HTTP method: {}", method)),
        };
        
        for (key, value) in headers {
            request = request.header(key, value);
        }
        
        if !body.is_empty() {
            request = request.body(body.to_string());
        }
        
        // Apply rate limiting before sending request
        self.rate_limit().await?;
        
        let response = request.send().await?;
        let status = response.status();
        
        if !status.is_success() {
            let error_text = response.text().await.unwrap_or_default();
            return Err(anyhow!("OKX API error {}: {}", status, error_text));
        }
        
        let response_text = response.text().await?;
        let api_response: OkxApiResponse<T> = serde_json::from_str(&response_text)
            .map_err(|e| anyhow!("Failed to parse OKX response: {}", e))?;
        
        if api_response.code != "0" {
            return Err(anyhow!("OKX API error: {} - {}", api_response.code, api_response.msg));
        }
        
        Ok(api_response)
    }
    
    pub async fn get_ticker(&self, symbol: &str) -> Result<OkxTicker> {
        let path = format!("/api/v5/market/ticker?instId={}", symbol);
        let response: OkxApiResponse<OkxTicker> = self.make_request("GET", &path, "").await?;
        
        response.data.into_iter().next()
            .ok_or_else(|| anyhow!("No ticker data returned for symbol: {}", symbol))
    }
    
    pub async fn get_order_book(&self, symbol: &str, depth: Option<u32>) -> Result<OkxOrderBook> {
        let sz = depth.unwrap_or(20);
        let path = format!("/api/v5/market/books?instId={}&sz={}", symbol, sz);
        let response: OkxApiResponse<OkxOrderBook> = self.make_request("GET", &path, "").await?;
        
        response.data.into_iter().next()
            .ok_or_else(|| anyhow!("No order book data returned for symbol: {}", symbol))
    }
    
    pub async fn get_account_balance(&self) -> Result<OkxAccountBalance> {
        let path = "/api/v5/account/balance";
        let response: OkxApiResponse<OkxAccountBalance> = self.make_request("GET", path, "").await?;
        
        response.data.into_iter().next()
            .ok_or_else(|| anyhow!("No account balance data returned"))
    }
    
    pub async fn get_positions(&self, symbol: Option<&str>) -> Result<Vec<OkxPosition>> {
        let path = if let Some(symbol) = symbol {
            format!("/api/v5/account/positions?instId={}", symbol)
        } else {
            "/api/v5/account/positions".to_string()
        };
        
        let response: OkxApiResponse<OkxPosition> = self.make_request("GET", &path, "").await?;
        Ok(response.data)
    }
    
    pub async fn place_order(&self, signal: &TradingSignal) -> Result<OkxOrderResponse> {
        let order_request = OkxOrderRequest {
            inst_id: signal.symbol.clone(),
            td_mode: "cash".to_string(),
            side: match signal.signal_type {
                SignalType::Buy | SignalType::StrongBuy => "buy".to_string(),
                SignalType::Sell | SignalType::StrongSell => "sell".to_string(),
                SignalType::Hold => return Err(anyhow!("Cannot place order for HOLD signal")),
            },
            ord_type: if signal.price_target.is_some() {
                "limit".to_string()
            } else {
                "market".to_string()
            },
            sz: "0.01".to_string(), // Minimum size for testing
            px: signal.price_target.map(|p| p.to_string()),
            ccy: None,
            cl_ord_id: Some(signal.id.to_string()),
            tag: Some("HFT-Rust".to_string()),
        };
        
        let body = serde_json::to_string(&order_request)?;
        let path = "/api/v5/trade/order";
        let response: OkxApiResponse<OkxOrderResponse> = self.make_request("POST", path, &body).await?;
        
        response.data.into_iter().next()
            .ok_or_else(|| anyhow!("No order response data returned"))
    }
    
    pub async fn cancel_order(&self, order_id: &str, symbol: &str) -> Result<()> {
        let cancel_request = serde_json::json!({
            "instId": symbol,
            "ordId": order_id
        });
        
        let body = serde_json::to_string(&cancel_request)?;
        let path = "/api/v5/trade/cancel-order";
        let _response: OkxApiResponse<serde_json::Value> = self.make_request("POST", path, &body).await?;
        
        Ok(())
    }
    
    pub async fn get_market_context(&self, symbol: &str) -> Result<MarketContext> {
        let ticker = self.get_ticker(symbol).await?;
        let order_book = self.get_order_book(symbol, Some(10)).await?;
        
        let current_price = ticker.last_price();
        let bid = ticker.bid_price();
        let ask = ticker.ask_price();
        let volume_24h = ticker.volume_24h();
        let change_24h = ticker.change_24h();
        
        let order_book_depth = if let (Some((bid_price, bid_size)), Some((ask_price, ask_size))) = 
            (order_book.best_bid(), order_book.best_ask()) 
        {
            Some(crate::types::OrderBookDepth {
                bid_depth: bid_size,
                ask_depth: ask_size,
                spread: ask_price - bid_price,
                imbalance: ((bid_size - ask_size) / (bid_size + ask_size)).to_f64().unwrap_or(0.0),
            })
        } else {
            None
        };
        
        Ok(MarketContext {
            symbol: symbol.to_string(),
            current_price,
            bid,
            ask,
            volume_24h,
            change_24h,
            volatility: Some(change_24h.abs().to_f64().unwrap_or(0.0) / 100.0), // Simple volatility approximation based on 24h change
            order_book_depth,
            timestamp: chrono::Utc::now(),
        })
    }
    
    pub async fn health_check(&self) -> Result<HealthStatus> {
        let start_time = Instant::now();
        
        match self.get_ticker("BTC-USDT").await {
            Ok(_) => {
                let response_time = start_time.elapsed();
                if response_time < Duration::from_millis(1000) {
                    info!("OKX health check passed in {:?}", response_time);
                    Ok(HealthStatus::Healthy)
                } else {
                    warn!("OKX health check slow: {:?}", response_time);
                    Ok(HealthStatus::Degraded)
                }
            }
            Err(e) => {
                error!("OKX health check failed: {}", e);
                Ok(HealthStatus::Unhealthy)
            }
        }
    }
    
    pub async fn get_instruments(&self, inst_type: &str) -> Result<Vec<serde_json::Value>> {
        let path = format!("/api/v5/public/instruments?instType={}", inst_type);
        let response: OkxApiResponse<serde_json::Value> = self.make_request("GET", &path, "").await?;
        Ok(response.data)
    }
    
    pub async fn get_funding_rate(&self, symbol: &str) -> Result<serde_json::Value> {
        let path = format!("/api/v5/public/funding-rate?instId={}", symbol);
        let response: OkxApiResponse<serde_json::Value> = self.make_request("GET", &path, "").await?;
        
        response.data.into_iter().next()
            .ok_or_else(|| anyhow!("No funding rate data returned for symbol: {}", symbol))
    }
    
    async fn rate_limit(&self) -> Result<()> {
        let delay_ms = 1000 / self.config.rate_limit_requests_per_second as u64;
        sleep(Duration::from_millis(delay_ms)).await;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::OkxConfig;
    
    fn create_test_config() -> OkxConfig {
        OkxConfig {
            api_key: "test_key".to_string(),
            secret_key: "dGVzdF9zZWNyZXQ=".to_string(),
            passphrase: "test_passphrase".to_string(),
            sandbox: true,
            base_url: None,
            timeout_ms: 5000,
            rate_limit_requests_per_second: 10,
        }
    }
    
    #[tokio::test]
    async fn test_client_creation() {
        let config = Arc::new(create_test_config());
        let client = OkxClient::new(config).await;
        assert!(client.is_ok());
    }
}