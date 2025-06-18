use crate::metrics::{LatencyMetrics, PerformanceStats};
use crate::histogram::Histogram;
use std::time::{Duration, Instant};
use std::collections::HashMap;
use parking_lot::RwLock;
use std::sync::Arc;
use chrono::Utc;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MeasurementPoint {
    OrderReceived,
    OrderValidated,
    OrderMatched,
    OrderExecuted,
    TradeSettled,
    MarketDataReceived,
    MarketDataProcessed,
    RiskChecked,
    EventProcessed,
    Custom(&'static str),
}

impl MeasurementPoint {
    pub fn as_str(&self) -> &'static str {
        match self {
            MeasurementPoint::OrderReceived => "order_received",
            MeasurementPoint::OrderValidated => "order_validated",
            MeasurementPoint::OrderMatched => "order_matched",
            MeasurementPoint::OrderExecuted => "order_executed",
            MeasurementPoint::TradeSettled => "trade_settled",
            MeasurementPoint::MarketDataReceived => "market_data_received",
            MeasurementPoint::MarketDataProcessed => "market_data_processed",
            MeasurementPoint::RiskChecked => "risk_checked",
            MeasurementPoint::EventProcessed => "event_processed",
            MeasurementPoint::Custom(name) => name,
        }
    }
}

#[derive(Debug, Clone)]
pub struct Measurement {
    pub point: MeasurementPoint,
    pub timestamp: Instant,
    pub duration: Option<Duration>,
    pub metadata: HashMap<String, String>,
}

impl Measurement {
    #[inline]
    pub fn new(point: MeasurementPoint) -> Self {
        Self {
            point,
            timestamp: Instant::now(),
            duration: None,
            metadata: HashMap::new(),
        }
    }
    
    #[inline]
    pub fn with_metadata(mut self, key: String, value: String) -> Self {
        self.metadata.insert(key, value);
        self
    }
    
    #[inline]
    pub fn with_duration(mut self, duration: Duration) -> Self {
        self.duration = Some(duration);
        self
    }
}

#[derive(Debug)]
pub struct LatencyProfiler {
    measurements: Arc<RwLock<HashMap<MeasurementPoint, LatencyMetrics>>>,
    histograms: Arc<RwLock<HashMap<MeasurementPoint, Histogram>>>,
    active_measurements: Arc<RwLock<HashMap<u64, (MeasurementPoint, Instant)>>>,
    measurement_id_counter: Arc<parking_lot::Mutex<u64>>,
    enabled: Arc<RwLock<bool>>,
}

impl LatencyProfiler {
    #[inline]
    pub fn new() -> Self {
        Self {
            measurements: Arc::new(RwLock::new(HashMap::new())),
            histograms: Arc::new(RwLock::new(HashMap::new())),
            active_measurements: Arc::new(RwLock::new(HashMap::new())),
            measurement_id_counter: Arc::new(parking_lot::Mutex::new(0)),
            enabled: Arc::new(RwLock::new(true)),
        }
    }
    
    #[inline]
    pub fn start_measurement(&self, point: MeasurementPoint) -> u64 {
        if !*self.enabled.read() {
            return 0;
        }
        
        let mut counter = self.measurement_id_counter.lock();
        *counter += 1;
        let id = *counter;
        drop(counter);
        
        let start_time = Instant::now();
        self.active_measurements.write().insert(id, (point, start_time));
        
        id
    }
    
    #[inline]
    pub fn end_measurement(&self, id: u64) -> Option<Duration> {
        if !*self.enabled.read() || id == 0 {
            return None;
        }
        
        let end_time = Instant::now();
        
        if let Some((point, start_time)) = self.active_measurements.write().remove(&id) {
            let duration = end_time.duration_since(start_time);
            self.record_latency(point, duration);
            Some(duration)
        } else {
            None
        }
    }
    
    #[inline]
    pub fn measure_instant(&self, point: MeasurementPoint) {
        if *self.enabled.read() {
            self.record_latency(point, Duration::ZERO);
        }
    }
    
    #[inline]
    pub fn record_latency(&self, point: MeasurementPoint, latency: Duration) {
        if !*self.enabled.read() {
            return;
        }
        
        {
            let mut measurements = self.measurements.write();
            let metrics = measurements.entry(point).or_insert_with(LatencyMetrics::new);
            metrics.record(latency);
        }
        
        {
            let mut histograms = self.histograms.write();
            let histogram = histograms.entry(point).or_insert_with(Histogram::new);
            histogram.record(latency.as_nanos() as u64);
        }
    }
    
