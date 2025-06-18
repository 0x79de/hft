pub mod manager;
pub mod limits;
pub mod position;
pub mod validation;

pub use manager::RiskManager;
pub use limits::*;
pub use position::Position;
pub use validation::*;

pub type Result<T> = std::result::Result<T, Box<dyn std::error::Error + Send + Sync>>;