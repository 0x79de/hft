//! Integration tests for the HFT trading system
//!
//! These tests verify end-to-end workflows across multiple components

use hft::*;
use order_book::{OrderBook, MatchResult};
use latency_profiler::{LatencyProfiler, profiler::MeasurementPoint};

use order_book::types::{Order, OrderType, Side, Price, Quantity};
use uuid::Uuid;
use std::sync::Arc;

#[tokio::test]
async fn test_end_to_end_order_processing() {
    // Initialize all components
    let order_book = Arc::new(OrderBook::new("BTCUSD".to_string()));
    let profiler = LatencyProfiler::new();
    let client_id = Uuid::new_v4();

    // Create and process orders
    let buy_order = Order::new(
        "BTCUSD".to_string(),
        Side::Buy,
        OrderType::Limit,
        Price::new(50000.0),
        Quantity::new(1.0),
        client_id,
    );

    let sell_order = Order::new(
        "BTCUSD".to_string(),
        Side::Sell,
        OrderType::Limit,
        Price::new(50000.0),
        Quantity::new(1.0),
        client_id,
    );

    // Measure latency of order processing
    let start = profiler.start_measurement(MeasurementPoint::OrderReceived);
    
    // Add first order (should not match)
    let result1 = order_book.add_order(buy_order);
    assert!(matches!(result1, MatchResult::NoMatch));
    
    // Add matching order (should match)
    let result2 = order_book.add_order(sell_order);
    
    profiler.end_measurement(start);

    // Verify matching occurred
    match result2 {
        MatchResult::FullMatch { trades } => {
            assert_eq!(trades.len(), 1);
            assert_eq!(trades[0].price, Price::new(50000.0));
            assert_eq!(trades[0].quantity, Quantity::new(1.0));
        },
        _ => panic!("Expected full match"),
    }

    // Verify order book state
    assert_eq!(order_book.best_bid(), None);
    assert_eq!(order_book.best_ask(), None);

    // Check latency metrics
    let metrics = profiler.get_metrics(MeasurementPoint::OrderReceived).unwrap();
    assert!(metrics.count() > 0);
    assert!(metrics.mean().as_nanos() < 1_000_000); // Should be under 1ms
}

#[test]
fn test_basic_integration() {
    // Simple integration test that just verifies components can work together
    let order_book = OrderBook::new("BTCUSD".to_string());
    let profiler = LatencyProfiler::new();
    let client_id = Uuid::new_v4();
    
    // Test that we can measure order book operations
    let start = profiler.start_measurement(MeasurementPoint::OrderReceived);
    
    let order = Order::new(
        "BTCUSD".to_string(),
        Side::Buy,
        OrderType::Limit,
        Price::new(50000.0),
        Quantity::new(1.0),
        client_id,
    );
    
    let _result = order_book.add_order(order);
    profiler.end_measurement(start);
    
    // Verify metrics were recorded
    let metrics = profiler.get_metrics(MeasurementPoint::OrderReceived);
    assert!(metrics.is_some());
}

#[tokio::test]
async fn test_concurrent_order_processing() {
    use std::sync::atomic::{AtomicU32, Ordering};
    use tokio::task;
    
    let order_book = Arc::new(OrderBook::new("BTCUSD".to_string()));
    let processed_count = Arc::new(AtomicU32::new(0));
    
    let mut handles = Vec::new();
    
    // Spawn multiple tasks to process orders concurrently
    for i in 0..10 {
        let book = order_book.clone();
        let counter = processed_count.clone();
        
        let handle = task::spawn(async move {
            let client_id = Uuid::new_v4();
            
            // Create alternating buy/sell orders
            let side = if i % 2 == 0 { Side::Buy } else { Side::Sell };
            let price = if side == Side::Buy { 49900.0 + i as f64 } else { 50100.0 + i as f64 };
            
            let order = Order::new(
                "BTCUSD".to_string(),
                side,
                OrderType::Limit,
                Price::new(price),
                Quantity::new(1.0),
                client_id,
            );
            
            let _result = book.add_order(order);
            counter.fetch_add(1, Ordering::Relaxed);
        });
        
        handles.push(handle);
    }
    
    // Wait for all tasks to complete
    for handle in handles {
        handle.await.expect("Task should complete successfully");
    }
    
    // Verify all orders were processed
    assert_eq!(processed_count.load(Ordering::Relaxed), 10);
    
    // Verify order book has orders
    let total_volume = order_book.total_volume(Side::Buy) + order_book.total_volume(Side::Sell);
    assert!(total_volume > Quantity::ZERO);
}

