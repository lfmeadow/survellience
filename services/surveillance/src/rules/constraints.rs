//! Constraint generation from normalized propositions

use sha2::{Sha256, Digest};
use serde::{Deserialize, Serialize};
use crate::rules::proposition::*;

/// A logical constraint between two markets
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Constraint {
    pub id: String,
    pub venue: String,
    pub constraint_type: String,        // "monotonic_ladder"
    pub a_market_id: String,
    pub a_outcome_id: Option<String>,
    pub b_market_id: String,
    pub b_outcome_id: Option<String>,
    pub relation: String,               // "P(A) <= P(B)"
    pub confidence: f64,
    pub notes: Vec<String>,
    pub group_key: String,              // hash of shared attributes
}

impl Constraint {
    /// Generate constraint ID from components
    pub fn generate_id(venue: &str, a_market: &str, b_market: &str, constraint_type: &str) -> String {
        let mut hasher = Sha256::new();
        hasher.update(venue.as_bytes());
        hasher.update(a_market.as_bytes());
        hasher.update(b_market.as_bytes());
        hasher.update(constraint_type.as_bytes());
        format!("{:x}", hasher.finalize())[..16].to_string()
    }
}

/// Key for grouping related propositions
#[derive(Debug, Clone, Hash, PartialEq, Eq)]
pub struct LadderGroupKey {
    pub underlier: String,
    pub comparator_direction: String,  // "up" or "down"
    pub window_kind: String,
    pub window_end_bucket: Option<i64>, // end_ts / 300000 (5-min buckets)
    pub source: String,
    pub measure: String,
}

impl LadderGroupKey {
    pub fn from_proposition(prop: &PropositionKind) -> Option<Self> {
        if let PropositionKind::PriceBarrier { 
            underlier, comparator, window, source, measure, .. 
        } = prop {
            let direction = if comparator.is_upward() { "up" } else { "down" };
            let window_kind = format!("{:?}", window.kind);
            let window_end_bucket = window.end_ts.map(|ts| ts / 300000); // 5-min buckets
            let source_str = format!("{:?}", source);
            let measure_str = format!("{:?}", measure);
            
            Some(Self {
                underlier: underlier.kind.clone(),
                comparator_direction: direction.to_string(),
                window_kind,
                window_end_bucket,
                source: source_str,
                measure: measure_str,
            })
        } else {
            None
        }
    }
    
    pub fn to_hash(&self) -> String {
        let mut hasher = Sha256::new();
        hasher.update(self.underlier.as_bytes());
        hasher.update(self.comparator_direction.as_bytes());
        hasher.update(self.window_kind.as_bytes());
        if let Some(bucket) = self.window_end_bucket {
            hasher.update(bucket.to_le_bytes());
        }
        hasher.update(self.source.as_bytes());
        hasher.update(self.measure.as_bytes());
        format!("{:x}", hasher.finalize())[..16].to_string()
    }
}

/// Generate monotonic ladder constraints
/// 
/// For comparator GE/GT (upward):
///   If strike2 > strike1 then P(strike2) <= P(strike1)
///   (Harder condition has lower probability)
/// 
/// For comparator LE/LT (downward):
///   If strike2 < strike1 then P(strike2) <= P(strike1)
///   (Harder condition has lower probability)
pub fn generate_monotonic_ladder_constraints(
    propositions: &[NormalizedProposition],
) -> Vec<Constraint> {
    use std::collections::HashMap;
    
    let mut constraints = Vec::new();
    
    // Group propositions by ladder key
    let mut groups: HashMap<LadderGroupKey, Vec<&NormalizedProposition>> = HashMap::new();
    
    for prop in propositions {
        if let Some(key) = LadderGroupKey::from_proposition(&prop.proposition) {
            groups.entry(key).or_default().push(prop);
        }
    }
    
    // Generate constraints within each group
    for (key, group) in &groups {
        if group.len() < 2 {
            continue;
        }
        
        let group_key = key.to_hash();
        
        // Extract strikes and sort
        let mut strikes: Vec<(&NormalizedProposition, f64)> = group
            .iter()
            .filter_map(|p| {
                if let PropositionKind::PriceBarrier { level, .. } = &p.proposition {
                    Some((*p, *level))
                } else {
                    None
                }
            })
            .collect();
        
        strikes.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal));
        
        // Generate pairwise constraints
        let is_upward = key.comparator_direction == "up";
        
        for i in 0..strikes.len() {
            for j in (i + 1)..strikes.len() {
                let (prop_low, strike_low) = &strikes[i];
                let (prop_high, strike_high) = &strikes[j];
                
                // For upward comparators (GE/GT):
                //   Higher strike is harder => P(high) <= P(low)
                // For downward comparators (LE/LT):
                //   Lower strike is harder => P(low) <= P(high)
                
                let (harder_prop, easier_prop, harder_strike, easier_strike) = if is_upward {
                    (*prop_high, *prop_low, *strike_high, *strike_low)
                } else {
                    (*prop_low, *prop_high, *strike_low, *strike_high)
                };
                
                let constraint_confidence = (harder_prop.confidence + easier_prop.confidence) / 2.0;
                
                let relation = format!(
                    "P({} {} ${:.0}) <= P({} {} ${:.0})",
                    key.underlier,
                    if is_upward { ">=" } else { "<=" },
                    harder_strike,
                    key.underlier,
                    if is_upward { ">=" } else { "<=" },
                    easier_strike
                );
                
                let constraint = Constraint {
                    id: Constraint::generate_id(
                        &harder_prop.venue,
                        &harder_prop.market_id,
                        &easier_prop.market_id,
                        "monotonic_ladder",
                    ),
                    venue: harder_prop.venue.clone(),
                    constraint_type: "monotonic_ladder".to_string(),
                    a_market_id: harder_prop.market_id.clone(),
                    a_outcome_id: harder_prop.outcome_id.clone(),
                    b_market_id: easier_prop.market_id.clone(),
                    b_outcome_id: easier_prop.outcome_id.clone(),
                    relation,
                    confidence: constraint_confidence,
                    notes: vec![
                        format!("Underlier: {}", key.underlier),
                        format!("Window: {:?}", key.window_kind),
                        format!("Direction: {}", key.comparator_direction),
                    ],
                    group_key: group_key.clone(),
                };
                
                constraints.push(constraint);
            }
        }
    }
    
    constraints
}

