pub mod profiler;
pub mod metrics;
pub mod histogram;
pub mod rdtsc_timer;

pub use profiler::LatencyProfiler;
pub use metrics::*;
pub use histogram::Histogram;
pub use rdtsc_timer::{RdtscTimer, RdtscTimestamp, RdtscProfiler, AtomicLatencyMetrics, LatencySnapshot, RdtscScopedMeasurement, GLOBAL_RDTSC_PROFILER};

pub type Result<T> = anyhow::Result<T>;