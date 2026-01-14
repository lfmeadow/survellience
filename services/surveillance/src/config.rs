use anyhow::{Context, Result};
use serde::Deserialize;
use std::path::Path;

#[derive(Debug, Clone, Deserialize)]
pub struct Config {
    pub data_dir: String,
    pub venues: VenuesConfig,
    pub storage: StorageConfig,
    pub rotation: RotationConfig,
    pub mock: MockConfig,
}

#[derive(Debug, Clone, Deserialize)]
pub struct VenuesConfig {
    pub polymarket: Option<VenueConfig>,
    pub kalshi: Option<VenueConfig>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct VenueConfig {
    pub enabled: bool,
    pub api_key: String,
    pub api_secret: String,
    #[serde(default)]
    pub ws_url: Option<String>,
    #[serde(default)]
    pub rest_url: Option<String>,
    #[serde(default = "default_max_subs")]
    pub max_subs: usize,
    #[serde(default = "default_hot_count")]
    pub hot_count: usize,
    #[serde(default = "default_rotation_period_secs")]
    pub rotation_period_secs: u64,
    #[serde(default = "default_snapshot_interval_ms_hot")]
    pub snapshot_interval_ms_hot: u64,
    #[serde(default = "default_snapshot_interval_ms_warm")]
    pub snapshot_interval_ms_warm: u64,
    #[serde(default = "default_subscription_churn_limit")]
    pub subscription_churn_limit_per_minute: usize,
}

#[derive(Debug, Clone, Deserialize)]
pub struct StorageConfig {
    #[serde(default = "default_top_k")]
    pub top_k: usize,
    #[serde(default = "default_flush_rows")]
    pub flush_rows: usize,
    #[serde(default = "default_flush_seconds")]
    pub flush_seconds: u64,
    #[serde(default = "default_bucket_minutes")]
    pub bucket_minutes: u64,
}

#[derive(Debug, Clone, Deserialize)]
pub struct RotationConfig {
    #[serde(default = "default_true")]
    pub enabled: bool,
}

#[derive(Debug, Clone, Deserialize)]
pub struct MockConfig {
    #[serde(default = "default_false")]
    pub enabled: bool,
    #[serde(default = "default_universe_size")]
    pub universe_size: usize,
    #[serde(default = "default_markets_per_venue")]
    pub markets_per_venue: usize,
}

impl Config {
    pub fn load<P: AsRef<Path>>(path: P) -> Result<Self> {
        let content = std::fs::read_to_string(path.as_ref())
            .with_context(|| format!("Failed to read config from {:?}", path.as_ref()))?;
        let config: Config = toml::from_str(&content)
            .context("Failed to parse config TOML")?;
        Ok(config)
    }

    pub fn get_venue_config(&self, venue_name: &str) -> Option<&VenueConfig> {
        match venue_name {
            "polymarket" => self.venues.polymarket.as_ref(),
            "kalshi" => self.venues.kalshi.as_ref(),
            _ => None,
        }
    }
}

fn default_max_subs() -> usize {
    200
}

fn default_hot_count() -> usize {
    40
}

fn default_rotation_period_secs() -> u64 {
    180
}

fn default_snapshot_interval_ms_hot() -> u64 {
    2000
}

fn default_snapshot_interval_ms_warm() -> u64 {
    10000
}

fn default_subscription_churn_limit() -> usize {
    20
}

fn default_top_k() -> usize {
    50
}

fn default_flush_rows() -> usize {
    50_000
}

fn default_flush_seconds() -> u64 {
    5
}

fn default_bucket_minutes() -> u64 {
    5
}

fn default_true() -> bool {
    true
}

fn default_false() -> bool {
    false
}

fn default_universe_size() -> usize {
    1000
}

fn default_markets_per_venue() -> usize {
    500
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_config_load() {
        let config = Config::load("../../config/surveillance.toml").unwrap();
        assert_eq!(config.data_dir, "data");
        assert!(config.storage.top_k > 0);
    }

    #[test]
    fn test_config_defaults() {
        let toml_str = r#"
data_dir = "test_data"
[storage]
[rotation]
[mock]
enabled = true
[venues.polymarket]
enabled = false
api_key = ""
api_secret = ""
"#;
        let config: Config = toml::from_str(toml_str).unwrap();
        assert_eq!(config.storage.top_k, 50);
        assert_eq!(config.storage.flush_rows, 50_000);
    }
}