/// Configuration for constraint generation
#[derive(Debug, Clone)]
pub struct ConstraintConfig {
    pub min_confidence: f64,
    pub time_tolerance_ms: i64,  // tolerance for matching end timestamps
}

impl Default for ConstraintConfig {
    fn default() -> Self {
        Self {
            min_confidence: 0.5,
            time_tolerance_ms: 300000, // 5 minutes
        }
    }
}

/// Generate all constraints from propositions
pub fn generate_constraints(
    propositions: &[NormalizedProposition],
    config: &ConstraintConfig,
) -> Vec<Constraint> {
    // Filter by minimum confidence
    let filtered: Vec<_> = propositions
        .iter()
        .filter(|p| p.confidence >= config.min_confidence)
        .cloned()
        .collect();
    
    let mut constraints = Vec::new();
    
    // Generate monotonic ladder constraints
    constraints.extend(generate_monotonic_ladder_constraints(&filtered));
    
    // TODO: Add other constraint types
    // - Sum constraints (outcomes must sum to 1)
    // - Exclusive constraints (mutually exclusive events)
    // - Implication constraints (A implies B)
    
    constraints
}

#[cfg(test)]
mod tests {
    use super::*;
    
    fn make_btc_prop(market_id: &str, strike: f64, comparator: Comparator) -> NormalizedProposition {
        NormalizedProposition {
            venue: "test".to_string(),
            market_id: market_id.to_string(),
            outcome_id: None,
            title: format!("BTC {} ${:.0}", if comparator.is_upward() { ">=" } else { "<=" }, strike),
            raw_rules_hash: "test".to_string(),
            proposition: PropositionKind::PriceBarrier {
                underlier: Underlier::new("BTC"),
                comparator,
                level: strike,
                measure: PriceMeasure::Spot,
                window: TimeWindow::any_time_before(1234567890000),
                source: PriceSource::Exchange("Coinbase".to_string()),
            },
            confidence: 0.9,
            parse_notes: vec![],
        }
    }
    
    #[test]
    fn test_ladder_group_key() {
        let prop = make_btc_prop("1", 100000.0, Comparator::GE);
        let key = LadderGroupKey::from_proposition(&prop.proposition).unwrap();
        
        assert_eq!(key.underlier, "BTC");
        assert_eq!(key.comparator_direction, "up");
    }
    
    #[test]
    fn test_monotonic_ladder_constraints() {
        let props = vec![
            make_btc_prop("btc-80k", 80000.0, Comparator::GE),
            make_btc_prop("btc-90k", 90000.0, Comparator::GE),
            make_btc_prop("btc-100k", 100000.0, Comparator::GE),
        ];
        
        let constraints = generate_monotonic_ladder_constraints(&props);
        
        // Should have 3 constraints: 80-90, 80-100, 90-100
        assert_eq!(constraints.len(), 3);
        
        // Each constraint should have P(higher) <= P(lower)
        for c in &constraints {
            assert_eq!(c.constraint_type, "monotonic_ladder");
            assert!(c.relation.contains("<="));
        }
    }
    
    #[test]
    fn test_downward_ladder() {
        let props = vec![
            make_btc_prop("btc-le-80k", 80000.0, Comparator::LE),
            make_btc_prop("btc-le-90k", 90000.0, Comparator::LE),
            make_btc_prop("btc-le-100k", 100000.0, Comparator::LE),
        ];
        
        let constraints = generate_monotonic_ladder_constraints(&props);
        
        assert_eq!(constraints.len(), 3);
    }
    
    #[test]
    fn test_separate_groups() {
        let props = vec![
            make_btc_prop("btc-80k", 80000.0, Comparator::GE),
            make_btc_prop("btc-90k", 90000.0, Comparator::GE),
            // Different underlier
            NormalizedProposition {
                venue: "test".to_string(),
                market_id: "eth-4k".to_string(),
                outcome_id: None,
                title: "ETH >= $4000".to_string(),
                raw_rules_hash: "test".to_string(),
                proposition: PropositionKind::PriceBarrier {
                    underlier: Underlier::new("ETH"),
                    comparator: Comparator::GE,
                    level: 4000.0,
                    measure: PriceMeasure::Spot,
                    window: TimeWindow::any_time_before(1234567890000),
                    source: PriceSource::Exchange("Coinbase".to_string()),
                },
                confidence: 0.9,
                parse_notes: vec![],
            },
        ];
        
        let constraints = generate_monotonic_ladder_constraints(&props);
        
        // Only BTC pair should generate constraint
        assert_eq!(constraints.len(), 1);
    }
}
