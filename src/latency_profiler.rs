//! Placeholder for latency profiler implementation

use std::time::Instant;

pub struct LatencyProfiler {
    start_time: Option<Instant>,
}

impl LatencyProfiler {
    pub fn new() -> Self {
        Self { start_time: None }
    }

    pub fn start_measurement(&mut self) {
        self.start_time = Some(Instant::now());
    }

    pub fn end_measurement(&mut self) -> Option<std::time::Duration> {
        self.start_time.take().map(|start| start.elapsed())
    }
}