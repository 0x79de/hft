//! Metrics collection and monitoring

use metrics::{counter, gauge, histogram};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Instant;

pub struct SystemMetrics {
    orders_processed: AtomicU64,
    trades_executed: AtomicU64,
    latency_measurements: AtomicU64,
}

impl SystemMetrics {
    pub fn new() -> Self {
        Self {
            orders_processed: AtomicU64::new(0),
            trades_executed: AtomicU64::new(0),
            latency_measurements: AtomicU64::new(0),
        }
    }

    pub fn record_order_processed(&self) {
        self.orders_processed.fetch_add(1, Ordering::Relaxed);
        counter!("orders_processed_total").increment(1);
    }

    pub fn record_trade_executed(&self, value: f64) {
        self.trades_executed.fetch_add(1, Ordering::Relaxed);
        counter!("trades_executed_total").increment(1);
        gauge!("last_trade_value").set(value);
    }

    pub fn record_latency(&self, operation: &str, duration_ns: u64) {
        self.latency_measurements.fetch_add(1, Ordering::Relaxed);
        histogram!(format!("{}_latency_ns", operation)).record(duration_ns as f64);
    }

    pub fn get_orders_processed(&self) -> u64 {
        self.orders_processed.load(Ordering::Relaxed)
    }

    pub fn get_trades_executed(&self) -> u64 {
        self.trades_executed.load(Ordering::Relaxed)
    }
}

pub struct LatencyTimer {
    start: Instant,
    operation: String,
}

impl LatencyTimer {
    pub fn start(operation: String) -> Self {
        Self {
            start: Instant::now(),
            operation,
        }
    }

    pub fn finish(self, metrics: &SystemMetrics) {
        let duration_ns = self.start.elapsed().as_nanos() as u64;
        metrics.record_latency(&self.operation, duration_ns);
    }
}