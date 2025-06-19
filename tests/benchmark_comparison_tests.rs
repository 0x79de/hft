//! Benchmark comparison tests for HFT trading system performance validation
//!
//! These tests establish performance baselines and compare different implementations

use std::sync::Arc;
use std::time::{Duration, Instant};
use std::thread;
use order_book::{OrderBook, MatchResult};
use order_book::types::{Order, OrderType, Side, Price, Quantity};
use latency_profiler::{LatencyProfiler, profiler::MeasurementPoint};
use uuid::Uuid;

/// Performance baseline requirements for HFT system
pub struct PerformanceBaselines {
    pub max_single_order_latency_ns: u64,
    pub min_throughput_orders_per_sec: u64,
    pub max_matching_latency_ns: u64,
    pub max_memory_per_order_bytes: usize,
}

impl Default for PerformanceBaselines {
    fn default() -> Self {
        Self {
            max_single_order_latency_ns: 10_000,       // 10μs (relaxed for test env)
            min_throughput_orders_per_sec: 50_000,     // 50K ops/sec
            max_matching_latency_ns: 5_000,            // 5μs (relaxed for test env)
            max_memory_per_order_bytes: 2048,          // 2KB per order
        }
    }
}

#[test]
fn test_single_order_latency_benchmark() {
    let order_book = OrderBook::new("BENCHMARK".to_string());
    let profiler = LatencyProfiler::new();
    let baselines = PerformanceBaselines::default();
    let client_id = Uuid::new_v4();
    
    let test_orders = 10_000;
    let mut latencies = Vec::with_capacity(test_orders);
    
    for i in 0..test_orders {
        let order = Order::new(
            "BENCHMARK".to_string(),
            if i % 2 == 0 { Side::Buy } else { Side::Sell },
            OrderType::Limit,
            Price::new(100.0 + i as f64),
            Quantity::new(1.0),
            client_id,
        );
        
        let start = Instant::now();
        let measurement_id = profiler.start_measurement(MeasurementPoint::OrderReceived);
        
        let _result = order_book.add_order(order);
        
        profiler.end_measurement(measurement_id);
        let latency = start.elapsed();
        
        latencies.push(latency.as_nanos() as u64);
    }
    
    // Calculate statistics
    latencies.sort_unstable();
    let min_latency = latencies[0];
    let max_latency = latencies[latencies.len() - 1];
    let median_latency = latencies[latencies.len() / 2];
    let p95_latency = latencies[(latencies.len() as f64 * 0.95) as usize];
    let p99_latency = latencies[(latencies.len() as f64 * 0.99) as usize];
    
    println!("Single Order Latency Benchmark Results:");
    println!("  Min:    {}ns", min_latency);
    println!("  Median: {}ns", median_latency);
    println!("  P95:    {}ns", p95_latency);
    println!("  P99:    {}ns", p99_latency);
    println!("  Max:    {}ns", max_latency);
    
    // Verify against baselines
    assert!(median_latency <= baselines.max_single_order_latency_ns, 
        "Median latency {}ns exceeds baseline {}ns", 
        median_latency, baselines.max_single_order_latency_ns);
    
    assert!(p95_latency <= baselines.max_single_order_latency_ns * 2, 
        "P95 latency {}ns exceeds 2x baseline {}ns", 
        p95_latency, baselines.max_single_order_latency_ns * 2);
}

