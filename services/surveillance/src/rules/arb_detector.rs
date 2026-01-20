//! Arbitrage violation detection

use anyhow::{Context, Result};
use polars::prelude::*;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::Path;
use crate::rules::constraints::Constraint;

/// A detected violation of a constraint
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Violation {
    pub ts: i64,                        // timestamp of detection
    pub constraint_id: String,
    pub constraint_type: String,
    pub a_market_id: String,
    pub a_outcome_id: Option<String>,
    pub b_market_id: String,
    pub b_outcome_id: Option<String>,
    pub p_a: f64,                       // implied probability of A
    pub p_b: f64,                       // implied probability of B
    pub violation_magnitude: f64,       // p_a - p_b (for P(A) <= P(B) constraint)
    pub margin: f64,                    // required margin
    pub confidence: f64,                // constraint confidence
    pub a_bid: Option<f64>,
    pub a_ask: Option<f64>,
    pub b_bid: Option<f64>,
    pub b_ask: Option<f64>,
}

/// Configuration for arb detection
#[derive(Debug, Clone)]
pub struct ArbDetectorConfig {
    pub margin: f64,                    // violation margin (default 0.01)
    pub mode: DetectionMode,
    pub window_minutes: Option<u32>,    // for rolling mode
}

impl Default for ArbDetectorConfig {
    fn default() -> Self {
        Self {
            margin: 0.01,
            mode: DetectionMode::Latest,
            window_minutes: None,
        }
    }
}

/// Detection mode
#[derive(Debug, Clone, Copy)]
pub enum DetectionMode {
    Latest,     // Use latest snapshot per market
    Rolling,    // Rolling window detection
}

/// Market price data from snapshots
#[derive(Debug, Clone, Default)]
pub struct MarketPrice {
    pub ts: i64,
    pub mid: Option<f64>,
    pub best_bid: Option<f64>,
    pub best_ask: Option<f64>,
}

impl MarketPrice {
    /// Get implied probability (0..1 scale)
    /// Assumes prices are already in 0..1 range
    pub fn implied_probability(&self) -> Option<f64> {
        if let Some(mid) = self.mid {
            if mid.is_finite() && mid >= 0.0 && mid <= 1.0 {
                return Some(mid);
            }
        }
        
        // Fallback to (bid + ask) / 2
        match (self.best_bid, self.best_ask) {
            (Some(bid), Some(ask)) if bid.is_finite() && ask.is_finite() => {
                let mid = (bid + ask) / 2.0;
                if mid >= 0.0 && mid <= 1.0 {
                    Some(mid)
                } else if mid >= 0.0 && mid <= 100.0 {
                    // Convert from cents to probability
                    Some(mid / 100.0)
                } else {
                    None
                }
            }
            _ => None,
        }
    }
}

/// Load latest prices from parquet snapshots
pub fn load_latest_prices(
    data_dir: &str,
    venue: &str,
    date: &str,
) -> Result<HashMap<(String, String), MarketPrice>> {
    let snapshot_dir = Path::new(data_dir)
        .join("orderbook_snapshots")
        .join(format!("venue={}", venue))
        .join(format!("date={}", date));
    
    if !snapshot_dir.exists() {
        return Ok(HashMap::new());
    }
    
    let mut prices: HashMap<(String, String), MarketPrice> = HashMap::new();
    
    // Find all parquet files
    let parquet_files: Vec<_> = walkdir::WalkDir::new(&snapshot_dir)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| e.path().extension().map(|ext| ext == "parquet").unwrap_or(false))
        .collect();
    
    for entry in parquet_files {
        // Use ParquetReader directly to avoid Hive partitioning issues
        let file = std::fs::File::open(entry.path())
            .with_context(|| format!("Failed to open parquet: {:?}", entry.path()))?;
        let df = polars::prelude::ParquetReader::new(file)
            .finish()
            .with_context(|| format!("Failed to read parquet: {:?}", entry.path()))?;
        
        for row_idx in 0..df.height() {
            let market_id = df.column("market_id")?
                .str()?
                .get(row_idx)
                .unwrap_or("")
                .to_string();
            let outcome_id = df.column("outcome_id")?
                .str()?
                .get(row_idx)
                .unwrap_or("0")
                .to_string();
            let ts = df.column("ts_recv")?
                .i64()?
                .get(row_idx)
                .unwrap_or(0);
            
            let mid = df.column("mid")
                .ok()
                .and_then(|c| c.f64().ok())
                .and_then(|c| c.get(row_idx));
            let best_bid = df.column("best_bid_px")
                .ok()
                .and_then(|c| c.f64().ok())
                .and_then(|c| c.get(row_idx));
            let best_ask = df.column("best_ask_px")
                .ok()
                .and_then(|c| c.f64().ok())
                .and_then(|c| c.get(row_idx));
            
            let key = (market_id, outcome_id);
            
            // Keep latest timestamp
            if let Some(existing) = prices.get(&key) {
                if ts <= existing.ts {
                    continue;
                }
            }
            
            prices.insert(key, MarketPrice {
                ts,
                mid,
                best_bid,
                best_ask,
            });
        }
    }
    
    Ok(prices)
}

