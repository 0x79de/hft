use tracing::{info, warn, error, Level};
#[cfg(feature = "integrations")]
use tracing::debug;
use tokio::signal;
use tokio::time::{interval, Duration};
use std::sync::Arc;
use uuid::Uuid;

use trading_engine::TradingEngine;
use order_book::{Order, OrderType, Side, Price, Quantity};
use event_processor::{Event, OrderEvent, TradeEvent, SystemEvent, HealthStatus};
use risk_manager::RiskLimits;
use latency_profiler::LatencyProfiler;

#[cfg(feature = "integrations")]
use integrations::{IntegrationConfig, okx::{OkxIntegration, websocket::OkxWebSocketEvent}};

#[global_allocator]
static GLOBAL: mimalloc::MiMalloc = mimalloc::MiMalloc;

struct HftSystem {
    trading_engine: Arc<TradingEngine>,
    profiler: Arc<LatencyProfiler>,
    #[cfg(feature = "integrations")]
    okx_integration: Option<Arc<OkxIntegration>>,
}

impl HftSystem {
    async fn new() -> anyhow::Result<Self> {
        info!("Initializing HFT Trading System components...");
        
        let trading_engine = Arc::new(TradingEngine::new());
        let profiler = Arc::new(LatencyProfiler::new());
        
        #[cfg(feature = "integrations")]
        let okx_integration = {
            match IntegrationConfig::from_env() {
                Ok(config) => {
                    info!("Loading OKX integration with environment configuration");
                    match OkxIntegration::new(config.okx).await {
                        Ok(integration) => {
                            info!("OKX integration initialized successfully");
                            Some(Arc::new(integration))
                        }
                        Err(e) => {
                            warn!("Failed to initialize OKX integration: {}", e);
                            warn!("Continuing without OKX integration");
                            None
                        }
                    }
                }
                Err(e) => {
                    warn!("Failed to load integration config: {}", e);
                    warn!("Continuing without OKX integration");
                    None
                }
            }
        };
        
        Ok(Self {
            trading_engine,
            profiler,
            #[cfg(feature = "integrations")]
            okx_integration,
        })
    }
    
    async fn start(&self) -> anyhow::Result<()> {
        info!("Starting HFT Trading System...");
        
        self.trading_engine.start().await?;
        
        self.setup_symbols().await?;
        self.setup_risk_limits().await?;
        self.setup_event_handlers().await?;
        
        #[cfg(feature = "integrations")]
        if let Some(okx) = &self.okx_integration {
            info!("Starting OKX integration...");
            okx.start().await?;
            self.setup_okx_market_data().await?;
            info!("OKX integration started successfully");
        }
        
        info!("HFT Trading System started successfully");
        Ok(())
    }
    
    async fn stop(&self) -> anyhow::Result<()> {
        info!("Stopping HFT Trading System...");
        
        #[cfg(feature = "integrations")]
        if let Some(okx) = &self.okx_integration {
            info!("Stopping OKX integration...");
            okx.stop().await?;
        }
        
        self.trading_engine.stop().await?;
        
        info!("HFT Trading System stopped");
        Ok(())
    }
    
    async fn setup_symbols(&self) -> anyhow::Result<()> {
        let symbols = vec!["BTCUSD", "ETHUSD", "SOLUSD", "ADAUSD"];
        
        for symbol in symbols {
            self.trading_engine.add_symbol(symbol.to_string())?;
            info!("Added symbol: {}", symbol);
        }
        
        Ok(())
    }
    
    async fn setup_risk_limits(&self) -> anyhow::Result<()> {
        let risk_manager = self.trading_engine.risk_manager();
        
        let btc_limits = RiskLimits::with_custom_limits(
            "BTCUSD".to_string(),
            10.0,      // position limit
            50_000.0,  // daily pnl limit
            5.0,       // order size limit
            2.0,       // price deviation limit
            500_000.0, // notional limit
        );
        
        let eth_limits = RiskLimits::with_custom_limits(
            "ETHUSD".to_string(),
            50.0,      // position limit
            25_000.0,  // daily pnl limit
            10.0,      // order size limit
            3.0,       // price deviation limit
            250_000.0, // notional limit
        );
        
        risk_manager.add_symbol_limits("BTCUSD".to_string(), btc_limits);
        risk_manager.add_symbol_limits("ETHUSD".to_string(), eth_limits);
        
        info!("Risk limits configured for all symbols");
        Ok(())
    }
    
