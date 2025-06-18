pub mod feed;
pub mod snapshot;
pub mod stream;
pub mod types;

pub use feed::MarketDataFeed;
pub use snapshot::*;
pub use stream::*;
pub use types::*;

pub type Result<T> = std::result::Result<T, Box<dyn std::error::Error + Send + Sync>>;