use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use std::sync::Arc;

/// RDTSC-based high-precision timer for sub-nanosecond latency measurement
/// Uses CPU cycle counters for maximum precision and minimal overhead
#[derive(Debug)]
pub struct RdtscTimer {
    /// CPU frequency in Hz (cycles per second)
    frequency: f64,
    /// Baseline offset for timestamp conversion
    baseline_cycles: u64,
    /// Baseline system time for timestamp conversion
    baseline_time_nanos: u64,
}

impl RdtscTimer {
    /// Create a new RDTSC timer with automatic frequency calibration
    pub fn new() -> Self {
        let (frequency, baseline_cycles, baseline_time_nanos) = Self::calibrate_frequency();
        
        Self {
            frequency,
            baseline_cycles,
            baseline_time_nanos,
        }
    }
    
    /// Create a timer with a known CPU frequency (for better performance)
    pub fn with_frequency(frequency_hz: f64) -> Self {
        let baseline_cycles = unsafe { rdtsc() };
        let baseline_time_nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos() as u64;
        
        Self {
            frequency: frequency_hz,
            baseline_cycles,
            baseline_time_nanos,
        }
    }
    
    /// Get current timestamp in CPU cycles
    #[inline]
    pub fn now_cycles(&self) -> u64 {
        unsafe { rdtsc() }
    }
    
    /// Get current timestamp as RdtscTimestamp
    #[inline]
    pub fn now(&self) -> RdtscTimestamp {
        RdtscTimestamp {
            cycles: unsafe { rdtsc() },
        }
    }
    
    /// Convert cycles to nanoseconds
    #[inline]
    pub fn cycles_to_nanos(&self, cycles: u64) -> u64 {
        ((cycles as f64) / self.frequency * 1_000_000_000.0) as u64
    }
    
    /// Convert nanoseconds to cycles
    #[inline]
    pub fn nanos_to_cycles(&self, nanos: u64) -> u64 {
        ((nanos as f64) / 1_000_000_000.0 * self.frequency) as u64
    }
    
    /// Calculate duration between two timestamps in nanoseconds
    #[inline]
    pub fn duration_nanos(&self, start: RdtscTimestamp, end: RdtscTimestamp) -> u64 {
        if end.cycles >= start.cycles {
            self.cycles_to_nanos(end.cycles - start.cycles)
        } else {
            // Handle cycle counter overflow (very rare but possible)
            let overflow_cycles = u64::MAX - start.cycles + end.cycles + 1;
            self.cycles_to_nanos(overflow_cycles)
        }
    }
    
    /// Calculate duration between two timestamps as Duration
    #[inline]
    pub fn duration(&self, start: RdtscTimestamp, end: RdtscTimestamp) -> Duration {
        Duration::from_nanos(self.duration_nanos(start, end))
    }
    
    /// Convert RDTSC timestamp to system time (approximate)
    #[inline]
    pub fn to_system_time(&self, timestamp: RdtscTimestamp) -> SystemTime {
        let elapsed_cycles = timestamp.cycles.saturating_sub(self.baseline_cycles);
        let elapsed_nanos = self.cycles_to_nanos(elapsed_cycles);
        let total_nanos = self.baseline_time_nanos + elapsed_nanos;
        
        UNIX_EPOCH + Duration::from_nanos(total_nanos)
    }
    
    /// Get CPU frequency in Hz
    #[inline]
    pub fn frequency(&self) -> f64 {
        self.frequency
    }
    