    async fn setup_event_handlers(&self) -> anyhow::Result<()> {
        let event_processor = self.trading_engine.event_processor();
        let profiler = Arc::clone(&self.profiler);
        let _trading_engine = Arc::clone(&self.trading_engine);
        
        let order_handler = {
            let profiler = Arc::clone(&profiler);
            Arc::new(move |event: &Event| -> anyhow::Result<()> {
                if let Event::Order(order_event) = event {
                    let _id = profiler.start_measurement(latency_profiler::profiler::MeasurementPoint::EventProcessed);
                    match order_event {
                        OrderEvent::AddOrder(order) => {
                            info!("Order added: {} {} {} @ {}", 
                                order.symbol, order.side, order.quantity, order.price);
                        },
                        OrderEvent::CancelOrder { order_id, symbol, .. } => {
                            info!("Order cancelled: {} for {}", order_id, symbol);
                        },
                        OrderEvent::OrderFilled { order_id, fill_quantity, fill_price, .. } => {
                            info!("Order filled: {} quantity {} @ {}", 
                                order_id, fill_quantity, fill_price);
                        },
                        OrderEvent::OrderRejected { order_id, reason, .. } => {
                            warn!("Order rejected: {} - {}", order_id, reason);
                        },
                        _ => {}
                    }
                }
                Ok(())
            })
        };
        
        let trade_handler = {
            let profiler = Arc::clone(&profiler);
            Arc::new(move |event: &Event| -> anyhow::Result<()> {
                if let Event::Trade(trade_event) = event {
                    let _id = profiler.start_measurement(latency_profiler::profiler::MeasurementPoint::EventProcessed);
                    if let TradeEvent::TradeExecuted(trade) = trade_event {
                        info!("Trade executed: {} {} @ {} ({})", 
                            trade.symbol, trade.quantity, trade.price, trade.id);
                    }
                }
                Ok(())
            })
        };
        
        let system_handler = Arc::new(move |event: &Event| -> anyhow::Result<()> {
            if let Event::System(SystemEvent::SystemHealthCheck { component, status, .. }) = event {
                match status {
                    HealthStatus::Healthy => {
                        info!("Health check: {} is healthy", component);
                    },
                    HealthStatus::Warning => {
                        warn!("Health check: {} has warnings", component);
                    },
                    HealthStatus::Critical | HealthStatus::Down => {
                        error!("Health check: {} is {}", component, 
                            if *status == HealthStatus::Critical { "critical" } else { "down" });
                    },
                }
            }
            Ok(())
        });
        
        event_processor.add_event_handler(order_handler);
        event_processor.add_event_handler(trade_handler);
        event_processor.add_event_handler(system_handler);
        
        info!("Event handlers configured");
        Ok(())
    }
    
    async fn run_demo_trading(&self) -> anyhow::Result<()> {
        info!("Starting demo trading simulation...");
        
        let client1 = Uuid::new_v4();
        let client2 = Uuid::new_v4();
        
        let sell_order = Order::new(
            "BTCUSD".to_string(),
            Side::Sell,
            OrderType::Limit,
            Price::new(45000.0),
            Quantity::new(0.1),
            client1,
        );
        
        let buy_order = Order::new(
            "BTCUSD".to_string(),
            Side::Buy,
            OrderType::Limit,
            Price::new(45000.0),
            Quantity::new(0.1),
            client2,
        );
        
        let id = self.profiler.start_measurement(latency_profiler::profiler::MeasurementPoint::OrderReceived);
        
        let response1 = self.trading_engine.submit_order(sell_order)?;
        info!("Sell order response: {:?}", response1);
        
        let response2 = self.trading_engine.submit_order(buy_order)?;
        
        // End the latency measurement
        self.profiler.end_measurement(id);
        info!("Buy order response: {:?}", response2);
        
        // Force a health check event for demonstration
        let health_event = Event::System(SystemEvent::SystemHealthCheck {
            component: "demo_trading".to_string(),
            status: HealthStatus::Healthy,
            timestamp: chrono::Utc::now(),
        });
        if let Err(e) = self.trading_engine.event_processor().send_event(health_event) {
            warn!("Failed to send demo health event: {}", e);
        } else {
            info!("Sent demo health check event");
        }
        
        // Force a manual order event for demonstration
        let demo_order = Order::new(
            "DEMO".to_string(),
            Side::Buy,
            OrderType::Limit,
            Price::new(1000.0),
            Quantity::new(1.0),
            client2,
        );
        let order_event = Event::Order(OrderEvent::AddOrder(demo_order));
        if let Err(e) = self.trading_engine.event_processor().send_event(order_event) {
            warn!("Failed to send demo order event: {}", e);
        } else {
            info!("Sent demo order event");
        }
        
        tokio::time::sleep(Duration::from_millis(100)).await;
        
        let market_data = self.trading_engine.get_market_data("BTCUSD");
        if let Some(md) = market_data {
            info!("Market data - Bid: {:?}, Ask: {:?}, Spread: {:?}", 
                md.best_bid, md.best_ask, md.best_bid.zip(md.best_ask).map(|(b, a)| a - b));
        }
        
        let risk_metrics = self.trading_engine.risk_manager().get_metrics();
        info!("Risk metrics: {:?}", risk_metrics);
        
        info!("Demo trading completed");
        Ok(())
    }
    
