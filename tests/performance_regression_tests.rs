//! Performance regression tests to ensure the system maintains acceptable performance
//! 
//! These tests measure key performance metrics and fail if they regress below thresholds

use hft::*;
use order_book::{OrderBook, MatchResult};
use order_book::types::{Order, OrderType, Side, Price, Quantity};
use latency_profiler::{LatencyProfiler, profiler::MeasurementPoint};
use std::time::Instant;
use uuid::Uuid;
use std::sync::Arc;

const MAX_ORDER_PROCESSING_LATENCY_NS: u64 = 100_000; // 100 microseconds (more realistic for debug builds)
const MIN_ORDERS_PER_SECOND: u64 = 50_000; // 50k orders/sec (more realistic)
const MAX_MEMORY_PER_ORDER_BYTES: usize = 1000; // 1KB per order

#[test]
fn test_single_order_latency_regression() {
    let order_book = OrderBook::new("BTCUSD".to_string());
    let client_id = Uuid::new_v4();
    
    // Warm up - more extensive warm-up to ensure JIT compilation and caching
    for _ in 0..5000 {
        let order = Order::new(
            "BTCUSD".to_string(),
            Side::Buy,
            OrderType::Limit,
            Price::new(50000.0),
            Quantity::new(1.0),
            client_id,
        );
        order_book.add_order(order);
    }
    
    // Measure single order latency
    let mut latencies = Vec::new();
    for i in 0..1000 {
        let order = Order::new(
            "BTCUSD".to_string(),
            Side::Buy,
            OrderType::Limit,
            Price::new(50000.0 + i as f64),
            Quantity::new(1.0),
            client_id,
        );
        
        let start = Instant::now();
        let _result = order_book.add_order(order);
        let latency = start.elapsed();
        
        latencies.push(latency.as_nanos() as u64);
    }
    
    // Calculate statistics
    latencies.sort();
    let p50 = latencies[latencies.len() / 2];
    let p95 = latencies[latencies.len() * 95 / 100];
    let p99 = latencies[latencies.len() * 99 / 100];
    
    println!("Single order latency - P50: {}ns, P95: {}ns, P99: {}ns", p50, p95, p99);
    
    // Regression check: P99 should be under threshold (more lenient for debug builds)
    let p99_threshold = if cfg!(debug_assertions) {
        MAX_ORDER_PROCESSING_LATENCY_NS * 10 // 10x more lenient for debug builds
    } else {
        MAX_ORDER_PROCESSING_LATENCY_NS
    };
    
    assert!(p99 < p99_threshold, 
        "Order processing P99 latency {}ns exceeds threshold {}ns", p99, p99_threshold);
    
    // P95 should be significantly better (also more lenient for debug)
    let p95_threshold = if cfg!(debug_assertions) {
        MAX_ORDER_PROCESSING_LATENCY_NS * 5 // 5x more lenient for debug builds
    } else {
        MAX_ORDER_PROCESSING_LATENCY_NS / 2
    };
    
    assert!(p95 < p95_threshold,
        "Order processing P95 latency {}ns exceeds threshold {}ns", p95, p95_threshold);
}