    /// Calibrate CPU frequency by measuring against system clock
    fn calibrate_frequency() -> (f64, u64, u64) {
        // Multiple calibration rounds for better accuracy
        let mut frequencies = Vec::new();
        
        for _ in 0..5 {
            let calibration_time = Duration::from_millis(100);
            
            let start_time = SystemTime::now();
            let start_cycles = unsafe { rdtsc() };
            
            // Busy wait for more accurate timing
            let target_time = start_time + calibration_time;
            while SystemTime::now() < target_time {
                std::hint::spin_loop();
            }
            
            let end_time = SystemTime::now();
            let end_cycles = unsafe { rdtsc() };
            
            let duration_nanos = end_time.duration_since(start_time).unwrap().as_nanos() as f64;
            let cycle_diff = (end_cycles - start_cycles) as f64;
            
            let frequency = cycle_diff / (duration_nanos / 1_000_000_000.0);
            frequencies.push(frequency);
        }
        
        // Use median frequency for better accuracy
        frequencies.sort_by(|a, b| a.partial_cmp(b).unwrap());
        let median_frequency = frequencies[frequencies.len() / 2];
        
        // Get baseline for timestamp conversion
        let baseline_cycles = unsafe { rdtsc() };
        let baseline_time_nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos() as u64;
        
        (median_frequency, baseline_cycles, baseline_time_nanos)
    }
    
    /// Re-calibrate the timer (useful for long-running processes)
    pub fn recalibrate(&mut self) {
        let (frequency, baseline_cycles, baseline_time_nanos) = Self::calibrate_frequency();
        self.frequency = frequency;
        self.baseline_cycles = baseline_cycles;
        self.baseline_time_nanos = baseline_time_nanos;
    }
}

impl Default for RdtscTimer {
    fn default() -> Self {
        Self::new()
    }
}

// Safe to send between threads (frequency is read-only after construction)
unsafe impl Send for RdtscTimer {}
unsafe impl Sync for RdtscTimer {}

/// High-precision timestamp based on CPU cycle counter
#[derive(Copy, Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct RdtscTimestamp {
    cycles: u64,
}

impl RdtscTimestamp {
    /// Create timestamp from raw cycle count
    #[inline]
    pub fn from_cycles(cycles: u64) -> Self {
        Self { cycles }
    }
    
    /// Get raw cycle count
    #[inline]
    pub fn cycles(&self) -> u64 {
        self.cycles
    }
    
    /// Get current timestamp
    #[inline]
    pub fn now() -> Self {
        Self {
            cycles: unsafe { rdtsc() },
        }
    }
    
    /// Calculate duration since this timestamp
    #[inline]
    pub fn elapsed_cycles(&self) -> u64 {
        let now = unsafe { rdtsc() };
        now.saturating_sub(self.cycles)
    }
}

/// Lock-free high-precision profiler using RDTSC
#[derive(Debug)]
pub struct RdtscProfiler {
    timer: RdtscTimer,
    measurements: crossbeam_skiplist::SkipMap<&'static str, Arc<AtomicLatencyMetrics>>,
}

impl RdtscProfiler {
    /// Create a new RDTSC profiler
    pub fn new() -> Self {
        Self {
            timer: RdtscTimer::new(),
            measurements: crossbeam_skiplist::SkipMap::new(),
        }
    }
    
    /// Create profiler with known CPU frequency
    pub fn with_frequency(frequency_hz: f64) -> Self {
        Self {
            timer: RdtscTimer::with_frequency(frequency_hz),
            measurements: crossbeam_skiplist::SkipMap::new(),
        }
    }
    
    /// Record a latency measurement (fastest path)
    #[inline]
    pub fn record_latency(&self, point: &'static str, nanos: u64) {
        let metrics = self.measurements
            .get_or_insert_with(point, || Arc::new(AtomicLatencyMetrics::new()));
        
        metrics.value().record(nanos);
    }
    
    /// Record latency between two RDTSC timestamps
    #[inline]
    pub fn record_duration(&self, point: &'static str, start: RdtscTimestamp, end: RdtscTimestamp) {
        let nanos = self.timer.duration_nanos(start, end);
        self.record_latency(point, nanos);
    }
    
    /// Start a measurement and return timestamp
    #[inline]
    pub fn start(&self) -> RdtscTimestamp {
        RdtscTimestamp::now()
    }
    
