pub mod types;
pub mod order_book;
pub mod price_level;

pub use order_book::{OrderBook, OrderBookError, OrderBookStats, MatchResult, BookSnapshot};
pub use types::*;
pub use price_level::{PriceLevel, AtomicPriceLevel, OrderInfo};

pub type Result<T> = std::result::Result<T, OrderBookError>;