/// Detect violations for a set of constraints
pub fn detect_violations(
    constraints: &[Constraint],
    prices: &HashMap<(String, String), MarketPrice>,
    config: &ArbDetectorConfig,
) -> Vec<Violation> {
    let mut violations = Vec::new();
    let now = chrono::Utc::now().timestamp_millis();
    
    for constraint in constraints {
        // Get prices for both markets
        let key_a = (
            constraint.a_market_id.clone(),
            constraint.a_outcome_id.clone().unwrap_or_else(|| "0".to_string()),
        );
        let key_b = (
            constraint.b_market_id.clone(),
            constraint.b_outcome_id.clone().unwrap_or_else(|| "0".to_string()),
        );
        
        let price_a = prices.get(&key_a);
        let price_b = prices.get(&key_b);
        
        let (p_a, p_b) = match (price_a, price_b) {
            (Some(pa), Some(pb)) => {
                match (pa.implied_probability(), pb.implied_probability()) {
                    (Some(a), Some(b)) => (a, b),
                    _ => continue, // Skip if can't compute probabilities
                }
            }
            _ => continue, // Skip if missing price data
        };
        
        // Check for violation
        // Constraint: P(A) <= P(B)
        // Violation: P(A) > P(B) + margin
        if p_a > p_b + config.margin {
            let violation_magnitude = p_a - p_b;
            
            violations.push(Violation {
                ts: now,
                constraint_id: constraint.id.clone(),
                constraint_type: constraint.constraint_type.clone(),
                a_market_id: constraint.a_market_id.clone(),
                a_outcome_id: constraint.a_outcome_id.clone(),
                b_market_id: constraint.b_market_id.clone(),
                b_outcome_id: constraint.b_outcome_id.clone(),
                p_a,
                p_b,
                violation_magnitude,
                margin: config.margin,
                confidence: constraint.confidence,
                a_bid: price_a.and_then(|p| p.best_bid),
                a_ask: price_a.and_then(|p| p.best_ask),
                b_bid: price_b.and_then(|p| p.best_bid),
                b_ask: price_b.and_then(|p| p.best_ask),
            });
        }
    }
    
    violations
}

/// Generate synthetic prices with violations for testing
pub fn generate_mock_prices_with_violations(
    constraints: &[Constraint],
) -> HashMap<(String, String), MarketPrice> {
    let mut prices = HashMap::new();
    let now = chrono::Utc::now().timestamp_millis();
    
    // Create some violations by inverting expected probabilities
    for (i, constraint) in constraints.iter().enumerate() {
        let key_a = (
            constraint.a_market_id.clone(),
            constraint.a_outcome_id.clone().unwrap_or_else(|| "0".to_string()),
        );
        let key_b = (
            constraint.b_market_id.clone(),
            constraint.b_outcome_id.clone().unwrap_or_else(|| "0".to_string()),
        );
        
        // Every other constraint has a violation
        let (p_a, p_b) = if i % 2 == 0 {
            // Violation: P(A) > P(B) when should be P(A) <= P(B)
            (0.6, 0.4)
        } else {
            // Normal: P(A) < P(B)
            (0.3, 0.5)
        };
        
        prices.insert(key_a, MarketPrice {
            ts: now,
            mid: Some(p_a),
            best_bid: Some(p_a - 0.01),
            best_ask: Some(p_a + 0.01),
        });
        
        prices.insert(key_b, MarketPrice {
            ts: now,
            mid: Some(p_b),
            best_bid: Some(p_b - 0.01),
            best_ask: Some(p_b + 0.01),
        });
    }
    
    prices
}

