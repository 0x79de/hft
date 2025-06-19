use std::time::Duration;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LatencyMetrics {
    count: u64,
    sum_ns: u64,
    min_ns: u64,
    max_ns: u64,
    sum_squared_ns: u128,
}

impl LatencyMetrics {
    #[inline]
    pub fn new() -> Self {
        Self {
            count: 0,
            sum_ns: 0,
            min_ns: u64::MAX,
            max_ns: 0,
            sum_squared_ns: 0,
        }
    }
    
    #[inline]
    pub fn record(&mut self, latency: Duration) {
        let ns = latency.as_nanos() as u64;
        
        self.count += 1;
        self.sum_ns += ns;
        self.min_ns = self.min_ns.min(ns);
        self.max_ns = self.max_ns.max(ns);
        self.sum_squared_ns += u128::from(ns) * u128::from(ns);
    }
    
    #[inline]
    pub fn count(&self) -> u64 {
        self.count
    }
    
    #[inline]
    pub fn sum(&self) -> Duration {
        Duration::from_nanos(self.sum_ns)
    }
    
    #[inline]
    pub fn min(&self) -> Duration {
        if self.count == 0 {
            Duration::ZERO
        } else {
            Duration::from_nanos(self.min_ns)
        }
    }
    
    #[inline]
    pub fn max(&self) -> Duration {
        Duration::from_nanos(self.max_ns)
    }
    
    #[inline]
    pub fn mean(&self) -> Duration {
        if self.count == 0 {
            Duration::ZERO
        } else {
            Duration::from_nanos(self.sum_ns / self.count)
        }
    }
    
    #[inline]
    pub fn variance(&self) -> f64 {
        if self.count <= 1 {
            0.0
        } else {
            let mean = self.sum_ns as f64 / self.count as f64;
            let sum_squared = self.sum_squared_ns as f64;
            let count = self.count as f64;
            
            (sum_squared / count - mean * mean).max(0.0)
        }
    }
    
    #[inline]
    pub fn std_dev(&self) -> Duration {
        Duration::from_nanos(self.variance().sqrt() as u64)
    }
    
    #[inline]
    pub fn merge(&mut self, other: &LatencyMetrics) {
        if other.count == 0 {
            return;
        }
        
        if self.count == 0 {
            *self = other.clone();
            return;
        }
        
        self.count += other.count;
        self.sum_ns += other.sum_ns;
        self.min_ns = self.min_ns.min(other.min_ns);
        self.max_ns = self.max_ns.max(other.max_ns);
        self.sum_squared_ns += other.sum_squared_ns;
    }
    
    #[inline]
    pub fn reset(&mut self) {
        self.count = 0;
        self.sum_ns = 0;
        self.min_ns = u64::MAX;
        self.max_ns = 0;
        self.sum_squared_ns = 0;
    }
}

impl Default for LatencyMetrics {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PerformanceStats {
    pub total_measurements: u64,
    pub avg_latency: Duration,
    pub max_latency: Duration,
    pub min_latency: Duration,
    pub active_measurements: usize,
    pub timestamp: DateTime<Utc>,
}

impl PerformanceStats {
    #[inline]
    pub fn new() -> Self {
        Self {
            total_measurements: 0,
            avg_latency: Duration::ZERO,
            max_latency: Duration::ZERO,
            min_latency: Duration::ZERO,
            active_measurements: 0,
            timestamp: Utc::now(),
        }
    }
    
    #[inline]
    pub fn avg_latency_us(&self) -> f64 {
        self.avg_latency.as_nanos() as f64 / 1000.0
    }
    
    #[inline]
    pub fn max_latency_us(&self) -> f64 {
        self.max_latency.as_nanos() as f64 / 1000.0
    }
    
    #[inline]
    pub fn min_latency_us(&self) -> f64 {
        self.min_latency.as_nanos() as f64 / 1000.0
    }
    
    #[inline]
    pub fn throughput_per_second(&self, window_duration: Duration) -> f64 {
        if window_duration.is_zero() {
            0.0
        } else {
            self.total_measurements as f64 / window_duration.as_secs_f64()
        }
    }
}

impl Default for PerformanceStats {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Percentile {
    pub p50: Duration,
    pub p90: Duration,
    pub p95: Duration,
    pub p99: Duration,
    pub p99_9: Duration,
}

impl Percentile {
    #[inline]
    pub fn new() -> Self {
        Self {
            p50: Duration::ZERO,
            p90: Duration::ZERO,
            p95: Duration::ZERO,
            p99: Duration::ZERO,
            p99_9: Duration::ZERO,
        }
    }
    
    #[inline]
    pub fn from_nanos(p50: u64, p90: u64, p95: u64, p99: u64, p99_9: u64) -> Self {
        Self {
            p50: Duration::from_nanos(p50),
            p90: Duration::from_nanos(p90),
            p95: Duration::from_nanos(p95),
            p99: Duration::from_nanos(p99),
            p99_9: Duration::from_nanos(p99_9),
        }
    }
}

impl Default for Percentile {
    fn default() -> Self {
        Self::new()
    }
}