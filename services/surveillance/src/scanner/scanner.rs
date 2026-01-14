use crate::config::Config;
use crate::venue::{MarketInfo, Venue};
use anyhow::{Context, Result};
use chrono::Utc;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use tracing::{info, warn};

pub struct Scanner {
    config: Config,
    venues: HashMap<String, Box<dyn Venue>>,
}

impl Scanner {
    pub fn new(config: Config, venues: HashMap<String, Box<dyn Venue>>) -> Self {
        Self { config, venues }
    }

    pub async fn scan_all(&self) -> Result<()> {
        for (venue_name, venue) in &self.venues {
            info!("Scanning venue: {}", venue_name);
            match self.scan_venue(venue_name, venue.as_ref()).await {
                Ok(count) => {
                    info!("Discovered {} markets for venue {}", count, venue_name);
                }
                Err(e) => {
                    warn!("Failed to scan venue {}: {}", venue_name, e);
                }
            }
        }
        Ok(())
    }

    async fn scan_venue(&self, venue_name: &str, venue: &dyn Venue) -> Result<usize> {
        let markets = venue
            .discover_markets()
            .await
            .with_context(|| format!("Failed to discover markets for {}", venue_name))?;

        let output_path = self.get_output_path(venue_name)?;
        
        // Ensure directory exists
        if let Some(parent) = output_path.parent() {
            std::fs::create_dir_all(parent)
                .with_context(|| format!("Failed to create directory: {:?}", parent))?;
        }

        // Write universe file as JSONL
        let mut file = std::fs::File::create(&output_path)
            .with_context(|| format!("Failed to create universe file: {:?}", output_path))?;
        
        use std::io::Write;
        for market in &markets {
            let json = serde_json::to_string(market)
                .context("Failed to serialize market info")?;
            writeln!(file, "{}", json)
                .context("Failed to write market info")?;
        }

        info!(
            "Wrote universe file: {:?} ({} markets)",
            output_path,
            markets.len()
        );

        Ok(markets.len())
    }

    fn get_output_path(&self, venue_name: &str) -> Result<PathBuf> {
        let today = Utc::now().date_naive();
        let date_str = today.format("%Y-%m-%d").to_string();
        
        let path = Path::new(&self.config.data_dir)
            .join("metadata")
            .join(format!("venue={}", venue_name))
            .join(format!("date={}", date_str))
            .join("universe.jsonl");
        
        Ok(path)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{MockConfig, RotationConfig, StorageConfig, VenuesConfig};
    use crate::venue::MockVenue;
    use tempfile::TempDir;

    #[tokio::test]
    async fn test_scanner_scan_venue() {
        let temp_dir = TempDir::new().unwrap();
        let config = Config {
            data_dir: temp_dir.path().to_string_lossy().to_string(),
            venues: VenuesConfig {
                polymarket: None,
                kalshi: None,
            },
            storage: StorageConfig {
                top_k: 50,
                flush_rows: 50000,
                flush_seconds: 5,
                bucket_minutes: 5,
            },
            rotation: RotationConfig { enabled: true },
            mock: MockConfig {
                enabled: true,
                universe_size: 1000,
                markets_per_venue: 10,
            },
        };

        let mut venues = HashMap::new();
        venues.insert(
            "polymarket".to_string(),
            Box::new(MockVenue::new("polymarket".to_string(), 10)) as Box<dyn Venue>,
        );

        let scanner = Scanner::new(config, venues);
        let result = scanner.scan_all().await;
        assert!(result.is_ok());

        // Check that universe file was created
        let today = Utc::now().date_naive();
        let date_str = today.format("%Y-%m-%d").to_string();
        let universe_path = temp_dir
            .path()
            .join("metadata")
            .join("venue=polymarket")
            .join(format!("date={}", date_str))
            .join("universe.jsonl");
        
        assert!(universe_path.exists());
        
        // Check file contents
        let content = std::fs::read_to_string(&universe_path).unwrap();
        let lines: Vec<&str> = content.trim().lines().collect();
        assert_eq!(lines.len(), 10);
    }
}
