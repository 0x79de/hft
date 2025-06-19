use tracing::{info, warn, error, Level};
use tracing_subscriber;
use tokio::signal;
use tokio::time::{interval, Duration};
use std::sync::Arc;
use uuid::Uuid;

use trading_engine::TradingEngine;
use order_book::{Order, OrderType, Side, Price, Quantity};
use event_processor::{Event, OrderEvent, TradeEvent, SystemEvent, HealthStatus};
use risk_manager::RiskLimits;
use latency_profiler::LatencyProfiler;

#[global_allocator]
static GLOBAL: mimalloc::MiMalloc = mimalloc::MiMalloc;

struct HftSystem {
    trading_engine: Arc<TradingEngine>,
    profiler: Arc<LatencyProfiler>,
}

impl HftSystem {
    async fn new() -> anyhow::Result<Self> {
        info!("Initializing HFT Trading System components...");
        
        let trading_engine = Arc::new(TradingEngine::new());
        let profiler = Arc::new(LatencyProfiler::new());
        
        Ok(Self {
            trading_engine,
            profiler,
        })
    }
    
    async fn start(&self) -> anyhow::Result<()> {
        info!("Starting HFT Trading System...");
        
        self.trading_engine.start().await?;
        
        self.setup_symbols().await?;
        self.setup_risk_limits().await?;
        self.setup_event_handlers().await?;
        
        info!("HFT Trading System started successfully");
        Ok(())
    }
    
    async fn stop(&self) -> anyhow::Result<()> {
        info!("Stopping HFT Trading System...");
        
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
                match event {
                    Event::Order(order_event) => {
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
                    },
                    _ => {}
                }
                Ok(())
            })
        };
        
        let trade_handler = {
            let profiler = Arc::clone(&profiler);
            Arc::new(move |event: &Event| -> anyhow::Result<()> {
                match event {
                    Event::Trade(trade_event) => {
                        let _id = profiler.start_measurement(latency_profiler::profiler::MeasurementPoint::EventProcessed);
                        match trade_event {
                            TradeEvent::TradeExecuted(trade) => {
                                info!("Trade executed: {} {} @ {} ({})", 
                                    trade.symbol, trade.quantity, trade.price, trade.id);
                            },
                            _ => {}
                        }
                    },
                    _ => {}
                }
                Ok(())
            })
        };
        
        let system_handler = Arc::new(move |event: &Event| -> anyhow::Result<()> {
            match event {
                Event::System(system_event) => {
                    match system_event {
                        SystemEvent::SystemHealthCheck { component, status, .. } => {
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
                        },
                        _ => {}
                    }
                },
                _ => {}
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
        
        let _id = self.profiler.start_measurement(latency_profiler::profiler::MeasurementPoint::OrderReceived);
        
        let response1 = self.trading_engine.submit_order(sell_order)?;
        info!("Sell order response: {:?}", response1);
        
        let response2 = self.trading_engine.submit_order(buy_order)?;
        info!("Buy order response: {:?}", response2);
        
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
    
    async fn print_performance_stats(&self) {
        let stats = self.profiler.get_all_metrics();
        info!("=== Performance Statistics ===");
        for (operation, metrics) in stats.iter() {
            info!("{}: min={:.2}μs, max={:.2}μs, avg={:.2}μs, count={}", 
                operation.as_str(), 
                metrics.min().as_nanos() as f64 / 1000.0,
                metrics.max().as_nanos() as f64 / 1000.0,
                metrics.mean().as_nanos() as f64 / 1000.0,
                metrics.count()
            );
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
            
            let _ = event_processor.send_event(health_event);
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
