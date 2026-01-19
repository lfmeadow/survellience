//! Rules ingestion - fetches and stores market rules text

use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sha2::{Sha256, Digest};
use std::collections::HashSet;
use std::path::Path;
use std::io::{BufRead, BufReader, Write};

/// Raw rules record for a market
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RulesRecord {
    pub venue: String,
    pub market_id: String,
    pub outcome_id: Option<String>,
    pub url: Option<String>,
    pub fetched_ts: i64,            // epoch ms
    pub title: String,
    pub close_ts: Option<i64>,
    pub raw_rules_text: String,
    pub raw_resolution_source: Option<String>,
    pub raw_json: Option<serde_json::Value>,
}

impl RulesRecord {
    /// Compute SHA256 hash of raw rules text
    pub fn rules_hash(&self) -> String {
        let mut hasher = Sha256::new();
        hasher.update(self.raw_rules_text.as_bytes());
        format!("{:x}", hasher.finalize())
    }
}

/// Market info from universe file
#[derive(Debug, Clone, Deserialize)]
pub struct UniverseMarket {
    pub market_id: String,
    pub title: String,
    pub outcome_ids: Vec<String>,
    pub close_ts: Option<i64>,
    pub status: String,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default)]
    pub token_ids: Vec<String>,
}

/// Configuration for rules ingestion
#[derive(Debug, Clone)]
pub struct IngestConfig {
    pub venue: String,
    pub date: String,
    pub data_dir: String,
    pub force_refetch: bool,
    pub concurrency: usize,
    pub rate_limit_ms: u64,
}

impl Default for IngestConfig {
    fn default() -> Self {
        Self {
            venue: "polymarket".to_string(),
            date: Utc::now().format("%Y-%m-%d").to_string(),
            data_dir: "data".to_string(),
            force_refetch: false,
            concurrency: 2,
            rate_limit_ms: 500,
        }
    }
}

/// Rules ingestor trait
pub trait RulesIngestor: Send + Sync {
    /// Fetch rules for a single market
    fn fetch_rules(&self, market: &UniverseMarket) -> Result<RulesRecord>;
    
    /// Venue name
    fn venue(&self) -> &str;
}

/// Mock ingestor for testing
pub struct MockIngestor {
    venue: String,
}

impl MockIngestor {
    pub fn new(venue: &str) -> Self {
        Self {
            venue: venue.to_string(),
        }
    }
    
    /// Generate synthetic rules for testing ladders
    fn generate_btc_ladder_rules(&self, market: &UniverseMarket) -> String {
        let title_lower = market.title.to_lowercase();
        
        // Parse strike from title like "BTC >= $100,000"
        if let Some(strike) = extract_strike_from_title(&title_lower) {
            let comparator = if title_lower.contains(">=") || title_lower.contains("at or above") || title_lower.contains("reach") {
                "at or above"
            } else if title_lower.contains(">") || title_lower.contains("above") {
                "above"
            } else if title_lower.contains("<=") || title_lower.contains("at or below") {
                "at or below"
            } else if title_lower.contains("<") || title_lower.contains("below") || title_lower.contains("dip") {
                "below"
            } else {
                "at or above"
            };
            
            let underlier = if title_lower.contains("btc") || title_lower.contains("bitcoin") {
                "Bitcoin (BTC)"
            } else if title_lower.contains("eth") || title_lower.contains("ethereum") {
                "Ethereum (ETH)"
            } else if title_lower.contains("sol") || title_lower.contains("solana") {
                "Solana (SOL)"
            } else {
                "Bitcoin (BTC)"
            };
            
            format!(
                "This market will resolve to \"Yes\" if the price of {} is {} ${:.0} at any time before the market closes, according to the spot price on Coinbase. Otherwise, it will resolve to \"No\".",
                underlier, comparator, strike
            )
        } else {
            // Generic yes/no event
            format!(
                "This market will resolve to \"Yes\" if {} occurs before the market closes. Otherwise, it will resolve to \"No\".",
                market.title
            )
        }
    }
}

impl RulesIngestor for MockIngestor {
    fn fetch_rules(&self, market: &UniverseMarket) -> Result<RulesRecord> {
        let rules_text = self.generate_btc_ladder_rules(market);
        
        Ok(RulesRecord {
            venue: self.venue.clone(),
            market_id: market.market_id.clone(),
            outcome_id: None,
            url: Some(format!("https://mock.venue/{}", market.market_id)),
            fetched_ts: Utc::now().timestamp_millis(),
            title: market.title.clone(),
            close_ts: market.close_ts,
            raw_rules_text: rules_text,
            raw_resolution_source: Some("Coinbase".to_string()),
            raw_json: None,
        })
    }
    
