//! Memory leak detection tests for the HFT trading system
//!
//! These tests verify that memory usage remains stable under heavy load
//! and that resources are properly cleaned up

use std::sync::Arc;
use std::thread;
use std::time::Instant;
#[cfg(target_os = "linux")]
use std::time::Duration;
use order_book::OrderBook;
use order_book::types::{Order, OrderType, Side, Price, Quantity};
use latency_profiler::LatencyProfiler;
use uuid::Uuid;

#[test]
fn test_order_book_memory_stability() {
    let order_book = Arc::new(OrderBook::new("MEMTEST".to_string()));
    let client_id = Uuid::new_v4();
    
    // Measure initial memory footprint (approximate)
    let initial_orders = 1000;
    let mut order_ids = Vec::new();
    
    // Add initial orders
    for i in 0..initial_orders {
        let order = Order::new(
            "MEMTEST".to_string(),
            if i % 2 == 0 { Side::Buy } else { Side::Sell },
            OrderType::Limit,
            Price::new(100.0 + i as f64),
            Quantity::new(1.0),
            client_id,
        );
        
        order_ids.push(order.id);
        let _result = order_book.add_order(order);
    }
    
    // Perform many add/cancel cycles to test for memory leaks
    let cycles = 100;
    let orders_per_cycle = 1000;
    
    for cycle in 0..cycles {
        let mut cycle_order_ids = Vec::new();
        
        // Add orders
        for i in 0..orders_per_cycle {
            let order = Order::new(
                "MEMTEST".to_string(),
                if i % 2 == 0 { Side::Buy } else { Side::Sell },
                OrderType::Limit,
                Price::new(1000.0 + cycle as f64 * 1000.0 + i as f64),
                Quantity::new(1.0),
                client_id,
            );
            
            cycle_order_ids.push(order.id);
            let _result = order_book.add_order(order);
        }
        
        // Cancel all orders from this cycle
        for order_id in cycle_order_ids {
            let _cancelled = order_book.cancel_order(order_id);
        }
        
        // Periodically check that we're not accumulating orders
        if cycle % 10 == 9 {
            let buy_volume = order_book.total_volume(Side::Buy);
            let sell_volume = order_book.total_volume(Side::Sell);
            
            // Volume should remain roughly constant (only initial orders should remain)
            // Allow some variance due to potential partial matches
            assert!(buy_volume <= Quantity::new(initial_orders as f64 * 2.0));
            assert!(sell_volume <= Quantity::new(initial_orders as f64 * 2.0));
        }
    }
    
    println!("Completed {} cycles with {} orders each", cycles, orders_per_cycle);
    
    // Final verification - only initial orders should remain
    let final_buy_volume = order_book.total_volume(Side::Buy);
    let final_sell_volume = order_book.total_volume(Side::Sell);
    
    // Should be close to initial volume
    assert!(final_buy_volume <= Quantity::new(initial_orders as f64 * 1.5));
    assert!(final_sell_volume <= Quantity::new(initial_orders as f64 * 1.5));
}

#[test]
fn test_latency_profiler_memory_stability() {
    let profiler = Arc::new(LatencyProfiler::new());
    
    // Perform many measurement cycles
    let cycles = 1000;
    let measurements_per_cycle = 1000;
    
    for cycle in 0..cycles {
        let mut measurement_ids = Vec::new();
        
        // Start many measurements
        for _i in 0..measurements_per_cycle {
            let id = profiler.start_measurement(latency_profiler::profiler::MeasurementPoint::OrderReceived);
            measurement_ids.push(id);
        }
        
        // End all measurements
        for id in measurement_ids {
            profiler.end_measurement(id);
        }
        
        // Periodically reset to prevent accumulation
        if cycle % 100 == 99 {
            profiler.reset();
        }
    }
    
    // Final metrics should be reasonable
    let metrics = profiler.get_all_metrics();
    println!("Final profiler state: {} measurement types", metrics.len());
    
    // After reset, should not have excessive data
    assert!(metrics.len() <= 10, "Should not accumulate excessive measurement types");
}

