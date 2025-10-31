use anyhow::Result;
use orderbook::engine;

#[tokio::main]
async fn main() -> Result<()> {
    engine::engine().await
}