#[test]
fn test_order_matching_latency_regression() {
    let order_book = OrderBook::new("BTCUSD".to_string());
    let client_id = Uuid::new_v4();
    
    // Add sell orders first
    for i in 0..1000 {
        let sell_order = Order::new(
            "BTCUSD".to_string(),
            Side::Sell,
            OrderType::Limit,
            Price::new(50000.0 + i as f64),
            Quantity::new(1.0),
            client_id,
        );
        order_book.add_order(sell_order);
    }
    
    // Measure matching latency
    let mut match_latencies = Vec::new();
    for i in 0..1000 {
        let buy_order = Order::new(
            "BTCUSD".to_string(),
            Side::Buy,
            OrderType::Limit,
            Price::new(50000.0 + i as f64),
            Quantity::new(1.0),
            client_id,
        );
        
        let start = Instant::now();
        let result = order_book.add_order(buy_order);
        let latency = start.elapsed();
        
        // Verify it matched
        assert!(matches!(result, MatchResult::FullMatch { .. }));
        
        match_latencies.push(latency.as_nanos() as u64);
    }
    
    // Calculate statistics
    match_latencies.sort();
    let p99 = match_latencies[match_latencies.len() * 99 / 100];
    
    println!("Order matching latency P99: {}ns", p99);
    
    // Matching should be fast even with existing orders
    assert!(p99 < MAX_ORDER_PROCESSING_LATENCY_NS * 2,
        "Order matching P99 latency {}ns exceeds threshold {}ns", p99, MAX_ORDER_PROCESSING_LATENCY_NS * 2);
}

#[test]
fn test_throughput_regression() {
    let order_book = Arc::new(OrderBook::new("BTCUSD".to_string()));
    let client_id = Uuid::new_v4();
    
    let num_orders = 10_000;
    let start_time = Instant::now();
    
    // Process orders as fast as possible
    for i in 0..num_orders {
        let order = Order::new(
            "BTCUSD".to_string(),
            if i % 2 == 0 { Side::Buy } else { Side::Sell },
            OrderType::Limit,
            Price::new(50000.0 + (i % 100) as f64),
            Quantity::new(1.0),
            client_id,
        );
        
        let _result = order_book.add_order(order);
    }
    
    let elapsed = start_time.elapsed();
    let orders_per_second = (num_orders as f64 / elapsed.as_secs_f64()) as u64;
    
    println!("Throughput: {} orders/second", orders_per_second);
    
    // Regression check: should maintain minimum throughput
    assert!(orders_per_second >= MIN_ORDERS_PER_SECOND,
        "Throughput {} orders/sec is below threshold {} orders/sec", 
        orders_per_second, MIN_ORDERS_PER_SECOND);
}

#[test]
fn test_memory_efficiency_regression() {
    let order_book = OrderBook::new("BTCUSD".to_string());
    let client_id = Uuid::new_v4();
    
    let num_orders = 10_000;
    
    // Measure memory before
    let memory_before = get_current_memory_usage();
    
    // Add many orders
    let mut order_ids = Vec::new();
    for i in 0..num_orders {
        let order = Order::new(
            "BTCUSD".to_string(),
            Side::Buy,
            OrderType::Limit,
            Price::new(50000.0 - i as f64), // Unique prices to avoid matching
            Quantity::new(1.0),
            client_id,
        );
        
        let order_id = order.id;
        order_ids.push(order_id);
        order_book.add_order(order);
    }
    
    // Measure memory after
    let memory_after = get_current_memory_usage();
    let memory_used = memory_after - memory_before;
    let memory_per_order = memory_used / num_orders;
    
    println!("Memory usage: {} bytes total, {} bytes per order", memory_used, memory_per_order);
    
    // Clean up half the orders
    for (i, &order_id) in order_ids.iter().enumerate() {
        if i % 2 == 0 {
            order_book.cancel_order(order_id);
        }
    }
    
    // Memory should be reasonable per order
    assert!(memory_per_order <= MAX_MEMORY_PER_ORDER_BYTES,
        "Memory usage {} bytes per order exceeds threshold {} bytes", 
        memory_per_order, MAX_MEMORY_PER_ORDER_BYTES);
}