    #[cfg(feature = "integrations")]
    async fn setup_okx_market_data(&self) -> anyhow::Result<()> {
        if let Some(okx) = &self.okx_integration {
            info!("Setting up OKX market data subscriptions...");
            
            // Subscribe to market data for the symbols we're trading
            let symbols = vec!["BTC-USDT", "ETH-USDT", "SOL-USDT", "ADA-USDT"];
            
            for symbol in symbols {
                // Subscribe to ticker data
                if let Err(e) = okx.websocket.subscribe_ticker(symbol).await {
                    warn!("Failed to subscribe to ticker for {}: {}", symbol, e);
                }
                
                // Subscribe to order book data
                if let Err(e) = okx.websocket.subscribe_order_book(symbol).await {
                    warn!("Failed to subscribe to order book for {}: {}", symbol, e);
                }
                
                // Subscribe to trades
                if let Err(e) = okx.websocket.subscribe_trades(symbol).await {
                    warn!("Failed to subscribe to trades for {}: {}", symbol, e);
                }
                
                info!("Subscribed to market data for {}", symbol);
            }
            
            // Subscribe to order updates
            if let Err(e) = okx.websocket.subscribe_orders().await {
                warn!("Failed to subscribe to order updates: {}", e);
            }
            
            // Start market data processing task
            self.start_okx_data_processing().await?;
            
            info!("OKX market data setup completed");
        }
        
        Ok(())
    }
    
    #[cfg(feature = "integrations")]
    async fn start_okx_data_processing(&self) -> anyhow::Result<()> {
        if let Some(okx) = &self.okx_integration {
            let okx_clone = Arc::clone(okx);
            let trading_engine = Arc::clone(&self.trading_engine);
            let profiler = Arc::clone(&self.profiler);
            
            // Start processing WebSocket events
            tokio::spawn(async move {
                if let Some(mut event_rx) = okx_clone.websocket.get_event_receiver().await {
                    info!("Started OKX WebSocket event processing");
                    
                    while let Some(event) = event_rx.recv().await {
                        let _measurement_id = profiler.start_measurement(
                            latency_profiler::profiler::MeasurementPoint::EventProcessed
                        );
                        
                        match event {
                            OkxWebSocketEvent::MarketData(data) => {
                                // Process market data and update our order book
                                Self::process_okx_market_data(&trading_engine, &data).await;
                            }
                            OkxWebSocketEvent::OrderUpdate(data) => {
                                // Process order updates
                                info!("Received order update: {:?}", data);
                            }
                            OkxWebSocketEvent::PositionUpdate(data) => {
                                // Process position updates
                                info!("Received position update: {:?}", data);
                            }
                            OkxWebSocketEvent::AccountUpdate(data) => {
                                // Process account updates
                                info!("Received account update: {:?}", data);
                            }
                            OkxWebSocketEvent::Connected => {
                                info!("OKX WebSocket connected");
                            }
                            OkxWebSocketEvent::Disconnected => {
                                warn!("OKX WebSocket disconnected");
                            }
                            OkxWebSocketEvent::Error(error) => {
                                error!("OKX WebSocket error: {}", error);
                            }
                        }
                    }
                }
            });
        }
        
        Ok(())
    }
    
    #[cfg(feature = "integrations")]
    async fn process_okx_market_data(_trading_engine: &Arc<TradingEngine>, data: &serde_json::Value) {
        // Process different types of market data
        if let Some(data_array) = data.as_array() {
            for item in data_array {
                if let Some(inst_id) = item.get("instId").and_then(|v| v.as_str()) {
                    // Convert OKX symbol format to our internal format
                    let symbol = inst_id.replace("-", "");
                    
                    // Process ticker data
                    if let Some(last_price) = item.get("last").and_then(|v| v.as_str()) {
                        if let Ok(price) = last_price.parse::<f64>() {
                            // Update market data in our system
                            debug!("Updated {} price to {}", symbol, price);
                            
                            // You could emit events to your trading engine here
                            // For example, trigger re-evaluation of trading strategies
                        }
                    }
                    
                    // Process order book data
                    if let (Some(_bids), Some(_asks)) = (item.get("bids"), item.get("asks")) {
                        // Process bid/ask data and update order book
                        debug!("Received order book update for {}", symbol);
                    }
                    
                    // Process trade data
                    if let Some(trade_id) = item.get("tradeId") {
                        debug!("Received trade data for {}: {:?}", symbol, trade_id);
                    }
                }
            }
        }
    }
    
