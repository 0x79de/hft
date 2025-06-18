use hdrhistogram::Histogram as HdrHistogram;
use std::time::Duration;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone)]
pub struct Histogram {
    inner: HdrHistogram<u64>,
    count: u64,
}

impl Histogram {
    #[inline]
    pub fn new() -> Self {
        Self {
            inner: HdrHistogram::<u64>::new(3).expect("Failed to create histogram"),
            count: 0,
        }
    }
    
    #[inline]
    pub fn with_bounds(min: u64, max: u64, precision: u32) -> Self {
        Self {
            inner: HdrHistogram::<u64>::new_with_bounds(min, max, precision as u8)
                .expect("Failed to create histogram"),
            count: 0,
        }
    }
    
    #[inline]
    pub fn record(&mut self, value: u64) {
        if self.inner.record(value).is_ok() {
            self.count += 1;
        }
    }
    
    #[inline]
    pub fn record_duration(&mut self, duration: Duration) {
        self.record(duration.as_nanos() as u64);
    }
    
    #[inline]
    pub fn percentile(&self, percentile: f64) -> u64 {
        self.inner.value_at_percentile(percentile)
    }
    
    #[inline]
    pub fn percentile_duration(&self, percentile: f64) -> Duration {
        Duration::from_nanos(self.percentile(percentile))
    }
    
    #[inline]
    pub fn min(&self) -> u64 {
        self.inner.min()
    }
    
    #[inline]
    pub fn max(&self) -> u64 {
        self.inner.max()
    }
    
    #[inline]
    pub fn mean(&self) -> f64 {
        self.inner.mean()
    }
    
    #[inline]
    pub fn count(&self) -> u64 {
        self.count
    }
    
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.count == 0
    }
    
    #[inline]
    pub fn reset(&mut self) {
        self.inner.reset();
        self.count = 0;
    }
    
    #[inline]
    pub fn merge(&mut self, other: &Histogram) {
        if self.inner.add(&other.inner).is_ok() {
            self.count += other.count;
        }
    }
    
    pub fn percentiles(&self) -> HistogramPercentiles {
        HistogramPercentiles {
            p50: self.percentile(50.0),
            p90: self.percentile(90.0),
            p95: self.percentile(95.0),
            p99: self.percentile(99.0),
            p99_9: self.percentile(99.9),
            p99_99: self.percentile(99.99),
        }
    }
    
    pub fn duration_percentiles(&self) -> DurationPercentiles {
        DurationPercentiles {
            p50: self.percentile_duration(50.0),
            p90: self.percentile_duration(90.0),
            p95: self.percentile_duration(95.0),
            p99: self.percentile_duration(99.0),
            p99_9: self.percentile_duration(99.9),
            p99_99: self.percentile_duration(99.99),
        }
    }
    
    pub fn export_percentiles(&self, percentiles: &[f64]) -> Vec<(f64, u64)> {
        percentiles.iter()
            .map(|&p| (p, self.percentile(p)))
            .collect()
    }
    
    pub fn export_csv(&self, path: &str) -> Result<(), Box<dyn std::error::Error>> {
        use std::fs::File;
        use std::io::Write;
        
        let mut file = File::create(path)?;
        writeln!(file, "percentile,value_ns")?;
        
        let percentiles = [50.0, 90.0, 95.0, 99.0, 99.9, 99.99];
        for percentile in &percentiles {
            writeln!(file, "{},{}", percentile, self.percentile(*percentile))?;
        }
        
        Ok(())
    }
}

impl Default for Histogram {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HistogramPercentiles {
    pub p50: u64,
    pub p90: u64,
    pub p95: u64,
    pub p99: u64,
    pub p99_9: u64,
    pub p99_99: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DurationPercentiles {
    pub p50: Duration,
    pub p90: Duration,
    pub p95: Duration,
    pub p99: Duration,
    pub p99_9: Duration,
    pub p99_99: Duration,
}

impl DurationPercentiles {
    #[inline]
    pub fn p50_us(&self) -> f64 {
        self.p50.as_nanos() as f64 / 1000.0
    }
    
    #[inline]
    pub fn p90_us(&self) -> f64 {
        self.p90.as_nanos() as f64 / 1000.0
    }
    
    #[inline]
    pub fn p95_us(&self) -> f64 {
        self.p95.as_nanos() as f64 / 1000.0
    }
    
    #[inline]
    pub fn p99_us(&self) -> f64 {
        self.p99.as_nanos() as f64 / 1000.0
    }
    
    #[inline]
    pub fn p99_9_us(&self) -> f64 {
        self.p99_9.as_nanos() as f64 / 1000.0
    }
    
    #[inline]
    pub fn p99_99_us(&self) -> f64 {
        self.p99_99.as_nanos() as f64 / 1000.0
    }
}