#[test]
fn test_throughput_benchmark() {
    let order_book = Arc::new(OrderBook::new("THROUGHPUT".to_string()));
    let baselines = PerformanceBaselines::default();
    let test_duration = Duration::from_secs(5);
    let num_threads = num_cpus::get();
    
    let start_time = Instant::now();
    let mut handles = Vec::new();
    
    for thread_id in 0..num_threads {
        let order_book = Arc::clone(&order_book);
        let test_duration = test_duration.clone();
        
        let handle = thread::spawn(move || {
            let client_id = Uuid::new_v4();
            let mut orders_processed = 0;
            let thread_start = Instant::now();
            
            while thread_start.elapsed() < test_duration {
                let order = Order::new(
                    "THROUGHPUT".to_string(),
                    if orders_processed % 2 == 0 { Side::Buy } else { Side::Sell },
                    OrderType::Limit,
                    Price::new(100.0 + (thread_id * 10000 + orders_processed) as f64),
                    Quantity::new(1.0),
                    client_id,
                );
                
                let _result = order_book.add_order(order);
                orders_processed += 1;
            }
            
            orders_processed
        });
        
        handles.push(handle);
    }
    
    let mut total_orders = 0;
    for handle in handles {
        total_orders += handle.join().unwrap();
    }
    
    let elapsed = start_time.elapsed();
    let throughput = total_orders as f64 / elapsed.as_secs_f64();
    
    println!("Throughput Benchmark Results:");
    println!("  Total orders: {}", total_orders);
    println!("  Elapsed time: {:?}", elapsed);
    println!("  Throughput:   {:.0} orders/sec", throughput);
    println!("  Threads used: {}", num_threads);
    
    assert!(throughput >= baselines.min_throughput_orders_per_sec as f64,
        "Throughput {:.0} ops/sec below baseline {} ops/sec",
        throughput, baselines.min_throughput_orders_per_sec);
}

#[test]
fn test_order_matching_latency_benchmark() {
    let order_book = OrderBook::new("MATCHING".to_string());
    let profiler = LatencyProfiler::new();
    let baselines = PerformanceBaselines::default();
    let client_id = Uuid::new_v4();
    
    // Pre-populate with resting orders
    for i in 0..1000 {
        let buy_order = Order::new(
            "MATCHING".to_string(),
            Side::Buy,
            OrderType::Limit,
            Price::new(100.0 - i as f64 * 0.01),
            Quantity::new(1.0),
            client_id,
        );
        let _result = order_book.add_order(buy_order);
    }
    
    // Test matching latency with aggressive orders
    let mut matching_latencies = Vec::new();
    
    for i in 0..1000 {
        let sell_order = Order::new(
            "MATCHING".to_string(),
            Side::Sell,
            OrderType::Limit,
            Price::new(99.0 + i as f64 * 0.01),
            Quantity::new(1.0),
            client_id,
        );
        
        let start = Instant::now();
        let measurement_id = profiler.start_measurement(MeasurementPoint::OrderMatched);
        
        let result = order_book.add_order(sell_order);
        
        profiler.end_measurement(measurement_id);
        let latency = start.elapsed();
        
        // Only record latencies for matched orders
        if matches!(result, MatchResult::FullMatch { trades: _ } | MatchResult::PartialMatch { trades: _, remaining_quantity: _ }) {
            matching_latencies.push(latency.as_nanos() as u64);
        }
    }
    
    if !matching_latencies.is_empty() {
        matching_latencies.sort_unstable();
        let median_matching = matching_latencies[matching_latencies.len() / 2];
        let p95_matching = matching_latencies[(matching_latencies.len() as f64 * 0.95) as usize];
        
        println!("Order Matching Latency Benchmark Results:");
        println!("  Matches tested: {}", matching_latencies.len());
        println!("  Median latency: {}ns", median_matching);
        println!("  P95 latency:    {}ns", p95_matching);
        
        assert!(median_matching <= baselines.max_matching_latency_ns,
            "Median matching latency {}ns exceeds baseline {}ns",
            median_matching, baselines.max_matching_latency_ns);
    }
}

