use order_book::{Order, OrderId, Trade, Price, Quantity};
use serde::{Deserialize, Serialize};
use chrono::{DateTime, Utc};
use uuid::Uuid;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum OrderEvent {
    AddOrder(Order),
    CancelOrder {
        order_id: OrderId,
        symbol: String,
        client_id: Uuid,
        timestamp: DateTime<Utc>,
    },
    ModifyOrder {
        order_id: OrderId,
        new_price: Option<Price>,
        new_quantity: Option<Quantity>,
        timestamp: DateTime<Utc>,
    },
    OrderFilled {
        order_id: OrderId,
        fill_quantity: Quantity,
        fill_price: Price,
        timestamp: DateTime<Utc>,
    },
    OrderRejected {
        order_id: OrderId,
        reason: String,
        timestamp: DateTime<Utc>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum TradeEvent {
    TradeExecuted(Trade),
    TradeSettlement {
        trade_id: u64,
        settlement_status: SettlementStatus,
        timestamp: DateTime<Utc>,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[repr(u8)]
pub enum SettlementStatus {
    Pending = 0,
    Settled = 1,
    Failed = 2,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum SystemEvent {
    MarketOpen {
        symbol: String,
        timestamp: DateTime<Utc>,
    },
    MarketClose {
        symbol: String,
        timestamp: DateTime<Utc>,
    },
    TradingHalt {
        symbol: String,
        reason: String,
        timestamp: DateTime<Utc>,
    },
    TradingResume {
        symbol: String,
        timestamp: DateTime<Utc>,
    },
    SystemHealthCheck {
        component: String,
        status: HealthStatus,
        timestamp: DateTime<Utc>,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[repr(u8)]
pub enum HealthStatus {
    Healthy = 0,
    Warning = 1,
    Critical = 2,
    Down = 3,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Event {
    Order(OrderEvent),
    Trade(TradeEvent),
    System(SystemEvent),
}

impl Event {
    #[inline]
    pub fn timestamp(&self) -> DateTime<Utc> {
        match self {
            Event::Order(order_event) => match order_event {
                OrderEvent::AddOrder(order) => order.timestamp,
                OrderEvent::CancelOrder { timestamp, .. } => *timestamp,
                OrderEvent::ModifyOrder { timestamp, .. } => *timestamp,
                OrderEvent::OrderFilled { timestamp, .. } => *timestamp,
                OrderEvent::OrderRejected { timestamp, .. } => *timestamp,
            },
            Event::Trade(trade_event) => match trade_event {
                TradeEvent::TradeExecuted(trade) => trade.timestamp,
                TradeEvent::TradeSettlement { timestamp, .. } => *timestamp,
            },
            Event::System(system_event) => match system_event {
                SystemEvent::MarketOpen { timestamp, .. } => *timestamp,
                SystemEvent::MarketClose { timestamp, .. } => *timestamp,
                SystemEvent::TradingHalt { timestamp, .. } => *timestamp,
                SystemEvent::TradingResume { timestamp, .. } => *timestamp,
                SystemEvent::SystemHealthCheck { timestamp, .. } => *timestamp,
            },
        }
    }
    
    #[inline]
    pub fn priority(&self) -> EventPriority {
        match self {
            Event::Order(OrderEvent::CancelOrder { .. }) => EventPriority::High,
            Event::Order(OrderEvent::ModifyOrder { .. }) => EventPriority::High,
            Event::Order(_) => EventPriority::Normal,
            Event::Trade(_) => EventPriority::Normal,
            Event::System(SystemEvent::TradingHalt { .. }) => EventPriority::Critical,
            Event::System(SystemEvent::SystemHealthCheck { status: HealthStatus::Critical, .. }) => EventPriority::Critical,
            Event::System(_) => EventPriority::Low,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[repr(u8)]
pub enum EventPriority {
    Low = 0,
    Normal = 1,
    High = 2,
    Critical = 3,
}

impl Default for EventPriority {
    fn default() -> Self {
        EventPriority::Normal
    }
}