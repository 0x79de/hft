pub mod processor;
pub mod events;
pub mod channels;
pub mod batch;

pub use processor::EventProcessor;
pub use events::*;
pub use channels::*;
pub use batch::BatchProcessor;

pub type Result<T> = std::result::Result<T, Box<dyn std::error::Error + Send + Sync>>;