    /// End a measurement and record the result
    #[inline]
    pub fn end(&self, point: &'static str, start: RdtscTimestamp) -> u64 {
        let end = RdtscTimestamp::now();
        let nanos = self.timer.duration_nanos(start, end);
        self.record_latency(point, nanos);
        nanos
    }
    
    /// Get metrics for a measurement point
    pub fn get_metrics(&self, point: &str) -> Option<LatencySnapshot> {
        self.measurements.get(point).map(|entry| {
            entry.value().snapshot()
        })
    }
    
    /// Get all measurement points and their metrics
    pub fn get_all_metrics(&self) -> Vec<(&'static str, LatencySnapshot)> {
        self.measurements
            .iter()
            .map(|entry| (*entry.key(), entry.value().snapshot()))
            .collect()
    }
    
    /// Reset all measurements
    pub fn reset(&self) {
        self.measurements.clear();
    }
    
    /// Reset specific measurement point
    pub fn reset_point(&self, point: &str) {
        self.measurements.remove(point);
    }
    
    /// Get the underlying timer
    pub fn timer(&self) -> &RdtscTimer {
        &self.timer
    }
    
    /// Export metrics to CSV format
    pub fn export_csv(&self, path: &str) -> Result<(), Box<dyn std::error::Error>> {
        use std::fs::File;
        use std::io::Write;
        
        let mut file = File::create(path)?;
        writeln!(file, "measurement_point,count,min_ns,max_ns,mean_ns,total_ns")?;
        
        for (point, metrics) in self.get_all_metrics() {
            writeln!(
                file,
                "{},{},{},{},{},{}",
                point,
                metrics.count,
                metrics.min_nanos,
                metrics.max_nanos,
                metrics.mean_nanos(),
                metrics.total_nanos,
            )?;
        }
        
        Ok(())
    }
}

impl Default for RdtscProfiler {
    fn default() -> Self {
        Self::new()
    }
}

unsafe impl Send for RdtscProfiler {}
unsafe impl Sync for RdtscProfiler {}

/// Lock-free atomic latency metrics
#[derive(Debug)]
#[repr(C, align(64))]
pub struct AtomicLatencyMetrics {
    count: AtomicU64,
    total_nanos: AtomicU64,
    min_nanos: AtomicU64,
    max_nanos: AtomicU64,
    // Histogram buckets for percentile calculation (powers of 2)
    histogram: [AtomicU64; 32],
    // Padding to prevent false sharing
    _padding: [u8; 64],
}

impl AtomicLatencyMetrics {
    pub fn new() -> Self {
        const INIT: AtomicU64 = AtomicU64::new(0);
        Self {
            count: AtomicU64::new(0),
            total_nanos: AtomicU64::new(0),
            min_nanos: AtomicU64::new(u64::MAX),
            max_nanos: AtomicU64::new(0),
            histogram: [INIT; 32],
            _padding: [0; 64],
        }
    }
    
    /// Record a latency measurement
    #[inline]
    pub fn record(&self, nanos: u64) {
        // Update counters
        self.count.fetch_add(1, Ordering::Relaxed);
        self.total_nanos.fetch_add(nanos, Ordering::Relaxed);
        
        // Update min with compare-and-swap loop
        self.update_min(nanos);
        
        // Update max with compare-and-swap loop
        self.update_max(nanos);
        
        // Update histogram
        let bucket = if nanos == 0 { 0 } else { 63 - nanos.leading_zeros() } as usize;
        if bucket < 32 {
            self.histogram[bucket].fetch_add(1, Ordering::Relaxed);
        }
    }
    
    #[inline]
    fn update_min(&self, value: u64) {
        let mut current = self.min_nanos.load(Ordering::Relaxed);
        while value < current {
            match self.min_nanos.compare_exchange_weak(
                current,
                value,
                Ordering::Relaxed,
                Ordering::Relaxed
            ) {
                Ok(_) => break,
                Err(actual) => current = actual,
            }
        }
    }
    