    #[cfg(feature = "integrations")]
    #[allow(dead_code)]
    async fn execute_okx_trade(&self, symbol: &str, side: &str, size: &str, price: Option<&str>) -> anyhow::Result<()> {
        if let Some(okx) = &self.okx_integration {
            info!("Executing OKX trade: {} {} {} @ {:?}", symbol, side, size, price);
            
            // Create a trading signal
            use integrations::types::{TradingSignal, SignalType, SignalSource};
            use rust_decimal::Decimal;
            
            let signal = TradingSignal {
                id: uuid::Uuid::new_v4(),
                symbol: symbol.to_string(),
                signal_type: match side {
                    "buy" => SignalType::Buy,
                    "sell" => SignalType::Sell,
                    _ => SignalType::Hold,
                },
                strength: 0.8,
                confidence: 0.8,
                price_target: price.and_then(|p| p.parse::<Decimal>().ok()),
                stop_loss: None,
                take_profit: None,
                timestamp: chrono::Utc::now(),
                metadata: std::collections::HashMap::new(),
                source: SignalSource::OKX,
            };
            
            // Place the order
            match okx.place_order(&signal).await {
                Ok(response) => {
                    info!("OKX order placed successfully: {:?}", response);
                }
                Err(e) => {
                    error!("Failed to place OKX order: {}", e);
                }
            }
        }
        
        Ok(())
    }
    
    async fn print_performance_stats(&self) {
        let stats = self.profiler.get_all_metrics();
        info!("=== Performance Statistics ===");
        
        if stats.is_empty() {
            info!("No performance measurements recorded");
            return;
        }
        
        for (operation, metrics) in stats.iter() {
            info!("{}: min={:.2}μs, max={:.2}μs, avg={:.2}μs, count={}", 
                operation.as_str(), 
                metrics.min().as_nanos() as f64 / 1000.0,
                metrics.max().as_nanos() as f64 / 1000.0,
                metrics.mean().as_nanos() as f64 / 1000.0,
                metrics.count()
            );
        }
        
        // Also print overall profiler performance stats
        let overall_stats = self.profiler.get_performance_stats();
        if overall_stats.total_measurements > 0 {
            info!("=== Overall Performance ===");
            info!("Total measurements: {}", overall_stats.total_measurements);
            info!("Average latency: {:.2}μs", overall_stats.avg_latency_us());
            info!("Min latency: {:.2}μs", overall_stats.min_latency_us());
            info!("Max latency: {:.2}μs", overall_stats.max_latency_us());
            info!("Active measurements: {}", overall_stats.active_measurements);
        }
    }
    
    async fn health_check_loop(&self) {
        let mut interval = interval(Duration::from_secs(30));
        let event_processor = self.trading_engine.event_processor();
        
        loop {
            interval.tick().await;
            
            let health_event = Event::System(SystemEvent::SystemHealthCheck {
                component: "trading_engine".to_string(),
                status: if self.trading_engine.is_running() {
                    HealthStatus::Healthy
                } else {
                    HealthStatus::Down
                },
                timestamp: chrono::Utc::now(),
            });
            
            if let Err(e) = event_processor.send_event(health_event) {
                error!("Failed to send health check event: {}", e);
            }
        }
    }
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_max_level(Level::INFO)
        .init();

    info!("Starting HFT Trading System v{}", env!("CARGO_PKG_VERSION"));
    
    let system = HftSystem::new().await?;
    
    system.start().await?;
    
    let system_arc = Arc::new(system);
    let health_system = Arc::clone(&system_arc);
    
    tokio::spawn(async move {
        health_system.health_check_loop().await;
    });
    
    system_arc.run_demo_trading().await?;
    
    system_arc.print_performance_stats().await;
    
    info!("System running. Press Ctrl+C to stop...");
    
    signal::ctrl_c().await?;
    
    system_arc.stop().await?;
    
    system_arc.print_performance_stats().await;
    
    info!("HFT Trading System shutdown complete");

    Ok(())
}