    fn venue(&self) -> &str {
        &self.venue
    }
}

/// Stub ingestor for Polymarket (TODO: implement real fetching)
pub struct PolymarketIngestor {
    #[allow(dead_code)]
    api_url: String,
}

impl PolymarketIngestor {
    pub fn new() -> Self {
        Self {
            api_url: "https://gamma-api.polymarket.com".to_string(),
        }
    }
}

impl Default for PolymarketIngestor {
    fn default() -> Self {
        Self::new()
    }
}

impl RulesIngestor for PolymarketIngestor {
    fn fetch_rules(&self, market: &UniverseMarket) -> Result<RulesRecord> {
        // TODO: Implement real Polymarket rules fetching
        // For now, use title as rules text placeholder
        Ok(RulesRecord {
            venue: "polymarket".to_string(),
            market_id: market.market_id.clone(),
            outcome_id: None,
            url: Some(format!("https://polymarket.com/event/{}", market.market_id)),
            fetched_ts: Utc::now().timestamp_millis(),
            title: market.title.clone(),
            close_ts: market.close_ts,
            raw_rules_text: market.title.clone(), // Placeholder
            raw_resolution_source: None,
            raw_json: None,
        })
    }
    
    fn venue(&self) -> &str {
        "polymarket"
    }
}

/// Stub ingestor for Kalshi (TODO: implement real fetching)
pub struct KalshiIngestor {
    #[allow(dead_code)]
    api_url: String,
}

impl KalshiIngestor {
    pub fn new() -> Self {
        Self {
            api_url: "https://trading-api.kalshi.com".to_string(),
        }
    }
}

impl Default for KalshiIngestor {
    fn default() -> Self {
        Self::new()
    }
}

impl RulesIngestor for KalshiIngestor {
    fn fetch_rules(&self, market: &UniverseMarket) -> Result<RulesRecord> {
        // TODO: Implement real Kalshi rules fetching
        Ok(RulesRecord {
            venue: "kalshi".to_string(),
            market_id: market.market_id.clone(),
            outcome_id: None,
            url: Some(format!("https://kalshi.com/markets/{}", market.market_id)),
            fetched_ts: Utc::now().timestamp_millis(),
            title: market.title.clone(),
            close_ts: market.close_ts,
            raw_rules_text: market.title.clone(), // Placeholder
            raw_resolution_source: None,
            raw_json: None,
        })
    }
    
    fn venue(&self) -> &str {
        "kalshi"
    }
}

/// Load universe file
pub fn load_universe(data_dir: &str, venue: &str, date: &str) -> Result<Vec<UniverseMarket>> {
    let path = Path::new(data_dir)
        .join("metadata")
        .join(format!("venue={}", venue))
        .join(format!("date={}", date))
        .join("universe.jsonl");
    
    if !path.exists() {
        anyhow::bail!("Universe file not found: {:?}", path);
    }
    
    let file = std::fs::File::open(&path)
        .with_context(|| format!("Failed to open universe file: {:?}", path))?;
    let reader = BufReader::new(file);
    
    let mut markets = Vec::new();
    for line in reader.lines() {
        let line = line?;
        if line.trim().is_empty() {
            continue;
        }
        let market: UniverseMarket = serde_json::from_str(&line)
            .with_context(|| format!("Failed to parse market: {}", line))?;
        markets.push(market);
    }
    
    Ok(markets)
}

/// Load existing rules to avoid refetching
pub fn load_existing_rules(data_dir: &str, venue: &str, date: &str) -> Result<HashSet<String>> {
    let path = Path::new(data_dir)
        .join("rules")
        .join(format!("venue={}", venue))
        .join(format!("date={}", date))
        .join("rules.jsonl");
    
    let mut existing = HashSet::new();
    
    if path.exists() {
        let file = std::fs::File::open(&path)?;
        let reader = BufReader::new(file);
        
        for line in reader.lines() {
            let line = line?;
            if line.trim().is_empty() {
                continue;
            }
            if let Ok(record) = serde_json::from_str::<RulesRecord>(&line) {
                existing.insert(record.market_id);
            }
        }
    }
    
    Ok(existing)
}