    #[inline]
    fn update_max(&self, value: u64) {
        let mut current = self.max_nanos.load(Ordering::Relaxed);
        while value > current {
            match self.max_nanos.compare_exchange_weak(
                current,
                value,
                Ordering::Relaxed,
                Ordering::Relaxed
            ) {
                Ok(_) => break,
                Err(actual) => current = actual,
            }
        }
    }
    
    /// Get a snapshot of current metrics
    pub fn snapshot(&self) -> LatencySnapshot {
        let count = self.count.load(Ordering::Acquire);
        let total_nanos = self.total_nanos.load(Ordering::Acquire);
        let min_nanos = self.min_nanos.load(Ordering::Acquire);
        let max_nanos = self.max_nanos.load(Ordering::Acquire);
        
        // Collect histogram
        let mut histogram = [0u64; 32];
        for i in 0..32 {
            histogram[i] = self.histogram[i].load(Ordering::Acquire);
        }
        
        LatencySnapshot {
            count,
            total_nanos,
            min_nanos: if min_nanos == u64::MAX { 0 } else { min_nanos },
            max_nanos,
            histogram,
        }
    }
}

impl Default for AtomicLatencyMetrics {
    fn default() -> Self {
        Self::new()
    }
}

unsafe impl Send for AtomicLatencyMetrics {}
unsafe impl Sync for AtomicLatencyMetrics {}

/// Snapshot of latency metrics at a point in time
#[derive(Debug, Clone)]
pub struct LatencySnapshot {
    pub count: u64,
    pub total_nanos: u64,
    pub min_nanos: u64,
    pub max_nanos: u64,
    pub histogram: [u64; 32],
}

impl LatencySnapshot {
    /// Calculate mean latency in nanoseconds
    pub fn mean_nanos(&self) -> u64 {
        if self.count == 0 {
            0
        } else {
            self.total_nanos / self.count
        }
    }
    
    /// Calculate percentile from histogram (approximate)
    pub fn percentile(&self, percentile: f64) -> u64 {
        if self.count == 0 {
            return 0;
        }
        
        let target_count = (self.count as f64 * percentile / 100.0) as u64;
        let mut cumulative_count = 0;
        
        for (bucket, &count) in self.histogram.iter().enumerate() {
            cumulative_count += count;
            if cumulative_count >= target_count {
                // Return the upper bound of this bucket
                return 1u64 << bucket;
            }
        }
        
        self.max_nanos
    }
    
    /// Convert to Duration types for compatibility
    pub fn mean_duration(&self) -> Duration {
        Duration::from_nanos(self.mean_nanos())
    }
    
    pub fn min_duration(&self) -> Duration {
        Duration::from_nanos(self.min_nanos)
    }
    
    pub fn max_duration(&self) -> Duration {
        Duration::from_nanos(self.max_nanos)
    }
}

/// Scoped measurement using RDTSC for automatic timing
pub struct RdtscScopedMeasurement<'a> {
    profiler: &'a RdtscProfiler,
    point: &'static str,
    start: RdtscTimestamp,
}

impl<'a> RdtscScopedMeasurement<'a> {
    #[inline]
    pub fn new(profiler: &'a RdtscProfiler, point: &'static str) -> Self {
        Self {
            profiler,
            point,
            start: RdtscTimestamp::now(),
        }
    }
}

impl<'a> Drop for RdtscScopedMeasurement<'a> {
    #[inline]
    fn drop(&mut self) {
        self.profiler.end(self.point, self.start);
    }
}

// Global RDTSC profiler instance for easy access
lazy_static::lazy_static! {
    pub static ref GLOBAL_RDTSC_PROFILER: RdtscProfiler = RdtscProfiler::new();
}