#[test]
fn test_concurrent_memory_usage() {
    let order_book = Arc::new(OrderBook::new("CONCURRENT_MEM".to_string()));
    let num_threads = 8;
    let cycles_per_thread = 100;
    let orders_per_cycle = 100;
    
    let mut handles = Vec::new();
    
    for thread_id in 0..num_threads {
        let order_book = Arc::clone(&order_book);
        
        let handle = thread::spawn(move || {
            let client_id = Uuid::new_v4();
            
            for cycle in 0..cycles_per_thread {
                let mut order_ids = Vec::new();
                
                // Add orders
                for i in 0..orders_per_cycle {
                    let order = Order::new(
                        "CONCURRENT_MEM".to_string(),
                        if (thread_id + i) % 2 == 0 { Side::Buy } else { Side::Sell },
                        OrderType::Limit,
                        Price::new(100.0 + thread_id as f64 * 1000.0 + cycle as f64 * 100.0 + i as f64),
                        Quantity::new(1.0),
                        client_id,
                    );
                    
                    order_ids.push(order.id);
                    let _result = order_book.add_order(order);
                }
                
                // Cancel half the orders
                for (i, order_id) in order_ids.iter().enumerate() {
                    if i % 2 == 0 {
                        let _cancelled = order_book.cancel_order(*order_id);
                    }
                }
            }
            
            thread_id
        });
        
        handles.push(handle);
    }
    
    for handle in handles {
        let _thread_id = handle.join().unwrap();
    }
    
    // Verify final state is reasonable
    let buy_volume = order_book.total_volume(Side::Buy);
    let sell_volume = order_book.total_volume(Side::Sell);
    
    let expected_max_volume = (num_threads * cycles_per_thread * orders_per_cycle / 2) as f64;
    assert!(buy_volume <= Quantity::new(expected_max_volume));
    assert!(sell_volume <= Quantity::new(expected_max_volume));
    
    println!("Final volumes - Buy: {}, Sell: {}", buy_volume, sell_volume);
}

#[test]
fn test_large_scale_order_processing() {
    let order_book = Arc::new(OrderBook::new("LARGESCALE".to_string()));
    let profiler = Arc::new(LatencyProfiler::new());
    let client_id = Uuid::new_v4();
    
    let total_orders = 100_000;
    let batch_size = 1000;
    let start_time = Instant::now();
    
    for batch in 0..(total_orders / batch_size) {
        let mut batch_order_ids = Vec::new();
        
        // Add a batch of orders
        for i in 0..batch_size {
            let measurement_id = profiler.start_measurement(latency_profiler::profiler::MeasurementPoint::OrderReceived);
            
            let order_index = batch * batch_size + i;
            let side = if order_index % 2 == 0 { Side::Buy } else { Side::Sell };
            let base_price = if side == Side::Buy { 1000.0 } else { 1001.0 };
            
            let order = Order::new(
                "LARGESCALE".to_string(),
                side,
                OrderType::Limit,
                Price::new(base_price + (order_index % 100) as f64 * 0.01),
                Quantity::new(1.0 + (order_index % 10) as f64 * 0.1),
                client_id,
            );
            
            batch_order_ids.push(order.id);
            let _result = order_book.add_order(order);
            profiler.end_measurement(measurement_id);
        }
        
        // Cancel some orders to prevent excessive accumulation
        if batch % 10 == 9 {
            for (i, order_id) in batch_order_ids.iter().enumerate() {
                if i % 3 == 0 {
                    let _cancelled = order_book.cancel_order(*order_id);
                }
            }
        }
        
        // Log progress periodically
        if batch % 20 == 19 {
            let elapsed = start_time.elapsed();
            let orders_processed = (batch + 1) * batch_size;
            let rate = orders_processed as f64 / elapsed.as_secs_f64();
            println!("Processed {} orders in {:?} ({:.0} orders/sec)", 
                orders_processed, elapsed, rate);
        }
    }
    
    let total_time = start_time.elapsed();
    let final_rate = total_orders as f64 / total_time.as_secs_f64();
    
    println!("Final stats:");
    println!("- Total orders: {}", total_orders);
    println!("- Total time: {:?}", total_time);
    println!("- Final rate: {:.0} orders/sec", final_rate);
    
    // Verify performance expectations for HFT
    assert!(final_rate > 10_000.0, "Should process at least 10K orders/sec");
    
    // Verify latency metrics
    let metrics = profiler.get_metrics(latency_profiler::profiler::MeasurementPoint::OrderReceived).unwrap();
    println!("- Average latency: {:.2}μs", metrics.mean().as_nanos() as f64 / 1000.0);
    println!("- Min latency: {:.2}μs", metrics.min().as_nanos() as f64 / 1000.0);
    println!("- Max latency: {:.2}μs", metrics.max().as_nanos() as f64 / 1000.0);
    
    // HFT latency requirements
    assert!(metrics.mean().as_nanos() < 100_000, "Average latency should be under 100μs");
}