/// Run ingestion for a venue
pub fn run_ingest(
    config: &IngestConfig,
    ingestor: &dyn RulesIngestor,
) -> Result<Vec<RulesRecord>> {
    let markets = load_universe(&config.data_dir, &config.venue, &config.date)?;
    tracing::info!("Loaded {} markets from universe", markets.len());
    
    let existing = if config.force_refetch {
        HashSet::new()
    } else {
        load_existing_rules(&config.data_dir, &config.venue, &config.date)?
    };
    tracing::info!("Found {} existing rules records", existing.len());
    
    let mut records = Vec::new();
    let mut skipped = 0;
    let mut errors = 0;
    
    for market in &markets {
        if existing.contains(&market.market_id) {
            skipped += 1;
            continue;
        }
        
        match ingestor.fetch_rules(market) {
            Ok(record) => {
                records.push(record);
            }
            Err(e) => {
                tracing::warn!("Failed to fetch rules for {}: {}", market.market_id, e);
                errors += 1;
            }
        }
        
        // Rate limiting
        if config.rate_limit_ms > 0 {
            std::thread::sleep(std::time::Duration::from_millis(config.rate_limit_ms));
        }
    }
    
    tracing::info!(
        "Ingested {} rules, skipped {} existing, {} errors",
        records.len(), skipped, errors
    );
    
    Ok(records)
}

/// Write rules records to JSONL file
pub fn write_rules_jsonl(
    data_dir: &str,
    venue: &str,
    date: &str,
    records: &[RulesRecord],
    append: bool,
) -> Result<()> {
    let dir = Path::new(data_dir)
        .join("rules")
        .join(format!("venue={}", venue))
        .join(format!("date={}", date));
    
    std::fs::create_dir_all(&dir)?;
    
    let path = dir.join("rules.jsonl");
    
    let mut file = if append && path.exists() {
        std::fs::OpenOptions::new()
            .append(true)
            .open(&path)?
    } else {
        std::fs::File::create(&path)?
    };
    
    for record in records {
        let json = serde_json::to_string(record)?;
        writeln!(file, "{}", json)?;
    }
    
    tracing::info!("Wrote {} rules records to {:?}", records.len(), path);
    Ok(())
}

/// Extract numeric strike from title
fn extract_strike_from_title(title: &str) -> Option<f64> {
    // Pattern: $100,000 or $100000 or 100k or 100K
    let re_dollar = regex::Regex::new(r"\$([0-9,]+(?:\.[0-9]+)?)").ok()?;
    let re_k = regex::Regex::new(r"([0-9]+(?:\.[0-9]+)?)\s*[kK]").ok()?;
    
    if let Some(caps) = re_dollar.captures(title) {
        let num_str = caps.get(1)?.as_str().replace(',', "");
        return num_str.parse().ok();
    }
    
    if let Some(caps) = re_k.captures(title) {
        let num: f64 = caps.get(1)?.as_str().parse().ok()?;
        return Some(num * 1000.0);
    }
    
    None
}

/// Generate mock universe for testing
pub fn generate_mock_universe(venue: &str) -> Vec<UniverseMarket> {
    let now = Utc::now();
    let close_ts = (now + chrono::Duration::days(7)).timestamp_millis();
    
    let strikes = vec![80000.0, 90000.0, 100000.0, 110000.0, 120000.0];
    
    strikes
        .iter()
        .enumerate()
        .map(|(i, &strike)| UniverseMarket {
            market_id: format!("mock-btc-{}", i),
            title: format!("Will BTC reach ${:.0} by next week?", strike),
            outcome_ids: vec!["0".to_string(), "1".to_string()],
            close_ts: Some(close_ts),
            status: "active".to_string(),
            tags: vec!["btc".to_string(), "crypto".to_string()],
            token_ids: vec![],
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_extract_strike() {
        assert_eq!(extract_strike_from_title("BTC >= $100,000"), Some(100000.0));
        assert_eq!(extract_strike_from_title("BTC above $90000"), Some(90000.0));
        assert_eq!(extract_strike_from_title("Will reach 100k"), Some(100000.0));
        assert_eq!(extract_strike_from_title("No number here"), None);
    }
    
    #[test]
    fn test_mock_ingestor() {
        let ingestor = MockIngestor::new("mock");
        let market = UniverseMarket {
            market_id: "test-1".to_string(),
            title: "Will BTC reach $100,000?".to_string(),
            outcome_ids: vec!["0".to_string(), "1".to_string()],
            close_ts: Some(1234567890000),
            status: "active".to_string(),
            tags: vec![],
            token_ids: vec![],
        };
        
        let record = ingestor.fetch_rules(&market).unwrap();
        assert!(record.raw_rules_text.contains("at or above"));
        assert!(record.raw_rules_text.contains("100000"));
    }
    
    #[test]
    fn test_rules_hash() {
        let record = RulesRecord {
            venue: "test".to_string(),
            market_id: "1".to_string(),
            outcome_id: None,
            url: None,
            fetched_ts: 0,
            title: "Test".to_string(),
            close_ts: None,
            raw_rules_text: "Test rules".to_string(),
            raw_resolution_source: None,
            raw_json: None,
        };
        
        let hash = record.rules_hash();
        assert_eq!(hash.len(), 64); // SHA256 hex
    }
}