/// CPU cycle counter intrinsic
#[inline]
unsafe fn rdtsc() -> u64 {
    #[cfg(target_arch = "x86_64")]
    {
        std::arch::x86_64::_rdtsc()
    }
    #[cfg(target_arch = "x86")]
    {
        std::arch::x86::_rdtsc()
    }
    #[cfg(not(any(target_arch = "x86", target_arch = "x86_64")))]
    {
        // Fallback for non-x86 architectures
        // Use system time as approximation
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos() as u64
    }
}

/// Convenience macros for RDTSC measurements
#[macro_export]
macro_rules! rdtsc_measure {
    ($profiler:expr, $point:expr, $code:block) => {{
        let _measurement = $crate::rdtsc_timer::RdtscScopedMeasurement::new($profiler, $point);
        $code
    }};
}

#[macro_export]
macro_rules! rdtsc_time {
    ($point:expr, $code:block) => {{
        let _measurement = $crate::rdtsc_timer::RdtscScopedMeasurement::new(
            &$crate::rdtsc_timer::GLOBAL_RDTSC_PROFILER, 
            $point
        );
        $code
    }};
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::thread;
    use std::sync::Arc;

    #[test]
    fn test_rdtsc_timer_creation() {
        let timer = RdtscTimer::new();
        assert!(timer.frequency() > 0.0);
        println!("Detected CPU frequency: {:.2} GHz", timer.frequency() / 1e9);
    }

    #[test]
    fn test_rdtsc_timestamp() {
        let ts1 = RdtscTimestamp::now();
        thread::sleep(Duration::from_micros(1));
        let ts2 = RdtscTimestamp::now();
        
        assert!(ts2 > ts1);
        assert!(ts2.cycles() > ts1.cycles());
    }

    #[test]
    fn test_duration_calculation() {
        let timer = RdtscTimer::new();
        let start = timer.now();
        
        // Busy wait for more precise timing
        let target_cycles = timer.nanos_to_cycles(1000); // 1 microsecond
        let start_cycles = start.cycles();
        while unsafe { rdtsc() } - start_cycles < target_cycles {
            std::hint::spin_loop();
        }
        
        let end = timer.now();
        let duration_nanos = timer.duration_nanos(start, end);
        
        // Should be at least 1 microsecond
        assert!(duration_nanos >= 900); // Allow some variance
        assert!(duration_nanos < 10_000); // But not too much
    }

    #[test]
    fn test_rdtsc_profiler() {
        let profiler = RdtscProfiler::new();
        
        // Record some measurements
        for i in 0..100 {
            profiler.record_latency("test_point", 1000 + i * 10);
        }
        
        let metrics = profiler.get_metrics("test_point").unwrap();
        assert_eq!(metrics.count, 100);
        assert_eq!(metrics.min_nanos, 1000);
        assert_eq!(metrics.max_nanos, 1990);
        assert!(metrics.mean_nanos() > 1000);
    }

    #[test]
    fn test_atomic_latency_metrics() {
        let metrics = AtomicLatencyMetrics::new();
        
        // Record measurements
        for i in 0..1000 {
            metrics.record(i);
        }
        
        let snapshot = metrics.snapshot();
        assert_eq!(snapshot.count, 1000);
        assert_eq!(snapshot.min_nanos, 0);
        assert_eq!(snapshot.max_nanos, 999);
        assert_eq!(snapshot.mean_nanos(), 499); // Average of 0..999
    }

    #[test]
    fn test_scoped_measurement() {
        let profiler = RdtscProfiler::new();
        
        {
            let _measurement = RdtscScopedMeasurement::new(&profiler, "scoped_test");
            // Do some work
            for _ in 0..100 {
                std::hint::black_box(42);
            }
        }
        
        let metrics = profiler.get_metrics("scoped_test").unwrap();
        assert_eq!(metrics.count, 1);
        assert!(metrics.min_nanos > 0);
    }

    #[test]
    fn test_rdtsc_measure_macro() {
        let profiler = RdtscProfiler::new();
        
        let result = rdtsc_measure!(&profiler, "macro_test", {
            // Do some work
            let mut sum = 0;
            for i in 0..100 {
                sum += i;
            }
            sum
        });
        
        assert_eq!(result, 4950); // Sum of 0..99
        
        let metrics = profiler.get_metrics("macro_test").unwrap();
        assert_eq!(metrics.count, 1);
        assert!(metrics.min_nanos > 0);
    }

    #[test]
    fn test_concurrent_measurements() {
        let profiler = Arc::new(RdtscProfiler::new());
        let num_threads = 10;
        let measurements_per_thread = 1000;
        
        let handles: Vec<_> = (0..num_threads).map(|thread_id| {
            let profiler = profiler.clone();
            thread::spawn(move || {
                for i in 0..measurements_per_thread {
                    let latency = thread_id * 1000 + i;
                    profiler.record_latency("concurrent_test", latency);
                }
            })
        }).collect();
        
        for handle in handles {
            handle.join().unwrap();
        }
        
        let metrics = profiler.get_metrics("concurrent_test").unwrap();
        assert_eq!(metrics.count, (num_threads * measurements_per_thread) as u64);
    }

    #[test]
    fn test_percentile_calculation() {
        let profiler = RdtscProfiler::new();
        
        // Record measurements with known distribution
        for i in 1..=1000 {
            profiler.record_latency("percentile_test", i);
        }
        
        let metrics = profiler.get_metrics("percentile_test").unwrap();
        
        let p50 = metrics.percentile(50.0);
        let p95 = metrics.percentile(95.0);
        let p99 = metrics.percentile(99.0);
        
        assert!(p50 > 0);
        assert!(p95 > p50);
        assert!(p99 > p95);
        
        println!("P50: {} ns, P95: {} ns, P99: {} ns", p50, p95, p99);
    }

    #[test]
    fn test_timer_frequency_consistency() {
        let timer1 = RdtscTimer::new();
        thread::sleep(Duration::from_millis(10));
        let timer2 = RdtscTimer::new();
        
        // Frequencies should be similar (within 1%)
        let freq_diff = (timer1.frequency() - timer2.frequency()).abs();
        let freq_avg = (timer1.frequency() + timer2.frequency()) / 2.0;
        let relative_diff = freq_diff / freq_avg;
        
        assert!(relative_diff < 0.01, "Frequency difference too large: {:.2}%", relative_diff * 100.0);
    }

    #[test]
    fn test_csv_export() {
        let profiler = RdtscProfiler::new();
        
        // Add test data
        profiler.record_latency("test1", 1000);
        profiler.record_latency("test2", 2000);
        
        let temp_path = "/tmp/test_rdtsc_export.csv";
        let result = profiler.export_csv(temp_path);
        assert!(result.is_ok());
        
        let content = std::fs::read_to_string(temp_path).unwrap();
        assert!(content.contains("measurement_point,count,min_ns"));
        assert!(content.contains("test1"));
        assert!(content.contains("test2"));
        
        // Clean up
        std::fs::remove_file(temp_path).ok();
    }

    #[test]
    fn test_global_profiler() {
        let result = rdtsc_time!("global_test", {
            // Do some work
            let mut sum = 0;
            for i in 0..50 {
                sum += i;
            }
            sum
        });
        
        assert_eq!(result, 1225); // Sum of 0..49
        
        let metrics = GLOBAL_RDTSC_PROFILER.get_metrics("global_test").unwrap();
        assert_eq!(metrics.count, 1);
    }

    #[test]
    fn test_timestamp_ordering() {
        let mut timestamps = Vec::new();
        
        for _ in 0..100 {
            timestamps.push(RdtscTimestamp::now());
            // Small delay to ensure different timestamps
            for _ in 0..10 {
                std::hint::black_box(42);
            }
        }
        
        // Timestamps should be in ascending order
        for window in timestamps.windows(2) {
            assert!(window[1] >= window[0], "Timestamps not in order: {:?}", window);
        }
    }
}