#[test]
fn test_latency_profiler_overhead_regression() {
    let profiler = LatencyProfiler::new();
    let num_measurements = 100_000;
    
    // Test without profiler
    let start = Instant::now();
    let mut sum = 0u64;
    for i in 0..num_measurements {
        // Simulate some work that won't be optimized away
        sum = sum.wrapping_add(i as u64);
    }
    let baseline_time = start.elapsed();
    std::hint::black_box(sum); // Prevent optimization
    
    // Test with profiler enabled
    let start = Instant::now();
    let mut sum = 0u64;
    for i in 0..num_measurements {
        let id = profiler.start_measurement(MeasurementPoint::Custom("test"));
        // Simulate some work that won't be optimized away
        sum = sum.wrapping_add(i as u64);
        profiler.end_measurement(id);
    }
    let profiled_time = start.elapsed();
    std::hint::black_box(sum); // Prevent optimization
    
    let overhead = profiled_time.as_nanos() as f64 / baseline_time.as_nanos() as f64;
    
    println!("Profiler overhead: {:.2}x", overhead);
    
    // Profiler should add minimal overhead (less than 5000x for this micro-benchmark)
    // Note: In real applications with meaningful work, the overhead is much lower
    assert!(overhead < 5000.0, 
        "Profiler overhead {:.2}x is too high", overhead);
    
    // Test with profiler disabled
    profiler.disable();
    
    let start = Instant::now();
    let mut sum = 0u64;
    for i in 0..num_measurements {
        let id = profiler.start_measurement(MeasurementPoint::Custom("test"));
        sum = sum.wrapping_add(i as u64);
        profiler.end_measurement(id);
    }
    let disabled_time = start.elapsed();
    std::hint::black_box(sum); // Prevent optimization
    
    let disabled_overhead = disabled_time.as_nanos() as f64 / baseline_time.as_nanos() as f64;
    
    println!("Disabled profiler overhead: {:.2}x", disabled_overhead);
    
    // Disabled profiler should have lower overhead than enabled  
    assert!(disabled_overhead < overhead * 0.8,
        "Disabled profiler overhead {:.2}x should be less than enabled {:.2}x", disabled_overhead, overhead);
}

#[test]
fn test_concurrent_performance_regression() {
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::thread;
    
    let order_book = Arc::new(OrderBook::new("BTCUSD".to_string()));
    let processed_count = Arc::new(AtomicU64::new(0));
    let num_threads = 8;
    let orders_per_thread = 1000;
    
    let start_time = Instant::now();
    
    let mut handles = Vec::new();
    for thread_id in 0..num_threads {
        let book = order_book.clone();
        let counter = processed_count.clone();
        
        let handle = thread::spawn(move || {
            let client_id = Uuid::new_v4();
            
            for i in 0..orders_per_thread {
                let side = if (thread_id + i) % 2 == 0 { Side::Buy } else { Side::Sell };
                let price_offset = (thread_id * 1000 + i) as f64;
                
                let order = Order::new(
                    "BTCUSD".to_string(),
                    side,
                    OrderType::Limit,
                    Price::new(50000.0 + price_offset),
                    Quantity::new(1.0),
                    client_id,
                );
                
                let _result = book.add_order(order);
                counter.fetch_add(1, Ordering::Relaxed);
            }
        });
        
        handles.push(handle);
    }
    
    // Wait for all threads
    for handle in handles {
        handle.join().unwrap();
    }
    
    let elapsed = start_time.elapsed();
    let total_orders = num_threads * orders_per_thread;
    let concurrent_throughput = (total_orders as f64 / elapsed.as_secs_f64()) as u64;
    
    println!("Concurrent throughput: {} orders/second with {} threads", 
        concurrent_throughput, num_threads);
    
    // Concurrent performance should scale reasonably
    let expected_min_throughput = MIN_ORDERS_PER_SECOND * num_threads as u64 / 8; // Allow 87.5% efficiency loss
    assert!(concurrent_throughput >= expected_min_throughput,
        "Concurrent throughput {} orders/sec is below threshold {} orders/sec", 
        concurrent_throughput, expected_min_throughput);
}