#[test]
fn test_latency_profiling_under_load() {
    let profiler = LatencyProfiler::new();
    let order_book = Arc::new(OrderBook::new("BTCUSD".to_string()));
    
    // Process many orders while measuring latency
    for i in 0..1000 {
        let client_id = Uuid::new_v4();
        let start = profiler.start_measurement(MeasurementPoint::OrderReceived);
        
        let order = Order::new(
            "BTCUSD".to_string(),
            Side::Buy,
            OrderType::Limit,
            Price::new(50000.0 + (i % 100) as f64),
            Quantity::new(1.0),
            client_id,
        );
        
        let _result = order_book.add_order(order);
        profiler.end_measurement(start);
    }
    
    // Verify latency statistics
    let metrics = profiler.get_metrics(MeasurementPoint::OrderReceived).unwrap();
    assert_eq!(metrics.count(), 1000);
    assert!(metrics.min().as_nanos() > 0);
    assert!(metrics.max() > metrics.min());
    assert!(metrics.mean().as_nanos() > 0);
    
    // In an HFT system, we'd want sub-microsecond latencies
    println!("Average latency: {}ns", metrics.mean().as_nanos());
    println!("Min latency: {}ns", metrics.min().as_nanos());
    println!("Max latency: {}ns", metrics.max().as_nanos());
}

#[tokio::test]
async fn test_system_recovery_after_error() {
    let order_book = Arc::new(OrderBook::new("BTCUSD".to_string()));
    let client_id = Uuid::new_v4();
    
    // Add a normal order
    let order1 = Order::new(
        "BTCUSD".to_string(),
        Side::Buy,
        OrderType::Limit,
        Price::new(50000.0),
        Quantity::new(1.0),
        client_id,
    );
    
    let result1 = order_book.add_order(order1);
    assert!(matches!(result1, MatchResult::NoMatch));
    
    // Try to cancel a non-existent order (error condition)
    let fake_order_id = order_book::types::OrderId::from_raw(99999);
    let cancel_result = order_book.cancel_order(fake_order_id);
    assert!(cancel_result.is_none());
    
    // Verify system still works after error
    let order2 = Order::new(
        "BTCUSD".to_string(),
        Side::Sell,
        OrderType::Limit,
        Price::new(50000.0),
        Quantity::new(1.0),
        client_id,
    );
    
    let result2 = order_book.add_order(order2);
    assert!(matches!(result2, MatchResult::FullMatch { .. }));
}

#[tokio::test]
async fn test_memory_efficiency() {
    let order_book = Arc::new(OrderBook::new("BTCUSD".to_string()));
    let client_id = Uuid::new_v4();
    
    // Add many orders to test memory efficiency
    let mut order_ids = Vec::new();
    
    for i in 0..10000 {
        let order = Order::new(
            "BTCUSD".to_string(),
            Side::Buy,
            OrderType::Limit,
            Price::new(50000.0 - i as f64),
            Quantity::new(1.0),
            client_id,
        );
        
        let order_id = order.id;
        order_ids.push(order_id);
        
        let _result = order_book.add_order(order);
    }
    
    // Cancel half the orders to test cleanup
    for (i, &order_id) in order_ids.iter().enumerate() {
        if i % 2 == 0 {
            let _cancelled = order_book.cancel_order(order_id);
        }
    }
    
    // Verify remaining orders
    let remaining_volume = order_book.total_volume(Side::Buy);
    assert_eq!(remaining_volume, Quantity::new(5000.0)); // Half should remain
}

#[test]
fn test_property_based_price_arithmetic() {
    use proptest::prelude::*;
    
    proptest!(|(a: f64, b: f64)| {
        // Only test with reasonable values to avoid overflow
        prop_assume!(a > -1000000.0 && a < 1000000.0);
        prop_assume!(b > -1000000.0 && b < 1000000.0);
        prop_assume!(a.is_finite() && b.is_finite());
        
        let price_a = Price::new(a);
        let price_b = Price::new(b);
        
        // Test commutativity of addition
        let sum1 = price_a + price_b;
        let sum2 = price_b + price_a;
        prop_assert_eq!(sum1, sum2);
        
        // Test associativity with zero
        let zero = Price::ZERO;
        prop_assert_eq!(price_a + zero, price_a);
        prop_assert_eq!(zero + price_a, price_a);
    });
}

#[test]
fn test_property_based_order_matching() {
    use proptest::prelude::*;
    
    proptest!(|(buy_price in 1.0f64..100000.0, sell_price in 1.0f64..100000.0, quantity in 0.1f64..1000.0)| {
        prop_assume!(buy_price.is_finite() && sell_price.is_finite() && quantity.is_finite());
        
        let order_book = OrderBook::new("TEST".to_string());
        let client_id = Uuid::new_v4();
        
        let sell_order = Order::new(
            "TEST".to_string(),
            Side::Sell,
            OrderType::Limit,
            Price::new(sell_price),
            Quantity::new(quantity),
            client_id,
        );
        
        let buy_order = Order::new(
            "TEST".to_string(),
            Side::Buy,
            OrderType::Limit,
            Price::new(buy_price),
            Quantity::new(quantity),
            client_id,
        );
        
        // Add sell order first
        let result1 = order_book.add_order(sell_order);
        prop_assert!(matches!(result1, MatchResult::NoMatch));
        
        // Add buy order
        let result2 = order_book.add_order(buy_order);
        
        // If buy price >= sell price, orders should match
        if buy_price >= sell_price {
            match result2 {
                MatchResult::FullMatch { .. } => {},
                _ => prop_assert!(false, "Expected FullMatch"),
            }
        } else {
            prop_assert!(matches!(result2, MatchResult::NoMatch));
        }
    });
}