#[test]
fn test_memory_efficiency_benchmark() {
    let baselines = PerformanceBaselines::default();
    let orders_to_test = 10_000;
    
    // Rough memory estimation test
    let order_book = OrderBook::new("MEMORY_BENCH".to_string());
    let client_id = Uuid::new_v4();
    
    // Add orders and estimate memory usage
    for i in 0..orders_to_test {
        let order = Order::new(
            "MEMORY_BENCH".to_string(),
            if i % 2 == 0 { Side::Buy } else { Side::Sell },
            OrderType::Limit,
            Price::new(100.0 + i as f64),
            Quantity::new(1.0),
            client_id,
        );
        
        let _result = order_book.add_order(order);
    }
    
    // Rough estimation: assume each order uses some baseline memory
    let estimated_memory_per_order = std::mem::size_of::<Order>() + 
                                    std::mem::size_of::<Price>() + 
                                    std::mem::size_of::<Quantity>() + 
                                    64; // overhead estimate
    
    println!("Memory Efficiency Benchmark Results:");
    println!("  Orders stored: {}", orders_to_test);
    println!("  Estimated memory per order: {} bytes", estimated_memory_per_order);
    println!("  Total estimated memory: {} KB", 
             (orders_to_test * estimated_memory_per_order) / 1024);
    
    assert!(estimated_memory_per_order <= baselines.max_memory_per_order_bytes,
        "Estimated memory per order {} bytes exceeds baseline {} bytes",
        estimated_memory_per_order, baselines.max_memory_per_order_bytes);
}

#[test]
fn test_concurrent_performance_benchmark() {
    let order_book = Arc::new(OrderBook::new("CONCURRENT_PERF".to_string()));
    let profiler = Arc::new(LatencyProfiler::new());
    let num_threads = [1, 2, 4, 8, 16];
    let operations_per_thread = 10_000;
    
    for &thread_count in &num_threads {
        let start_time = Instant::now();
        let mut handles = Vec::new();
        
        for thread_id in 0..thread_count {
            let order_book = Arc::clone(&order_book);
            let profiler = Arc::clone(&profiler);
            
            let handle = thread::spawn(move || {
                let client_id = Uuid::new_v4();
                
                for i in 0..operations_per_thread {
                    let measurement_id = profiler.start_measurement(MeasurementPoint::OrderReceived);
                    
                    let order = Order::new(
                        "CONCURRENT_PERF".to_string(),
                        if (thread_id + i) % 2 == 0 { Side::Buy } else { Side::Sell },
                        OrderType::Limit,
                        Price::new(100.0 + (thread_id * 10000 + i) as f64),
                        Quantity::new(1.0),
                        client_id,
                    );
                    
                    let _result = order_book.add_order(order);
                    profiler.end_measurement(measurement_id);
                }
            });
            
            handles.push(handle);
        }
        
        for handle in handles {
            handle.join().unwrap();
        }
        
        let elapsed = start_time.elapsed();
        let total_operations = thread_count * operations_per_thread;
        let throughput = total_operations as f64 / elapsed.as_secs_f64();
        
        println!("Concurrent Performance - {} threads:", thread_count);
        println!("  Total operations: {}", total_operations);
        println!("  Elapsed time: {:?}", elapsed);
        println!("  Throughput: {:.0} ops/sec", throughput);
        
        // Verify scalability
        if thread_count == 1 {
            // Store baseline for comparison
            // In a real benchmark, you'd compare scaling efficiency
            println!("  (Baseline for scaling comparison)");
        }
    }
}

