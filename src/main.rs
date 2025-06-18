use tracing::{info, Level};
use tracing_subscriber;

#[global_allocator]
static GLOBAL: mimalloc::MiMalloc = mimalloc::MiMalloc;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_max_level(Level::INFO)
        .init();

    info!("Starting HFT Trading System v{}", env!("CARGO_PKG_VERSION"));

    info!("HFT Trading System initialized successfully");

    Ok(())
}
