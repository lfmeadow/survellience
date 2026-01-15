use crate::config::Config;
use crate::scheduler::scoring::{score_markets, MarketStats};
use crate::venue::MarketInfo;
use anyhow::{Context, Result};
use chrono::Utc;
use polars::prelude::*;
use std::collections::{HashMap, HashSet};
use std::path::Path;
use tracing::{debug, info, warn};

pub struct Scheduler {
    config: Config,
    last_rotation: Option<std::time::Instant>,
    current_hot: HashSet<(String, String)>,
    current_warm: HashSet<(String, String)>,
    rotation_cursor: usize,
}

impl Scheduler {
    pub fn new(config: Config) -> Self {
        Self {
            config,
            last_rotation: None,
            current_hot: HashSet::new(),
            current_warm: HashSet::new(),
            rotation_cursor: 0,
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

        // Select HOT markets (top 10% by score, minimum 1)
        let hot_count = std::cmp::max(1, venue_config.max_subs / 10);
        let mut new_hot = HashSet::new();
        let mut new_warm = HashSet::new();

        let mut scored_markets: Vec<&MarketInfo> = scores
            .iter()
            .filter_map(|score| markets.iter().find(|m| m.market_id == score.market_id))
            .collect();

        let hot_markets: Vec<&MarketInfo> = scored_markets.drain(0..std::cmp::min(hot_count, scored_markets.len())).collect();
        let remaining_markets = scored_markets;

        // Rotate warm markets by advancing a cursor through the remaining list.
        let warm_capacity = venue_config.max_subs.saturating_sub(hot_count);
        let remaining_len = remaining_markets.len();
        let mut warm_selected: Vec<&MarketInfo> = Vec::new();
        if remaining_len > 0 && warm_capacity > 0 {
            let start = self.rotation_cursor % remaining_len;
            for i in 0..remaining_len {
                if warm_selected.len() >= warm_capacity {
                    break;
                }
                let idx = (start + i) % remaining_len;
                warm_selected.push(remaining_markets[idx]);
            }
            self.rotation_cursor = (start + warm_selected.len()) % remaining_len;
        }

        let add_polymarket_tokens = |set: &mut HashSet<(String, String)>, market: &MarketInfo, max_subs: usize| {
            if market.token_ids.is_empty() {
                debug!("Skipping market {} - no token_ids available", market.market_id);
                return;
            }
            for token_id in &market.token_ids {
                if set.len() >= max_subs {
                    break;
                }
                set.insert((token_id.clone(), "".to_string()));
            }
        };

        let add_standard_market = |set: &mut HashSet<(String, String)>, market: &MarketInfo, max_subs: usize| {
            for outcome_id in &market.outcome_ids {
                if set.len() >= max_subs {
                    break;
                }
                set.insert((market.market_id.clone(), outcome_id.clone()));
            }
        };

        if venue_name == "polymarket" {
            for market in &hot_markets {
                add_polymarket_tokens(&mut new_hot, market, hot_count);
            }
            for market in &warm_selected {
                if new_hot.len() + new_warm.len() >= venue_config.max_subs {
                    break;
                }
                add_polymarket_tokens(&mut new_warm, market, venue_config.max_subs - new_hot.len());
            }
        } else {
            for market in &hot_markets {
                add_standard_market(&mut new_hot, market, hot_count);
            }
            for market in &warm_selected {
                if new_hot.len() + new_warm.len() >= venue_config.max_subs {
                    break;
                }
                add_standard_market(&mut new_warm, market, venue_config.max_subs - new_hot.len());
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

        let file = std::fs::File::open(&stats_path)
            .with_context(|| format!("Failed to open stats cache: {:?}", stats_path))?;
        let df = ParquetReader::new(file)
            .finish()
            .context("Failed to read stats cache parquet")?;

        // Aggregate per market_id (stats cache is per market_id/outcome_id)
        let aggregated = df
            .lazy()
            .group_by([col("market_id")])
            .agg([
                col("avg_depth").mean().alias("avg_depth"),
                col("avg_spread").mean().alias("avg_spread"),
                col("update_count").sum().alias("update_count"),
            ])
            .collect()
            .context("Failed to aggregate stats cache")?;

        let market_id_col = aggregated
            .column("market_id")?
            .str()
            .context("market_id column is not utf8")?;
        let avg_depth_col = aggregated
            .column("avg_depth")?
            .f64()
            .context("avg_depth column is not f64")?;
        let avg_spread_col = aggregated
            .column("avg_spread")?
            .f64()
            .context("avg_spread column is not f64")?;
        let update_count_col = aggregated
            .column("update_count")?
            .i64()
            .context("update_count column is not i64")?;

        let mut stats_map = HashMap::new();
        for idx in 0..aggregated.height() {
            let market_id = market_id_col.get(idx).unwrap_or("").to_string();
            if market_id.is_empty() {
                continue;
            }
            let avg_depth = avg_depth_col.get(idx).unwrap_or(0.0);
            let avg_spread = avg_spread_col.get(idx).unwrap_or(0.0);
            let update_count = update_count_col.get(idx).unwrap_or(0).max(0) as usize;

            stats_map.insert(
                market_id.clone(),
                MarketStats {
                    market_id,
                    avg_depth,
                    avg_spread,
                    update_count,
                },
            );
        }

        Ok(stats_map)
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
