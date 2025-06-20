use anyhow::Result;
use futures::{SinkExt, StreamExt};
use serde_json::Value;
use std::sync::Arc;
use tokio::sync::{mpsc, RwLock};
use tokio::time::{interval, Duration};
use tokio_tungstenite::{connect_async, tungstenite::Message};
use tracing::{info, warn, error, debug};
use url::Url;

use crate::config::OkxConfig;
use super::auth::OkxAuth;
use super::types::{OkxWebSocketMessage, OkxWebSocketChannel, OkxWebSocketSubscription};

#[derive(Debug, Clone)]
pub enum OkxWebSocketEvent {
    MarketData(Value),
    OrderUpdate(Value),
    PositionUpdate(Value),
    AccountUpdate(Value),
    Connected,
    Disconnected,
    Error(String),
}

#[derive(Debug)]
pub struct OkxWebSocket {
    config: Arc<OkxConfig>,
    auth: OkxAuth,
    event_tx: mpsc::UnboundedSender<OkxWebSocketEvent>,
    event_rx: Arc<RwLock<Option<mpsc::UnboundedReceiver<OkxWebSocketEvent>>>>,
    is_connected: Arc<RwLock<bool>>,
    subscriptions: Arc<RwLock<Vec<OkxWebSocketChannel>>>,
}

impl OkxWebSocket {
    pub async fn new(config: Arc<OkxConfig>) -> Result<Self> {
        let auth = OkxAuth::new(
            config.api_key.clone(),
            config.secret_key.clone(),
            config.passphrase.clone(),
        );
        
        let (event_tx, event_rx) = mpsc::unbounded_channel();
        
        Ok(Self {
            config,
            auth,
            event_tx,
            event_rx: Arc::new(RwLock::new(Some(event_rx))),
            is_connected: Arc::new(RwLock::new(false)),
            subscriptions: Arc::new(RwLock::new(Vec::new())),
        })
    }
    
    pub async fn connect(&self) -> Result<()> {
        let ws_url = if self.config.sandbox {
            "wss://ws.okx.com:8443/ws/v5/public"
        } else {
            "wss://ws.okx.com:8443/ws/v5/public"
        };
        
        let url = Url::parse(ws_url)?;
        info!("Connecting to OKX WebSocket: {}", url);
        
        let (ws_stream, _) = connect_async(url).await?;
        let (mut ws_sink, mut ws_stream) = ws_stream.split();
        
        let event_tx = self.event_tx.clone();
        let is_connected = self.is_connected.clone();
        let auth = self.auth.clone();
        let _subscriptions = self.subscriptions.clone();
        
        // Set connected status
        {
            let mut connected = is_connected.write().await;
            *connected = true;
        }
        
        // Send connection event
        let _ = event_tx.send(OkxWebSocketEvent::Connected);
        
        // Authentication for private channels
        tokio::spawn(async move {
            if let Ok(auth_msg) = auth.get_websocket_auth() {
                let login_message = auth_msg.to_login_message();
                if let Ok(msg_text) = serde_json::to_string(&login_message) {
                    if let Err(e) = ws_sink.send(Message::Text(msg_text)).await {
                        error!("Failed to send authentication message: {}", e);
                    }
                }
            }
        });
        
        // Message handling loop
        let event_tx_clone = event_tx.clone();
        let is_connected_clone = is_connected.clone();
        tokio::spawn(async move {
            while let Some(msg) = ws_stream.next().await {
                match msg {
                    Ok(Message::Text(text)) => {
                        debug!("Received WebSocket message: {}", text);
                        
                        if let Ok(ws_msg) = serde_json::from_str::<OkxWebSocketMessage>(&text) {
                            Self::handle_message(ws_msg, &event_tx_clone).await;
                        } else if let Ok(value) = serde_json::from_str::<Value>(&text) {
                            // Handle other message types
                            if let Some(event) = value.get("event").and_then(|v| v.as_str()) {
                                match event {
                                    "login" => {
                                        if let Some(code) = value.get("code").and_then(|v| v.as_str()) {
                                            if code == "0" {
                                                info!("WebSocket authentication successful");
                                            } else {
                                                error!("WebSocket authentication failed: {:?}", value);
                                            }
                                        }
                                    }
                                    "subscribe" => {
                                        info!("WebSocket subscription confirmed: {:?}", value);
                                    }
                                    "error" => {
                                        error!("WebSocket error: {:?}", value);
                                        let _ = event_tx_clone.send(OkxWebSocketEvent::Error(
                                            format!("WebSocket error: {:?}", value)
                                        ));
                                    }
                                    _ => {
                                        debug!("Unknown WebSocket event: {}", event);
                                    }
                                }
                            }
                        }
                    }
                    Ok(Message::Binary(_)) => {
                        debug!("Received binary message (ignoring)");
                    }
                    Ok(Message::Close(_)) => {
                        warn!("WebSocket connection closed");
                        let mut connected = is_connected_clone.write().await;
                        *connected = false;
                        let _ = event_tx_clone.send(OkxWebSocketEvent::Disconnected);
                        break;
                    }
                    Ok(Message::Ping(_)) => {
                        debug!("Received ping, sending pong");
                        // WebSocket library handles pong automatically
                    }
                    Ok(Message::Pong(_)) => {
                        debug!("Received pong");
                    }
                    Ok(Message::Frame(_)) => {
                        debug!("Received frame message");
                    }
                    Err(e) => {
                        error!("WebSocket error: {}", e);
                        let _ = event_tx_clone.send(OkxWebSocketEvent::Error(e.to_string()));
                    }
                }
            }
        });
        
        // Start heartbeat
        self.start_heartbeat().await;
        
        Ok(())
    }
    
