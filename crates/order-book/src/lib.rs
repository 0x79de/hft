pub mod types;
pub mod order_book;
pub mod price_level;
pub mod atomic_price_level;
pub mod lockfree_order_book;
pub mod memory_pools;

pub use order_book::{OrderBook, OrderBookError, OrderBookStats, MatchResult, BookSnapshot};
pub use lockfree_order_book::{LockFreeOrderBook, LockFreeOrderBookError, LockFreeMatchResult, LockFreeBookSnapshot, LockFreeOrderBookStats};
pub use types::*;
pub use price_level::{PriceLevel, OrderInfo};
pub use atomic_price_level::{AtomicPriceLevel, LockFreeOrderQueue};
pub use memory_pools::{MemoryPool, VecPool, PooledObject, PooledVec, TradeArray, OrderArray, GlobalPools, allocators};

pub type Result<T> = std::result::Result<T, OrderBookError>;