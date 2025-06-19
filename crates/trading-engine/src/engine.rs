use order_book::{OrderBook, MatchResult, Order, OrderId, Trade, Quantity, Side};
use event_processor::{EventProcessor, Event, OrderEvent, TradeEvent};
use risk_manager::RiskManager;
use std::collections::HashMap;
use std::sync::Arc;
use parking_lot::RwLock;
use anyhow::Result;
use tracing::info;
use chrono::Utc;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EngineConfig {
    pub max_symbols: usize,
    pub enable_risk_checks: bool,
    pub enable_event_emission: bool,
    pub max_orders_per_symbol: usize,
}

impl Default for EngineConfig {
    fn default() -> Self {
        Self {
            max_symbols: 1000,
            enable_risk_checks: true,
            enable_event_emission: true,
            max_orders_per_symbol: 1_000_000,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum OrderResponse {
    Accepted {
        order_id: OrderId,
        symbol: String,
        timestamp: chrono::DateTime<Utc>,
    },
    Rejected {
        order_id: OrderId,
        reason: String,
        timestamp: chrono::DateTime<Utc>,
    },
    PartiallyFilled {
        order_id: OrderId,
        trades: Vec<Trade>,
        remaining_quantity: Quantity,
        timestamp: chrono::DateTime<Utc>,
    },
    FullyFilled {
        order_id: OrderId,
        trades: Vec<Trade>,
        timestamp: chrono::DateTime<Utc>,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum CancelResponse {
    Cancelled {
        order_id: OrderId,
        timestamp: chrono::DateTime<Utc>,
    },
    NotFound {
        order_id: OrderId,
        timestamp: chrono::DateTime<Utc>,
    },
}

pub struct TradingEngine {
    config: EngineConfig,
    order_books: Arc<RwLock<HashMap<String, Arc<OrderBook>>>>,
    risk_manager: Arc<RiskManager>,
    event_processor: Arc<EventProcessor>,
    running: Arc<RwLock<bool>>,
}

impl TradingEngine {
    #[inline]
    pub fn new() -> Self {
        Self::with_config(EngineConfig::default())
    }
    
    #[inline]
    pub fn with_config(config: EngineConfig) -> Self {
        let event_processor = Arc::new(EventProcessor::new());
        let risk_manager = Arc::new(RiskManager::new());
        
        Self {
            config,
            order_books: Arc::new(RwLock::new(HashMap::new())),
            risk_manager,
            event_processor,
            running: Arc::new(RwLock::new(false)),
        }
    }
    
    pub async fn start(&self) -> Result<()> {
        if *self.running.read() {
            return Ok(());
        }
        
        *self.running.write() = true;
        
        self.event_processor.start().await?;
        
        info!("Trading engine started");
        Ok(())
    }
    
    pub async fn stop(&self) -> Result<()> {
        *self.running.write() = false;
        
        self.event_processor.stop().await?;
        
        info!("Trading engine stopped");
        Ok(())
    }
    
    #[inline]
    pub fn is_running(&self) -> bool {
        *self.running.read()
    }
    
    #[inline]
    pub fn add_symbol(&self, symbol: String) -> Result<()> {
        let mut books = self.order_books.write();
        
        if books.len() >= self.config.max_symbols {
            return Err(anyhow::anyhow!("Maximum symbols limit reached"));
        }
        
        if !books.contains_key(&symbol) {
            let order_book = Arc::new(OrderBook::new(symbol.clone()));
            books.insert(symbol.clone(), order_book);
            info!("Added new symbol: {}", symbol);
        }
        
        Ok(())
    }
    
    #[inline]
    pub fn remove_symbol(&self, symbol: &str) -> Result<()> {
        let mut books = self.order_books.write();
        
        if books.remove(symbol).is_some() {
            info!("Removed symbol: {}", symbol);
            Ok(())
        } else {
            Err(anyhow::anyhow!("Symbol not found: {}", symbol))
        }
    }
    
    #[inline]
    pub fn get_symbols(&self) -> Vec<String> {
        self.order_books.read().keys().cloned().collect()
    }
    
    #[inline]
    pub fn submit_order(&self, order: Order) -> Result<OrderResponse> {
        let symbol = order.symbol.clone();
        let order_id = order.id;
        
        if self.config.enable_risk_checks {
            if let Err(e) = self.risk_manager.validate_order(&order) {
                let response = OrderResponse::Rejected {
                    order_id,
                    reason: e.to_string(),
                    timestamp: Utc::now(),
                };
                
                if self.config.enable_event_emission {
                    let _ = self.event_processor.send_event(Event::Order(OrderEvent::OrderRejected {
                        order_id,
                        reason: e.to_string(),
                        timestamp: Utc::now(),
                    }));
                }
                
                return Ok(response);
            }
        }
        
        let order_books = self.order_books.read();
        let order_book = match order_books.get(&symbol) {
            Some(book) => book.clone(),
            None => {
                let response = OrderResponse::Rejected {
                    order_id,
                    reason: format!("Symbol not supported: {}", symbol),
                    timestamp: Utc::now(),
                };
                return Ok(response);
            }
        };
        drop(order_books);
        
        let match_result = order_book.add_order(order.clone());
        
        let response = match match_result {
            MatchResult::NoMatch => {
                if self.config.enable_event_emission {
                    let _ = self.event_processor.send_event(Event::Order(OrderEvent::AddOrder(order)));
                }
                
                OrderResponse::Accepted {
                    order_id,
                    symbol,
                    timestamp: Utc::now(),
                }
            },
            MatchResult::PartialMatch { trades, remaining_quantity } => {
                if self.config.enable_event_emission {
                    for trade in &trades {
                        let _ = self.event_processor.send_event(Event::Trade(TradeEvent::TradeExecuted(trade.clone())));
                    }
                    
                    let _ = self.event_processor.send_event(Event::Order(OrderEvent::OrderFilled {
                        order_id,
                        fill_quantity: order.quantity - remaining_quantity,
                        fill_price: trades.first().map(|t| t.price).unwrap_or(order.price),
                        timestamp: Utc::now(),
                    }));
                }
                
                if self.config.enable_risk_checks {
                    for trade in &trades {
                        let _ = self.risk_manager.process_trade(trade);
                    }
                }
                
                OrderResponse::PartiallyFilled {
                    order_id,
                    trades,
                    remaining_quantity,
                    timestamp: Utc::now(),
                }
            },
            MatchResult::FullMatch { trades } => {
                if self.config.enable_event_emission {
                    for trade in &trades {
                        let _ = self.event_processor.send_event(Event::Trade(TradeEvent::TradeExecuted(trade.clone())));
                    }
                    
                    let _ = self.event_processor.send_event(Event::Order(OrderEvent::OrderFilled {
                        order_id,
                        fill_quantity: order.quantity,
                        fill_price: trades.first().map(|t| t.price).unwrap_or(order.price),
                        timestamp: Utc::now(),
                    }));
                }
                
                if self.config.enable_risk_checks {
                    for trade in &trades {
                        let _ = self.risk_manager.process_trade(trade);
                    }
                }
                
                OrderResponse::FullyFilled {
                    order_id,
                    trades,
                    timestamp: Utc::now(),
                }
            },
        };
        
        Ok(response)
    }
    
    #[inline]
    pub fn cancel_order(&self, symbol: &str, order_id: OrderId) -> Result<CancelResponse> {
        let order_books = self.order_books.read();
        let order_book = match order_books.get(symbol) {
            Some(book) => book.clone(),
            None => {
                return Ok(CancelResponse::NotFound {
                    order_id,
                    timestamp: Utc::now(),
                });
            }
        };
        drop(order_books);
        
        match order_book.cancel_order(order_id) {
            Some(cancelled_order) => {
                if self.config.enable_event_emission {
                    let _ = self.event_processor.send_event(Event::Order(OrderEvent::CancelOrder {
                        order_id,
                        symbol: symbol.to_string(),
                        client_id: cancelled_order.client_id,
                        timestamp: Utc::now(),
                    }));
                }
                
                Ok(CancelResponse::Cancelled {
                    order_id,
                    timestamp: Utc::now(),
                })
            },
            None => {
                Ok(CancelResponse::NotFound {
                    order_id,
                    timestamp: Utc::now(),
                })
            }
        }
    }
    
    #[inline]
    pub fn get_order(&self, symbol: &str, order_id: OrderId) -> Option<Order> {
        let order_books = self.order_books.read();
        order_books.get(symbol)?.get_order(order_id)
    }
    
    #[inline]
    pub fn get_order_book(&self, symbol: &str) -> Option<Arc<OrderBook>> {
        self.order_books.read().get(symbol).cloned()
    }
    
    #[inline]
    pub fn get_market_data(&self, symbol: &str) -> Option<order_book::MarketData> {
        let order_books = self.order_books.read();
        let order_book = order_books.get(symbol)?;
        
        Some(order_book::MarketData {
            symbol: symbol.to_string(),
            best_bid: order_book.best_bid(),
            best_ask: order_book.best_ask(),
            bid_size: order_book.total_volume(Side::Buy),
            ask_size: order_book.total_volume(Side::Sell),
            last_trade_price: None,
            last_trade_quantity: None,
            volume: order_book.total_volume(Side::Buy) + order_book.total_volume(Side::Sell),
            timestamp: Utc::now(),
        })
    }
    
    #[inline]
    pub fn event_processor(&self) -> &Arc<EventProcessor> {
        &self.event_processor
    }
    
    #[inline]
    pub fn risk_manager(&self) -> &Arc<RiskManager> {
        &self.risk_manager
    }
}

impl Default for TradingEngine {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use order_book::{OrderType, Price};
    use uuid::Uuid;
    
    fn create_test_order(symbol: &str, side: Side, price: f64, quantity: f64) -> Order {
        Order::new(
            symbol.to_string(),
            side,
            OrderType::Limit,
            Price::new(price),
            Quantity::new(quantity),
            Uuid::new_v4(),
        )
    }
    
    #[tokio::test]
    async fn test_engine_creation_and_lifecycle() {
        let engine = TradingEngine::new();
        assert!(!engine.is_running());
        
        engine.start().await.unwrap();
        assert!(engine.is_running());
        
        engine.stop().await.unwrap();
        assert!(!engine.is_running());
    }
    
    #[tokio::test]
    async fn test_symbol_management() {
        let engine = TradingEngine::new();
        
        assert!(engine.get_symbols().is_empty());
        
        engine.add_symbol("BTCUSD".to_string()).unwrap();
        engine.add_symbol("ETHUSD".to_string()).unwrap();
        
        let symbols = engine.get_symbols();
        assert_eq!(symbols.len(), 2);
        assert!(symbols.contains(&"BTCUSD".to_string()));
        assert!(symbols.contains(&"ETHUSD".to_string()));
        
        engine.remove_symbol("BTCUSD").unwrap();
        let symbols = engine.get_symbols();
        assert_eq!(symbols.len(), 1);
        assert!(!symbols.contains(&"BTCUSD".to_string()));
    }
    
    #[tokio::test]
    async fn test_order_submission_accepted() {
        let engine = TradingEngine::new();
        engine.add_symbol("BTCUSD".to_string()).unwrap();
        
        let order = create_test_order("BTCUSD", Side::Buy, 50000.0, 1.0);
        let order_id = order.id;
        
        let response = engine.submit_order(order).unwrap();
        
        match response {
            OrderResponse::Accepted { order_id: resp_id, symbol, .. } => {
                assert_eq!(resp_id, order_id);
                assert_eq!(symbol, "BTCUSD");
            },
            _ => panic!("Expected accepted response"),
        }
    }
    
    #[tokio::test]
    async fn test_order_submission_rejected_unknown_symbol() {
        let engine = TradingEngine::new();
        
        let order = create_test_order("UNKNOWN", Side::Buy, 50000.0, 1.0);
        let order_id = order.id;
        
        let response = engine.submit_order(order).unwrap();
        
        match response {
            OrderResponse::Rejected { order_id: resp_id, reason, .. } => {
                assert_eq!(resp_id, order_id);
                assert!(reason.contains("Symbol not supported"));
            },
            _ => panic!("Expected rejected response"),
        }
    }
    
    #[tokio::test]
    async fn test_order_matching() {
        let engine = TradingEngine::new();
        engine.add_symbol("BTCUSD".to_string()).unwrap();
        
        let sell_order = create_test_order("BTCUSD", Side::Sell, 50000.0, 1.0);
        let sell_response = engine.submit_order(sell_order).unwrap();
        assert!(matches!(sell_response, OrderResponse::Accepted { .. }));
        
        let buy_order = create_test_order("BTCUSD", Side::Buy, 50000.0, 1.0);
        let buy_response = engine.submit_order(buy_order).unwrap();
        
        match buy_response {
            OrderResponse::FullyFilled { trades, .. } => {
                assert_eq!(trades.len(), 1);
                assert_eq!(trades[0].price, Price::new(50000.0));
                assert_eq!(trades[0].quantity, Quantity::new(1.0));
            },
            _ => panic!("Expected fully filled response"),
        }
    }
    
    #[tokio::test]
    async fn test_order_cancellation() {
        let engine = TradingEngine::new();
        engine.add_symbol("BTCUSD".to_string()).unwrap();
        
        let order = create_test_order("BTCUSD", Side::Buy, 50000.0, 1.0);
        let order_id = order.id;
        
        let submit_response = engine.submit_order(order).unwrap();
        assert!(matches!(submit_response, OrderResponse::Accepted { .. }));
        
        let cancel_response = engine.cancel_order("BTCUSD", order_id).unwrap();
        
        match cancel_response {
            CancelResponse::Cancelled { order_id: resp_id, .. } => {
                assert_eq!(resp_id, order_id);
            },
            _ => panic!("Expected cancelled response"),
        }
        
        let cancel_response2 = engine.cancel_order("BTCUSD", order_id).unwrap();
        assert!(matches!(cancel_response2, CancelResponse::NotFound { .. }));
    }
    
    #[tokio::test]
    async fn test_market_data_retrieval() {
        let engine = TradingEngine::new();
        engine.add_symbol("BTCUSD".to_string()).unwrap();
        
        let buy_order = create_test_order("BTCUSD", Side::Buy, 49950.0, 1.0);
        let sell_order = create_test_order("BTCUSD", Side::Sell, 50050.0, 1.0);
        
        engine.submit_order(buy_order).unwrap();
        engine.submit_order(sell_order).unwrap();
        
        let market_data = engine.get_market_data("BTCUSD").unwrap();
        assert_eq!(market_data.best_bid, Some(Price::new(49950.0)));
        assert_eq!(market_data.best_ask, Some(Price::new(50050.0)));
        assert_eq!(market_data.bid_size, Quantity::new(1.0));
        assert_eq!(market_data.ask_size, Quantity::new(1.0));
    }
    
    #[tokio::test]
    async fn test_order_retrieval() {
        let engine = TradingEngine::new();
        engine.add_symbol("BTCUSD".to_string()).unwrap();
        
        let order = create_test_order("BTCUSD", Side::Buy, 50000.0, 1.0);
        let order_id = order.id;
        let original_timestamp = order.timestamp;
        
        engine.submit_order(order).unwrap();
        
        let retrieved_order = engine.get_order("BTCUSD", order_id).unwrap();
        assert_eq!(retrieved_order.id, order_id);
        assert_eq!(retrieved_order.symbol, "BTCUSD");
        assert_eq!(retrieved_order.price, Price::new(50000.0));
        assert_eq!(retrieved_order.timestamp, original_timestamp);
        
        assert!(engine.get_order("UNKNOWN", order_id).is_none());
        assert!(engine.get_order("BTCUSD", OrderId::from_raw(99999)).is_none());
    }
    
    #[tokio::test]
    async fn test_engine_with_risk_checks_disabled() {
        let mut config = EngineConfig::default();
        config.enable_risk_checks = false;
        
        let engine = TradingEngine::with_config(config);
        engine.add_symbol("BTCUSD".to_string()).unwrap();
        
        let order = create_test_order("BTCUSD", Side::Buy, 50000.0, 1.0);
        let response = engine.submit_order(order).unwrap();
        
        assert!(matches!(response, OrderResponse::Accepted { .. }));
    }
}