    #[inline]
    pub fn get_metrics(&self, point: MeasurementPoint) -> Option<LatencyMetrics> {
        self.measurements.read().get(&point).cloned()
    }
    
    #[inline]
    pub fn get_histogram(&self, point: MeasurementPoint) -> Option<Histogram> {
        self.histograms.read().get(&point).cloned()
    }
    
    #[inline]
    pub fn get_all_metrics(&self) -> HashMap<MeasurementPoint, LatencyMetrics> {
        self.measurements.read().clone()
    }
    
    #[inline]
    pub fn get_performance_stats(&self) -> PerformanceStats {
        let measurements = self.measurements.read();
        let total_measurements: u64 = measurements.values().map(|m| m.count()).sum();
        let avg_latency_ns: f64 = measurements.values()
            .map(|m| m.mean().as_nanos() as f64)
            .sum::<f64>() / measurements.len() as f64;
        
        PerformanceStats {
            total_measurements,
            avg_latency: Duration::from_nanos(avg_latency_ns as u64),
            max_latency: measurements.values()
                .map(|m| m.max())
                .max()
                .unwrap_or(Duration::ZERO),
            min_latency: measurements.values()
                .map(|m| m.min())
                .min()
                .unwrap_or(Duration::ZERO),
            active_measurements: self.active_measurements.read().len(),
            timestamp: Utc::now(),
        }
    }
    
    #[inline]
    pub fn reset(&self) {
        self.measurements.write().clear();
        self.histograms.write().clear();
        self.active_measurements.write().clear();
        *self.measurement_id_counter.lock() = 0;
    }
    
    #[inline]
    pub fn reset_point(&self, point: MeasurementPoint) {
        self.measurements.write().remove(&point);
        self.histograms.write().remove(&point);
    }
    
    #[inline]
    pub fn enable(&self) {
        *self.enabled.write() = true;
    }
    
    #[inline]
    pub fn disable(&self) {
        *self.enabled.write() = false;
    }
    
    #[inline]
    pub fn is_enabled(&self) -> bool {
        *self.enabled.read()
    }
    
    pub fn export_csv(&self, path: &str) -> Result<(), Box<dyn std::error::Error>> {
        use std::fs::File;
        use std::io::Write;
        
        let mut file = File::create(path)?;
        writeln!(file, "measurement_point,count,min_ns,max_ns,mean_ns,p50_ns,p95_ns,p99_ns")?;
        
        let measurements = self.measurements.read();
        let histograms = self.histograms.read();
        
        for (point, metrics) in measurements.iter() {
            let histogram = histograms.get(point);
            writeln!(
                file,
                "{},{},{},{},{},{},{},{}",
                point.as_str(),
                metrics.count(),
                metrics.min().as_nanos(),
                metrics.max().as_nanos(),
                metrics.mean().as_nanos(),
                histogram.map(|h| h.percentile(50.0)).unwrap_or(0),
                histogram.map(|h| h.percentile(95.0)).unwrap_or(0),
                histogram.map(|h| h.percentile(99.0)).unwrap_or(0),
            )?;
        }
        
        Ok(())
    }
}

impl Default for LatencyProfiler {
    fn default() -> Self {
        Self::new()
    }
}

pub struct ScopedMeasurement<'a> {
    profiler: &'a LatencyProfiler,
    id: u64,
}

impl<'a> ScopedMeasurement<'a> {
    #[inline]
    pub fn new(profiler: &'a LatencyProfiler, point: MeasurementPoint) -> Self {
        let id = profiler.start_measurement(point);
        Self { profiler, id }
    }
}

impl<'a> Drop for ScopedMeasurement<'a> {
    fn drop(&mut self) {
        self.profiler.end_measurement(self.id);
    }
}

#[macro_export]
macro_rules! measure {
    ($profiler:expr, $point:expr, $code:block) => {{
        let _measurement = $crate::profiler::ScopedMeasurement::new($profiler, $point);
        $code
    }};
}

#[macro_export]
macro_rules! measure_async {
    ($profiler:expr, $point:expr, $code:expr) => {{
        let id = $profiler.start_measurement($point);
        let result = $code.await;
        $profiler.end_measurement(id);
        result
    }};
}