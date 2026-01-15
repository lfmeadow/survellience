use crate::config::Config;
use crate::scheduler::scoring::{score_markets, MarketStats};
use crate::venue::MarketInfo;
use anyhow::{Context, Result};
use chrono::Utc;
use std::collections::{HashMap, HashSet};
use std::path::Path;
use tracing::{debug, info, warn};

pub struct Scheduler {
    config: Config,
    last_rotation: Option<std::time::Instant>,
    current_hot: HashSet<(String, String)>,
    current_warm: HashSet<(String, String)>,
}

impl Scheduler {
    pub fn new(config: Config) -> Self {
        Self {
            config,
            last_rotation: None,
            current_hot: HashSet::new(),
            current_warm: HashSet::new(),
        }
    }

    pub fn get_target_subscriptions(
        &mut self,
        venue_name: &str,
    ) -> Result<(HashSet<(String, String)>, HashSet<(String, String)>)> {
        let venue_config = self
            .config
            .get_venue_config(venue_name)
            .context("Venue config not found")?;

        // Load universe
        let markets = self.load_universe(venue_name)?;

        // Load stats cache if available
        let stats_cache = self.load_stats_cache(venue_name).ok();

        // Score markets
        let scores = score_markets(&markets, stats_cache.as_ref());

        // Select HOT markets (top N by score)
        let hot_count = venue_config.hot_count.min(venue_config.max_subs);
        let mut new_hot = HashSet::new();
        let mut new_warm = HashSet::new();

        for (idx, score) in scores.iter().enumerate() {
            if let Some(market) = markets.iter().find(|m| m.market_id == score.market_id) {
                // For Polymarket, use token_ids for subscriptions (CLOB WebSocket requires token IDs)
                // For other venues, use market_id/outcome_id pairs
                if venue_name == "polymarket" {
                    if !market.token_ids.is_empty() {
                        // Polymarket: subscribe to all token_ids for this market
                        for token_id in &market.token_ids {
                            if idx < hot_count {
                                new_hot.insert((token_id.clone(), "".to_string())); // outcome_id not used for token-based subs
                            } else if new_hot.len() + new_warm.len() < venue_config.max_subs {
                                new_warm.insert((token_id.clone(), "".to_string()));
                            }
                        }
                    } else {
                        // Skip markets without token_ids (they can't be subscribed to via CLOB WebSocket)
                        debug!("Skipping market {} - no token_ids available", market.market_id);
                    }
                } else {
                    // Other venues: use market_id/outcome_id pairs
                    for outcome_id in &market.outcome_ids {
                        if idx < hot_count {
                            new_hot.insert((score.market_id.clone(), outcome_id.clone()));
                        } else if new_hot.len() + new_warm.len() < venue_config.max_subs {
                            new_warm.insert((score.market_id.clone(), outcome_id.clone()));
                        }
                    }
                }
            }
        }

        // Compute diffs
        let hot_to_add: Vec<_> = new_hot.difference(&self.current_hot).cloned().collect();
        let hot_to_remove: Vec<_> = self.current_hot.difference(&new_hot).cloned().collect();
        let warm_to_add: Vec<_> = new_warm.difference(&self.current_warm).cloned().collect();
        let warm_to_remove: Vec<_> = self.current_warm.difference(&new_warm).cloned().collect();

        info!(
            "Scheduler for {}: HOT {}->{} (add {}, remove {}), WARM {}->{} (add {}, remove {})",
            venue_name,
            self.current_hot.len(),
            new_hot.len(),
            hot_to_add.len(),
            hot_to_remove.len(),
            self.current_warm.len(),
            new_warm.len(),
            warm_to_add.len(),
            warm_to_remove.len(),
        );

        self.current_hot = new_hot.clone();
        self.current_warm = new_warm.clone();

        Ok((new_hot, new_warm))
    }

    pub fn should_rotate(&self, venue_name: &str) -> bool {
        if !self.config.rotation.enabled {
            return false;
        }

        let venue_config = match self.config.get_venue_config(venue_name) {
            Some(cfg) => cfg,
            None => {
                warn!("Venue config not found for {}", venue_name);
                return false;
            }
        };

        if let Some(last) = self.last_rotation {
            last.elapsed().as_secs() >= venue_config.rotation_period_secs
        } else {
            true
        }
    }

    pub fn mark_rotated(&mut self) {
        self.last_rotation = Some(std::time::Instant::now());
    }

    fn load_universe(&self, venue_name: &str) -> Result<Vec<MarketInfo>> {
        let today = Utc::now().date_naive();
        let date_str = today.format("%Y-%m-%d").to_string();

        let universe_path = Path::new(&self.config.data_dir)
            .join("metadata")
            .join(format!("venue={}", venue_name))
            .join(format!("date={}", date_str))
            .join("universe.jsonl");

        if !universe_path.exists() {
            warn!("Universe file not found: {:?}", universe_path);
            return Ok(vec![]);
        }

        let content = std::fs::read_to_string(&universe_path)
            .with_context(|| format!("Failed to read universe file: {:?}", universe_path))?;

        let mut markets = Vec::new();
        for line in content.lines() {
            if line.trim().is_empty() {
                continue;
            }
            let market: MarketInfo = serde_json::from_str(line)
                .with_context(|| format!("Failed to parse market info: {}", line))?;
            markets.push(market);
        }

        Ok(markets)
    }

    fn load_stats_cache(&self, venue_name: &str) -> Result<HashMap<String, MarketStats>> {
        let today = Utc::now().date_naive();
        let date_str = today.format("%Y-%m-%d").to_string();

        let stats_path = Path::new(&self.config.data_dir)
            .join("stats")
            .join(format!("venue={}", venue_name))
            .join(format!("date={}", date_str))
            .join("stats.parquet");

        if !stats_path.exists() {
            return Ok(HashMap::new());
        }

        // TODO: Load from Parquet using Polars
        // For now, return empty
        Ok(HashMap::new())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{MockConfig, RotationConfig, StorageConfig, VenuesConfig, VenueConfig};
    use tempfile::TempDir;

    #[test]
    fn test_scheduler_should_rotate() {
        let temp_dir = TempDir::new().unwrap();
        let config = Config {
            data_dir: temp_dir.path().to_string_lossy().to_string(),
            venues: VenuesConfig {
                polymarket: Some(VenueConfig {
                    enabled: true,
                    api_key: String::new(),
                    api_secret: String::new(),
                    ws_url: None,
                    rest_url: None,
                    max_subs: 200,
                    hot_count: 40,
                    rotation_period_secs: 1, // 1 second for testing
                    snapshot_interval_ms_hot: 2000,
                    snapshot_interval_ms_warm: 10000,
                    subscription_churn_limit_per_minute: 20,
                }),
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
                markets_per_venue: 500,
            },
        };

        let mut scheduler = Scheduler::new(config);
        assert!(scheduler.should_rotate("polymarket"));
        scheduler.mark_rotated();
        assert!(!scheduler.should_rotate("polymarket"));
    }
}
