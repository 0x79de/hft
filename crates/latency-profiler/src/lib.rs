pub mod profiler;
pub mod metrics;
pub mod histogram;

pub use profiler::LatencyProfiler;
pub use metrics::*;
pub use histogram::Histogram;

pub type Result<T> = std::result::Result<T, Box<dyn std::error::Error + Send + Sync>>;