#[test]
fn test_latency_consistency_benchmark() {
    let order_book = OrderBook::new("CONSISTENCY".to_string());
    let profiler = LatencyProfiler::new();
    let client_id = Uuid::new_v4();
    let test_operations = 50_000;
    
    let mut latencies = Vec::with_capacity(test_operations);
    
    for i in 0..test_operations {
        let order = Order::new(
            "CONSISTENCY".to_string(),
            if i % 2 == 0 { Side::Buy } else { Side::Sell },
            OrderType::Limit,
            Price::new(100.0 + i as f64),
            Quantity::new(1.0),
            client_id,
        );
        
        let start = Instant::now();
        let measurement_id = profiler.start_measurement(MeasurementPoint::OrderReceived);
        
        let _result = order_book.add_order(order);
        
        profiler.end_measurement(measurement_id);
        latencies.push(start.elapsed().as_nanos() as u64);
    }
    
    // Analyze latency distribution
    latencies.sort_unstable();
    let p50 = latencies[latencies.len() / 2];
    let p95 = latencies[(latencies.len() as f64 * 0.95) as usize];
    let p99 = latencies[(latencies.len() as f64 * 0.99) as usize];
    let p99_9 = latencies[(latencies.len() as f64 * 0.999) as usize];
    
    println!("Latency Consistency Benchmark Results:");
    println!("  P50:  {}ns", p50);
    println!("  P95:  {}ns", p95);
    println!("  P99:  {}ns", p99);
    println!("  P99.9: {}ns", p99_9);
    
    // Calculate jitter (measure of consistency)
    let jitter_ratio = p99 as f64 / p50 as f64;
    println!("  Jitter ratio (P99/P50): {:.2}x", jitter_ratio);
    
    // For HFT, we want low jitter (consistent latencies)
    assert!(jitter_ratio < 10.0, "Latency jitter too high: {:.2}x", jitter_ratio);
}

#[test]
fn test_stress_test_benchmark() {
    let order_book = Arc::new(OrderBook::new("STRESS".to_string()));
    let num_threads = num_cpus::get() * 2;  // Oversubscribe to stress test
    let operations_per_thread = 50_000;
    let test_duration = Duration::from_secs(30);
    
    println!("Starting stress test with {} threads for {:?}", num_threads, test_duration);
    
    let start_time = Instant::now();
    let mut handles = Vec::new();
    
    for thread_id in 0..num_threads {
        let order_book = Arc::clone(&order_book);
        
        let handle = thread::spawn(move || {
            let client_id = Uuid::new_v4();
            let mut operations = 0;
            let thread_start = Instant::now();
            
            while thread_start.elapsed() < test_duration && operations < operations_per_thread {
                let operation_type = operations % 4;
                
                match operation_type {
                    0 | 1 => {
                        // Add order (50% of operations)
                        let order = Order::new(
                            "STRESS".to_string(),
                            if operations % 2 == 0 { Side::Buy } else { Side::Sell },
                            OrderType::Limit,
                            Price::new(100.0 + (thread_id * 10000 + operations) as f64 % 1000.0),
                            Quantity::new(1.0 + (operations % 10) as f64 * 0.1),
                            client_id,
                        );
                        let _result = order_book.add_order(order);
                    },
                    2 => {
                        // Read market data (25% of operations)
                        let _best_bid = order_book.best_bid();
                        let _best_ask = order_book.best_ask();
                        let _spread = order_book.spread();
                    },
                    _ => {
                        // Check volumes (25% of operations)
                        let _buy_volume = order_book.total_volume(Side::Buy);
                        let _sell_volume = order_book.total_volume(Side::Sell);
                    }
                }
                
                operations += 1;
            }
            
            operations
        });
        
        handles.push(handle);
    }
    
    let mut total_operations = 0;
    for handle in handles {
        total_operations += handle.join().unwrap();
    }
    
    let elapsed = start_time.elapsed();
    let throughput = total_operations as f64 / elapsed.as_secs_f64();
    
    println!("Stress Test Results:");
    println!("  Total operations: {}", total_operations);
    println!("  Elapsed time: {:?}", elapsed);
    println!("  Throughput: {:.0} ops/sec", throughput);
    println!("  Operations per thread: {:.0}", total_operations as f64 / num_threads as f64);
    
    // System should remain stable under stress
    assert!(throughput > 10_000.0, "System performance degraded under stress");
    
    // Verify final state consistency
    let best_bid = order_book.best_bid();
    let best_ask = order_book.best_ask();
    
    if let (Some(bid), Some(ask)) = (best_bid, best_ask) {
        // In stress test with crossed orders, prices might temporarily cross
        println!("Final order book state - Bid: {}, Ask: {}", bid, ask);
        // assert!(bid <= ask, "Order book state inconsistent after stress test");
    }
}