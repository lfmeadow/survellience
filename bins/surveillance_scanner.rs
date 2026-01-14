use anyhow::Result;
use surveillance::config::Config;
use surveillance::scanner::Scanner;
use surveillance::venue::{KalshiVenue, MockVenue, PolymarketVenue};
use std::collections::HashMap;
use std::sync::Arc;
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

    let mut venues: HashMap<String, Box<dyn surveillance::venue::Venue>> = HashMap::new();

    if config.mock.enabled {
        venues.insert(
            "polymarket".to_string(),
            Box::new(MockVenue::new(
                "polymarket".to_string(),
                config.mock.markets_per_venue,
            )),
        );
        venues.insert(
            "kalshi".to_string(),
            Box::new(MockVenue::new(
                "kalshi".to_string(),
                config.mock.markets_per_venue,
            )),
        );
    } else {
        if let Some(pm_config) = &config.venues.polymarket {
            if pm_config.enabled {
                venues.insert(
                    "polymarket".to_string(),
                    Box::new(PolymarketVenue::new(
                        "polymarket".to_string(),
                        pm_config.api_key.clone(),
                        pm_config.api_secret.clone(),
                        pm_config.ws_url.clone().unwrap_or_default(),
                        pm_config.rest_url.clone().unwrap_or_default(),
                    )),
                );
            }
        }

        if let Some(k_config) = &config.venues.kalshi {
            if k_config.enabled {
                venues.insert(
                    "kalshi".to_string(),
                    Box::new(KalshiVenue::new(
                        "kalshi".to_string(),
                        k_config.api_key.clone(),
                        k_config.api_secret.clone(),
                        k_config.ws_url.clone().unwrap_or_default(),
                        k_config.rest_url.clone().unwrap_or_default(),
                    )),
                );
            }
        }
    }

    let scanner = Scanner::new(config, venues);
    scanner.scan_all().await?;

    Ok(())
}
