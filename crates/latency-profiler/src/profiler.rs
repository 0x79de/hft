use crate::metrics::{LatencyMetrics, PerformanceStats};
use crate::histogram::Histogram;
use std::time::{Duration, Instant};
use std::collections::HashMap;
use parking_lot::RwLock;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
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
    enabled: Arc<AtomicBool>,
}

impl LatencyProfiler {
    #[inline]
    pub fn new() -> Self {
        Self {
            measurements: Arc::new(RwLock::new(HashMap::new())),
            histograms: Arc::new(RwLock::new(HashMap::new())),
            active_measurements: Arc::new(RwLock::new(HashMap::new())),
            measurement_id_counter: Arc::new(parking_lot::Mutex::new(0)),
            enabled: Arc::new(AtomicBool::new(true)),
        }
    }
    
    #[inline]
    pub fn start_measurement(&self, point: MeasurementPoint) -> u64 {
        // Ultra-fast check - if disabled, do absolutely nothing
        if !self.enabled.load(Ordering::Relaxed) {
            return 0;
        }
        
        // Generate unique measurement ID and store start time
        let id = {
            let mut counter = self.measurement_id_counter.lock();
            *counter += 1;
            *counter
        };
        
        let start_time = Instant::now();
        self.active_measurements.write().insert(id, (point, start_time));
        
        id
    }
    
    #[inline]
    pub fn end_measurement(&self, id: u64) -> Option<Duration> {
        // Ultra-fast check - if disabled or invalid ID, do nothing
        if id == 0 || !self.enabled.load(Ordering::Relaxed) {
            return None;
        }
        
        // Calculate duration and record the measurement
        let end_time = Instant::now();
        let (point, start_time) = self.active_measurements.write().remove(&id)?;
        let duration = end_time.duration_since(start_time);
        
        // Record the latency measurement
        self.record_latency(point, duration);
        
        Some(duration)
    }
    
    #[inline]
    pub fn measure_instant(&self, point: MeasurementPoint) {
        if self.enabled.load(Ordering::Relaxed) {
            self.record_latency(point, Duration::ZERO);
        }
    }
    