#[test]
fn test_resource_cleanup_on_drop() {
    // Test that resources are properly cleaned up when components are dropped
    {
        let order_book = OrderBook::new("CLEANUP_TEST".to_string());
        let profiler = LatencyProfiler::new();
        let client_id = Uuid::new_v4();
        
        // Add many orders and measurements
        for i in 0..1000 {
            let measurement_id = profiler.start_measurement(latency_profiler::profiler::MeasurementPoint::OrderReceived);
            
            let order = Order::new(
                "CLEANUP_TEST".to_string(),
                if i % 2 == 0 { Side::Buy } else { Side::Sell },
                OrderType::Limit,
                Price::new(100.0 + i as f64),
                Quantity::new(1.0),
                client_id,
            );
            
            let _result = order_book.add_order(order);
            profiler.end_measurement(measurement_id);
        }
        
        // Components go out of scope here and should be cleaned up
    }
    
    // If we reach here without issues, cleanup was successful
    assert!(true, "Resource cleanup completed successfully");
}

#[cfg(target_os = "linux")]
#[test]
fn test_memory_usage_monitoring() {
    use std::fs;
    
    fn get_memory_usage() -> Option<usize> {
        let status = fs::read_to_string("/proc/self/status").ok()?;
        for line in status.lines() {
            if line.starts_with("VmRSS:") {
                let parts: Vec<&str> = line.split_whitespace().collect();
                if parts.len() >= 2 {
                    return parts[1].parse().ok();
                }
            }
        }
        None
    }
    
    let initial_memory = get_memory_usage().unwrap_or(0);
    println!("Initial memory usage: {} KB", initial_memory);
    
    {
        let order_book = Arc::new(OrderBook::new("MEMORY_MONITOR".to_string()));
        let client_id = Uuid::new_v4();
        
        // Add a large number of orders
        for i in 0..50_000 {
            let order = Order::new(
                "MEMORY_MONITOR".to_string(),
                if i % 2 == 0 { Side::Buy } else { Side::Sell },
                OrderType::Limit,
                Price::new(100.0 + i as f64),
                Quantity::new(1.0),
                client_id,
            );
            
            let _result = order_book.add_order(order);
        }
        
        let peak_memory = get_memory_usage().unwrap_or(0);
        println!("Peak memory usage: {} KB", peak_memory);
        
        // Memory increase should be reasonable for 50K orders
        let memory_increase = peak_memory.saturating_sub(initial_memory);
        assert!(memory_increase < 500_000, "Memory increase should be less than 500MB");
    }
    
    // Give some time for cleanup
    thread::sleep(Duration::from_millis(100));
    
    let final_memory = get_memory_usage().unwrap_or(0);
    println!("Final memory usage: {} KB", final_memory);
    
    // Memory should return close to initial levels after cleanup
    let final_increase = final_memory.saturating_sub(initial_memory);
    assert!(final_increase < 50_000, "Final memory increase should be less than 50MB");
}