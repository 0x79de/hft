//! Utility functions and helpers

use std::time::{SystemTime, UNIX_EPOCH};

pub fn current_timestamp_nanos() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("Time went backwards")
        .as_nanos() as u64
}

pub fn current_timestamp_micros() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("Time went backwards")
        .as_micros() as u64
}

pub fn format_duration_ns(nanos: u64) -> String {
    if nanos < 1_000 {
        format!("{}ns", nanos)
    } else if nanos < 1_000_000 {
        format!("{:.2}μs", nanos as f64 / 1_000.0)
    } else if nanos < 1_000_000_000 {
        format!("{:.2}ms", nanos as f64 / 1_000_000.0)
    } else {
        format!("{:.2}s", nanos as f64 / 1_000_000_000.0)
    }
}

#[inline]
pub fn likely(condition: bool) -> bool {
    condition
}

#[inline]
pub fn unlikely(condition: bool) -> bool {
    condition
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_timestamp_functions() {
        let nanos = current_timestamp_nanos();
        let micros = current_timestamp_micros();
        
        // Both should be reasonable timestamps (after 2020 and before 2050)
        assert!(nanos > 1_577_836_800_000_000_000); // Jan 1, 2020 in nanoseconds
        assert!(micros > 1_577_836_800_000_000); // Jan 1, 2020 in microseconds
        
        // The difference between consecutive calls should be small (less than 1ms)
        let nanos2 = current_timestamp_nanos();
        assert!((nanos2.saturating_sub(nanos)) < 1_000_000); // Less than 1ms apart
    }

    #[test]
    fn test_format_duration() {
        assert_eq!(format_duration_ns(500), "500ns");
        assert_eq!(format_duration_ns(1500), "1.50μs");
        assert_eq!(format_duration_ns(1_500_000), "1.50ms");
        assert_eq!(format_duration_ns(1_500_000_000), "1.50s");
    }
}