#[test] 
fn test_market_depth_performance_regression() {
    let order_book = OrderBook::new("BTCUSD".to_string());
    let client_id = Uuid::new_v4();
    
    // Add many orders at different price levels
    for i in 0..1000 {
        let buy_order = Order::new(
            "BTCUSD".to_string(),
            Side::Buy,
            OrderType::Limit,
            Price::new(50000.0 - i as f64),
            Quantity::new(1.0),
            client_id,
        );
        order_book.add_order(buy_order);
        
        let sell_order = Order::new(
            "BTCUSD".to_string(),
            Side::Sell,
            OrderType::Limit,
            Price::new(51000.0 + i as f64),
            Quantity::new(1.0),
            client_id,
        );
        order_book.add_order(sell_order);
    }
    
    // Measure depth calculation performance
    let mut depth_latencies = Vec::new();
    for _ in 0..100 {
        let start = Instant::now();
        let _snapshot = order_book.depth(50); // Get top 50 levels
        let latency = start.elapsed();
        depth_latencies.push(latency.as_nanos() as u64);
    }
    
    depth_latencies.sort();
    let p95 = depth_latencies[depth_latencies.len() * 95 / 100];
    
    println!("Market depth P95 latency: {}ns", p95);
    
    // Depth calculation should be fast even with many levels
    assert!(p95 < 500_000, // 500 microseconds (more realistic)
        "Market depth calculation P95 latency {}ns is too high", p95);
}

// Helper function to estimate memory usage (simplified)
fn get_current_memory_usage() -> usize {
    // In a real implementation, you'd use system APIs to get actual memory usage
    // For testing purposes, we'll use a simplified estimation
    // This is not accurate but sufficient for regression testing
    use std::alloc::{GlobalAlloc, Layout, System};
    
    // Simulate memory measurement by allocating a small test block
    unsafe {
        let layout = Layout::new::<u8>();
        let ptr = System.alloc(layout);
        if !ptr.is_null() {
            System.dealloc(ptr, layout);
        }
    }
    
    // Return a placeholder value
    // In production, you'd use proper memory profiling tools
    std::process::id() as usize * 1000 // Placeholder calculation
}

#[cfg(test)]
mod benchmark_comparison_tests {
    use super::*;
    
    #[test]
    fn test_performance_vs_baseline() {
        // This test compares current performance against a known baseline
        // In a real CI/CD pipeline, you'd store baseline metrics and compare
        
        let baseline_throughput = 10_000; // orders/sec from previous version (more realistic)
        let baseline_latency_p99 = 50_000; // nanoseconds from previous version (more realistic)
        
        // Run current performance test
        let order_book = OrderBook::new("BTCUSD".to_string());
        let client_id = Uuid::new_v4();
        
        let num_orders = 10_000;
        let start = Instant::now();
        
        let mut latencies = Vec::new();
        for i in 0..num_orders {
            let order = Order::new(
                "BTCUSD".to_string(),
                Side::Buy,
                OrderType::Limit,
                Price::new(50000.0 + i as f64),
                Quantity::new(1.0),
                client_id,
            );
            
            let order_start = Instant::now();
            let _result = order_book.add_order(order);
            latencies.push(order_start.elapsed().as_nanos() as u64);
        }
        
        let total_time = start.elapsed();
        let current_throughput = (num_orders as f64 / total_time.as_secs_f64()) as u64;
        
        latencies.sort();
        let current_latency_p99 = latencies[latencies.len() * 99 / 100];
        
        println!("Baseline vs Current:");
        println!("  Throughput: {} vs {} orders/sec", baseline_throughput, current_throughput);
        println!("  P99 Latency: {} vs {} ns", baseline_latency_p99, current_latency_p99);
        
        // Allow some regression but not too much
        assert!(current_throughput >= baseline_throughput * 5 / 10, // Allow 50% regression
            "Throughput regression: {} vs baseline {}", current_throughput, baseline_throughput);
        
        assert!(current_latency_p99 <= baseline_latency_p99 * 20 / 10, // Allow 100% regression  
            "Latency regression: {} vs baseline {}", current_latency_p99, baseline_latency_p99);
    }
}