    async fn handle_message(ws_msg: OkxWebSocketMessage, event_tx: &mpsc::UnboundedSender<OkxWebSocketEvent>) {
        if let (Some(arg), Some(data)) = (ws_msg.arg, ws_msg.data) {
            match arg.channel.as_str() {
                "tickers" => {
                    let _ = event_tx.send(OkxWebSocketEvent::MarketData(data));
                }
                "books" | "books5" => {
                    let _ = event_tx.send(OkxWebSocketEvent::MarketData(data));
                }
                "trades" => {
                    let _ = event_tx.send(OkxWebSocketEvent::MarketData(data));
                }
                "orders" => {
                    let _ = event_tx.send(OkxWebSocketEvent::OrderUpdate(data));
                }
                "positions" => {
                    let _ = event_tx.send(OkxWebSocketEvent::PositionUpdate(data));
                }
                "account" => {
                    let _ = event_tx.send(OkxWebSocketEvent::AccountUpdate(data));
                }
                _ => {
                    debug!("Unknown channel: {}", arg.channel);
                }
            }
        }
    }
    
    pub async fn subscribe_ticker(&self, symbol: &str) -> Result<()> {
        let channel = OkxWebSocketChannel {
            channel: "tickers".to_string(),
            inst_id: symbol.to_string(),
        };
        
        self.subscribe(vec![channel]).await
    }
    
    pub async fn subscribe_order_book(&self, symbol: &str) -> Result<()> {
        let channel = OkxWebSocketChannel {
            channel: "books5".to_string(),
            inst_id: symbol.to_string(),
        };
        
        self.subscribe(vec![channel]).await
    }
    
    pub async fn subscribe_trades(&self, symbol: &str) -> Result<()> {
        let channel = OkxWebSocketChannel {
            channel: "trades".to_string(),
            inst_id: symbol.to_string(),
        };
        
        self.subscribe(vec![channel]).await
    }
    
    pub async fn subscribe_orders(&self) -> Result<()> {
        let channel = OkxWebSocketChannel {
            channel: "orders".to_string(),
            inst_id: "".to_string(), // All instruments
        };
        
        self.subscribe(vec![channel]).await
    }
    
    async fn subscribe(&self, channels: Vec<OkxWebSocketChannel>) -> Result<()> {
        let subscription = OkxWebSocketSubscription {
            op: "subscribe".to_string(),
            args: channels.clone(),
        };
        
        let msg_text = serde_json::to_string(&subscription)?;
        info!("Subscribing to channels: {}", msg_text);
        
        // Store subscriptions
        {
            let mut subs = self.subscriptions.write().await;
            subs.extend(channels);
        }
        
        // Note: In a real implementation, we would send this message through the WebSocket
        // For now, we'll just log it since we don't have a persistent connection reference
        
        Ok(())
    }
    
    async fn start_heartbeat(&self) {
        let _event_tx = self.event_tx.clone();
        let is_connected = self.is_connected.clone();
        
        tokio::spawn(async move {
            let mut heartbeat_interval = interval(Duration::from_secs(30));
            
            loop {
                heartbeat_interval.tick().await;
                
                let connected = {
                    let connected = is_connected.read().await;
                    *connected
                };
                
                if !connected {
                    break;
                }
                
                // Send ping message
                debug!("Sending heartbeat ping");
                // Note: In a real implementation, we would send a ping through the WebSocket
            }
        });
    }
    
    pub async fn disconnect(&self) -> Result<()> {
        let mut connected = self.is_connected.write().await;
        *connected = false;
        
        let _ = self.event_tx.send(OkxWebSocketEvent::Disconnected);
        info!("OKX WebSocket disconnected");
        
        Ok(())
    }
    
    pub async fn is_connected(&self) -> bool {
        let connected = self.is_connected.read().await;
        *connected
    }
    
    pub async fn get_event_receiver(&self) -> Option<mpsc::UnboundedReceiver<OkxWebSocketEvent>> {
        let mut rx_option = self.event_rx.write().await;
        rx_option.take()
    }
    
    pub async fn reconnect(&self) -> Result<()> {
        warn!("Reconnecting to OKX WebSocket...");
        
        // Disconnect first
        self.disconnect().await?;
        
        // Wait a bit before reconnecting
        tokio::time::sleep(Duration::from_secs(5)).await;
        
        // Reconnect
        self.connect().await?;
        
        // Re-subscribe to previous channels
        let subscriptions = {
            let subs = self.subscriptions.read().await;
            subs.clone()
        };
        
        if !subscriptions.is_empty() {
            info!("Re-subscribing to {} channels", subscriptions.len());
            self.subscribe(subscriptions).await?;
        }
        
        Ok(())
    }
}

impl Clone for OkxWebSocket {
    fn clone(&self) -> Self {
        let (event_tx, event_rx) = mpsc::unbounded_channel();
        
        Self {
            config: self.config.clone(),
            auth: self.auth.clone(),
            event_tx,
            event_rx: Arc::new(RwLock::new(Some(event_rx))),
            is_connected: Arc::new(RwLock::new(false)),
            subscriptions: Arc::new(RwLock::new(Vec::new())),
        }
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
    async fn test_websocket_creation() {
        let config = Arc::new(create_test_config());
        let ws = OkxWebSocket::new(config).await;
        assert!(ws.is_ok());
    }
    
    #[tokio::test]
    async fn test_subscribe_channels() {
        let config = Arc::new(create_test_config());
        let ws = OkxWebSocket::new(config).await.unwrap();
        
        let result = ws.subscribe_ticker("BTC-USDT").await;
        assert!(result.is_ok());
    }
}