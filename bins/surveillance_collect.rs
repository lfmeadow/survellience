use anyhow::Result;
use surveillance::collector::Collector;
use surveillance::config::Config;
use surveillance::scheduler::Scheduler;
use surveillance::storage::ParquetWriter;
use surveillance::venue::{KalshiVenue, MockVenue, PolymarketVenue};
use std::sync::Arc;
use tracing_subscriber;

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .with_target(false)
        .init();

    let config_path = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "config/surveillance.toml".to_string());

    let config = Arc::new(Config::load(&config_path)?);
    let writer = Arc::new(ParquetWriter::new(config.clone()));
    let scheduler = Arc::new(Scheduler::new((*config).clone()));

    let mut collectors = Vec::new();

    if config.mock.enabled {
        let venue: Box<dyn surveillance::venue::Venue> = Box::new(MockVenue::new(
            "polymarket".to_string(),
            config.mock.markets_per_venue,
        ));
        let mut collector = Collector::new(
            config.clone(),
            venue,
            "polymarket".to_string(),
            writer.clone(),
            scheduler.clone(),
        );
        collectors.push(tokio::spawn(async move {
            collector.run().await
        }));
    } else {
        if let Some(pm_config) = &config.venues.polymarket {
            if pm_config.enabled {
                let venue: Box<dyn surveillance::venue::Venue> = Box::new(PolymarketVenue::new(
                    "polymarket".to_string(),
                    pm_config.api_key.clone(),
                    pm_config.api_secret.clone(),
                    pm_config.ws_url.clone().unwrap_or_default(),
                    pm_config.rest_url.clone().unwrap_or_default(),
                ));
                let mut collector = Collector::new(
                    config.clone(),
                    venue,
                    "polymarket".to_string(),
                    writer.clone(),
                    scheduler.clone(),
                );
                collectors.push(tokio::spawn(async move {
                    collector.run().await
                }));
            }
        }

        if let Some(k_config) = &config.venues.kalshi {
            if k_config.enabled {
                let (api_key, api_secret) = KalshiVenue::load_credentials(
                    &k_config.api_key,
                    &k_config.api_secret,
                )?;
                let venue: Box<dyn surveillance::venue::Venue> = Box::new(KalshiVenue::new(
                    "kalshi".to_string(),
                    api_key,
                    api_secret,
                    k_config.ws_url.clone().unwrap_or_default(),
                    k_config.rest_url.clone().unwrap_or_default(),
                ));
                let mut collector = Collector::new(
                    config.clone(),
                    venue,
                    "kalshi".to_string(),
                    writer.clone(),
                    scheduler.clone(),
                );
                collectors.push(tokio::spawn(async move {
                    collector.run().await
                }));
            }
        }
    }

    futures::future::try_join_all(collectors).await?;

    Ok(())
}
