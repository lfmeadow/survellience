use anyhow::Result;
use surveillance::analytics::Miner;
use surveillance::config::Config;
use tracing_subscriber;

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .init();

    let config_path = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "config/surveillance.toml".to_string());

    let config = Config::load(&config_path)?;
    let miner = Miner::new(config);

    let venue = std::env::args()
        .nth(2)
        .unwrap_or_else(|| "polymarket".to_string());

    let date = std::env::args().nth(3);

    miner.mine(&venue, date.as_deref()).await?;

    Ok(())
}