    #[inline]
    pub fn record_latency(&self, point: MeasurementPoint, latency: Duration) {
        if !self.enabled.load(Ordering::Relaxed) {
            return;
        }
        
        // Try non-blocking approach first, fall back to blocking for reliability
        if let Some(mut measurements) = self.measurements.try_write() {
            let metrics = measurements.entry(point).or_default();
            metrics.record(latency);
            
            // Try histogram too, but don't block if contended
            if let Some(mut histograms) = self.histograms.try_write() {
                let histogram = histograms.entry(point).or_default();
                histogram.record(latency.as_nanos() as u64);
            }
        } else {
            // Fall back to blocking write to ensure measurement is recorded
            let mut measurements = self.measurements.write();
            let metrics = measurements.entry(point).or_default();
            metrics.record(latency);
            
            // Also record in histogram with blocking write
            let mut histograms = self.histograms.write();
            let histogram = histograms.entry(point).or_default();
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
        self.enabled.store(true, Ordering::Relaxed);
    }
    
    #[inline]
    pub fn disable(&self) {
        self.enabled.store(false, Ordering::Relaxed);
    }
    
    #[inline]
    pub fn is_enabled(&self) -> bool {
        self.enabled.load(Ordering::Relaxed)
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
                histogram.map_or(0, |h| h.percentile(50.0)),
                histogram.map_or(0, |h| h.percentile(95.0)),
                histogram.map_or(0, |h| h.percentile(99.0)),
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::thread;
    use tokio::time::{sleep, Duration as TokioDuration};

    #[test]
    fn test_profiler_creation() {
        let profiler = LatencyProfiler::new();
        assert!(profiler.is_enabled());
        assert_eq!(profiler.get_all_metrics().len(), 0);
    }

    #[test]
    fn test_basic_measurement() {
        let profiler = LatencyProfiler::new();
        let point = MeasurementPoint::OrderReceived;
        
        let id = profiler.start_measurement(point);
        thread::sleep(Duration::from_millis(1));
        let duration = profiler.end_measurement(id);
        
        assert!(duration.is_some());
        assert!(duration.unwrap() >= Duration::from_millis(1));
        
        let metrics = profiler.get_metrics(point).unwrap();
        assert_eq!(metrics.count(), 1);
        assert!(metrics.mean() >= Duration::from_millis(1));
    }

    #[test]
    fn test_multiple_measurements() {
        let profiler = LatencyProfiler::new();
        let point = MeasurementPoint::OrderValidated;
        
        // Record multiple measurements
        for i in 0..10 {
            let duration = Duration::from_nanos(1000 + i * 100);
            profiler.record_latency(point, duration);
        }
        
        let metrics = profiler.get_metrics(point).unwrap();
        assert_eq!(metrics.count(), 10);
        assert_eq!(metrics.min(), Duration::from_nanos(1000));
        assert_eq!(metrics.max(), Duration::from_nanos(1900));
    }

    #[test]
    fn test_instant_measurement() {
        let profiler = LatencyProfiler::new();
        let point = MeasurementPoint::EventProcessed;
        
        profiler.measure_instant(point);
        
        let metrics = profiler.get_metrics(point).unwrap();
        assert_eq!(metrics.count(), 1);
        assert_eq!(metrics.mean(), Duration::ZERO);
    }

    #[test]
    fn test_measurement_point_strings() {
        assert_eq!(MeasurementPoint::OrderReceived.as_str(), "order_received");
        assert_eq!(MeasurementPoint::OrderMatched.as_str(), "order_matched");
        assert_eq!(MeasurementPoint::Custom("test").as_str(), "test");
    }

    #[test]
    fn test_scoped_measurement() {
        let profiler = LatencyProfiler::new();
        let point = MeasurementPoint::RiskChecked;
        
        {
            let _measurement = ScopedMeasurement::new(&profiler, point);
            thread::sleep(Duration::from_millis(1));
            // Measurement automatically ends when _measurement is dropped
        }
        
        let metrics = profiler.get_metrics(point).unwrap();
        assert_eq!(metrics.count(), 1);
        assert!(metrics.mean() >= Duration::from_millis(1));
    }

    #[test]
    fn test_measure_macro() {
        let profiler = LatencyProfiler::new();
        let point = MeasurementPoint::TradeSettled;
        
        let result = measure!(&profiler, point, {
            thread::sleep(Duration::from_millis(1));
            42
        });
        
        assert_eq!(result, 42);
        
        let metrics = profiler.get_metrics(point).unwrap();
        assert_eq!(metrics.count(), 1);
        assert!(metrics.mean() >= Duration::from_millis(1));
    }

    #[tokio::test]
    async fn test_measure_async_macro() {
        let profiler = LatencyProfiler::new();
        let point = MeasurementPoint::MarketDataProcessed;
        
        let result = measure_async!(&profiler, point, async {
            sleep(TokioDuration::from_millis(1)).await;
            "test_result"
        });
        
        assert_eq!(result, "test_result");
        
        let metrics = profiler.get_metrics(point).unwrap();
        assert_eq!(metrics.count(), 1);
        assert!(metrics.mean() >= Duration::from_millis(1));
    }

    #[test]
    fn test_profiler_enable_disable() {
        let profiler = LatencyProfiler::new();
        let point = MeasurementPoint::OrderExecuted;
        
        // Disable profiler
        profiler.disable();
        assert!(!profiler.is_enabled());
        
        // This measurement should be ignored
        let id = profiler.start_measurement(point);
        assert_eq!(id, 0);
        let duration = profiler.end_measurement(id);
        assert!(duration.is_none());
        
        // Re-enable profiler
        profiler.enable();
        assert!(profiler.is_enabled());
        
        // This measurement should work
        let id = profiler.start_measurement(point);
        assert_ne!(id, 0);
        let duration = profiler.end_measurement(id);
        assert!(duration.is_some());
    }

    #[test]
    fn test_reset_functionality() {
        let profiler = LatencyProfiler::new();
        let point1 = MeasurementPoint::OrderReceived;
        let point2 = MeasurementPoint::OrderMatched;
        
        // Add some measurements
        profiler.record_latency(point1, Duration::from_millis(1));
        profiler.record_latency(point2, Duration::from_millis(2));
        
        assert_eq!(profiler.get_all_metrics().len(), 2);
        
        // Reset specific point
        profiler.reset_point(point1);
        assert!(profiler.get_metrics(point1).is_none());
        assert!(profiler.get_metrics(point2).is_some());
        
        // Reset all
        profiler.reset();
        assert_eq!(profiler.get_all_metrics().len(), 0);
    }

    #[test]
    fn test_performance_stats() {
        let profiler = LatencyProfiler::new();
        
        // Add measurements for different points
        profiler.record_latency(MeasurementPoint::OrderReceived, Duration::from_millis(1));
        profiler.record_latency(MeasurementPoint::OrderMatched, Duration::from_millis(2));
        profiler.record_latency(MeasurementPoint::OrderExecuted, Duration::from_millis(3));
        
        let stats = profiler.get_performance_stats();
        assert_eq!(stats.total_measurements, 3);
        assert!(stats.avg_latency > Duration::ZERO);
        assert!(stats.max_latency >= Duration::from_millis(3));
        assert!(stats.min_latency > Duration::ZERO);
    }

    #[test]
    fn test_concurrent_measurements() {
        use std::sync::Arc;
        use std::thread;
        
        let profiler = Arc::new(LatencyProfiler::new());
        let mut handles = Vec::new();
        
        // Spawn multiple threads recording measurements
        for i in 0..10 {
            let profiler_clone = profiler.clone();
            let handle = thread::spawn(move || {
                let point = MeasurementPoint::Custom("concurrent_test");
                for _ in 0..100 {
                    let duration = Duration::from_nanos(1000 + i * 10);
                    profiler_clone.record_latency(point, duration);
                }
            });
            handles.push(handle);
        }
        
        // Wait for all threads to complete
        for handle in handles {
            handle.join().unwrap();
        }
        
        let metrics = profiler.get_metrics(MeasurementPoint::Custom("concurrent_test")).unwrap();
        assert_eq!(metrics.count(), 1000); // 10 threads * 100 measurements each
    }

    #[test]
    fn test_histogram_functionality() {
        let profiler = LatencyProfiler::new();
        let point = MeasurementPoint::MarketDataReceived;
        
        // Record measurements with known distribution
        for i in 1..=100 {
            profiler.record_latency(point, Duration::from_nanos(i * 1000));
        }
        
        let histogram = profiler.get_histogram(point).unwrap();
        assert_eq!(histogram.count(), 100);
        
        // Check percentiles are reasonable
        let p50 = histogram.percentile(50.0);
        let p95 = histogram.percentile(95.0);
        let p99 = histogram.percentile(99.0);
        
        assert!(p50 > 0);
        assert!(p95 > p50);
        assert!(p99 > p95);
    }

    #[test]
    fn test_measurement_with_metadata() {
        let mut measurement = Measurement::new(MeasurementPoint::OrderReceived);
        measurement = measurement.with_metadata("symbol".to_string(), "BTCUSD".to_string());
        measurement = measurement.with_duration(Duration::from_millis(1));
        
        assert_eq!(measurement.point, MeasurementPoint::OrderReceived);
        assert_eq!(measurement.metadata.get("symbol"), Some(&"BTCUSD".to_string()));
        assert_eq!(measurement.duration, Some(Duration::from_millis(1)));
    }

    #[test]
    fn test_invalid_measurement_id() {
        let profiler = LatencyProfiler::new();
        
        // Try to end a measurement that doesn't exist
        let duration = profiler.end_measurement(99999);
        assert!(duration.is_none());
    }

    #[test]
    fn test_csv_export() {
        let profiler = LatencyProfiler::new();
        
        // Add some test data
        profiler.record_latency(MeasurementPoint::OrderReceived, Duration::from_millis(1));
        profiler.record_latency(MeasurementPoint::OrderMatched, Duration::from_millis(2));
        
        // Export to temporary file
        let temp_path = "/tmp/test_latency_export.csv";
        let result = profiler.export_csv(temp_path);
        assert!(result.is_ok());
        
        // Verify file was created and has content
        let content = std::fs::read_to_string(temp_path).unwrap();
        assert!(content.contains("measurement_point,count,min_ns"));
        assert!(content.contains("order_received"));
        assert!(content.contains("order_matched"));
        
        // Clean up
        std::fs::remove_file(temp_path).ok();
    }

    #[test]
    fn test_large_number_of_measurements() {
        let profiler = LatencyProfiler::new();
        let point = MeasurementPoint::Custom("stress_test");
        
        // Record many measurements to test performance
        for i in 0..10000 {
            profiler.record_latency(point, Duration::from_nanos(i));
        }
        
        let metrics = profiler.get_metrics(point).unwrap();
        assert_eq!(metrics.count(), 10000);
        assert_eq!(metrics.min(), Duration::from_nanos(0));
        assert_eq!(metrics.max(), Duration::from_nanos(9999));
    }
}