#[cfg(test)]
mod tests {
    use super::*;
    
    fn make_test_constraint() -> Constraint {
        Constraint {
            id: "test-1".to_string(),
            venue: "test".to_string(),
            constraint_type: "monotonic_ladder".to_string(),
            a_market_id: "btc-100k".to_string(),
            a_outcome_id: Some("0".to_string()),
            b_market_id: "btc-90k".to_string(),
            b_outcome_id: Some("0".to_string()),
            relation: "P(BTC >= 100k) <= P(BTC >= 90k)".to_string(),
            confidence: 0.9,
            notes: vec![],
            group_key: "test".to_string(),
        }
    }
    
    #[test]
    fn test_implied_probability() {
        let price = MarketPrice {
            ts: 0,
            mid: Some(0.5),
            best_bid: Some(0.49),
            best_ask: Some(0.51),
        };
        
        assert_eq!(price.implied_probability(), Some(0.5));
    }
    
    #[test]
    fn test_detect_violation() {
        let constraint = make_test_constraint();
        let constraints = vec![constraint];
        
        let mut prices = HashMap::new();
        prices.insert(
            ("btc-100k".to_string(), "0".to_string()),
            MarketPrice {
                ts: 0,
                mid: Some(0.7),  // Higher than should be
                best_bid: Some(0.69),
                best_ask: Some(0.71),
            },
        );
        prices.insert(
            ("btc-90k".to_string(), "0".to_string()),
            MarketPrice {
                ts: 0,
                mid: Some(0.5),
                best_bid: Some(0.49),
                best_ask: Some(0.51),
            },
        );
        
        let config = ArbDetectorConfig::default();
        let violations = detect_violations(&constraints, &prices, &config);
        
        assert_eq!(violations.len(), 1);
        let v = &violations[0];
        assert_eq!(v.p_a, 0.7);
        assert_eq!(v.p_b, 0.5);
        assert!((v.violation_magnitude - 0.2).abs() < 0.001);
    }
    
    #[test]
    fn test_no_violation() {
        let constraint = make_test_constraint();
        let constraints = vec![constraint];
        
        let mut prices = HashMap::new();
        prices.insert(
            ("btc-100k".to_string(), "0".to_string()),
            MarketPrice {
                ts: 0,
                mid: Some(0.3),  // Lower as expected
                best_bid: Some(0.29),
                best_ask: Some(0.31),
            },
        );
        prices.insert(
            ("btc-90k".to_string(), "0".to_string()),
            MarketPrice {
                ts: 0,
                mid: Some(0.5),
                best_bid: Some(0.49),
                best_ask: Some(0.51),
            },
        );
        
        let config = ArbDetectorConfig::default();
        let violations = detect_violations(&constraints, &prices, &config);
        
        assert_eq!(violations.len(), 0);
    }
    
    #[test]
    fn test_mock_prices_with_violations() {
        let constraints = vec![
            make_test_constraint(),
            Constraint {
                id: "test-2".to_string(),
                a_market_id: "btc-110k".to_string(),
                b_market_id: "btc-100k".to_string(),
                ..make_test_constraint()
            },
        ];
        
        let prices = generate_mock_prices_with_violations(&constraints);
        
        // Should have prices for all markets
        assert!(prices.contains_key(&("btc-100k".to_string(), "0".to_string())));
        assert!(prices.contains_key(&("btc-90k".to_string(), "0".to_string())));
    }
}
