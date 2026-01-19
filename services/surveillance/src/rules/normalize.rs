//! Normalize raw rules into canonical propositions

use anyhow::Result;
use crate::rules::proposition::*;
use crate::rules::ingest::RulesRecord;
use crate::rules::extract::{extract_proposition, ExtractionResult};
use crate::rules::confidence::compute_confidence;

/// Normalize a single rules record into a proposition
pub fn normalize_rules(record: &RulesRecord) -> NormalizedProposition {
    let rules_hash = record.rules_hash();
    
    // Extract proposition
    let (proposition, extraction_result) = extract_proposition(record);
    
    // Compute confidence
    let confidence = compute_confidence(&proposition, &extraction_result);
    
    // Build normalized proposition
    let mut np = NormalizedProposition::new(
        &record.venue,
        &record.market_id,
        &record.title,
        &rules_hash,
    );
    
    if let Some(outcome_id) = &record.outcome_id {
        np = np.with_outcome(outcome_id);
    }
    
    np = np.with_proposition(proposition)
        .with_confidence(confidence);
    
    // Add parse notes
    for note in &extraction_result.notes {
        np = np.add_note(note);
    }
    for conflict in &extraction_result.conflicts {
        np = np.add_note(&format!("CONFLICT: {}", conflict));
    }
    
    // If it's a YesNoEvent fallback, use title as description
    if let PropositionKind::YesNoEvent { description: ref _d, window } = np.proposition {
        np.proposition = PropositionKind::YesNoEvent {
            description: record.title.clone(),
            window,
        };
    }
    
    np
}

/// Batch normalize rules records
pub fn normalize_batch(records: &[RulesRecord]) -> Vec<NormalizedProposition> {
    records.iter().map(normalize_rules).collect()
}

/// Filter propositions by confidence threshold
pub fn filter_by_confidence(
    propositions: &[NormalizedProposition],
    min_confidence: f64,
) -> (Vec<NormalizedProposition>, Vec<NormalizedProposition>) {
    let (high, low): (Vec<_>, Vec<_>) = propositions
        .iter()
        .cloned()
        .partition(|p| p.confidence >= min_confidence);
    (high, low)
}

/// Get propositions that are price barriers
pub fn get_price_barriers(propositions: &[NormalizedProposition]) -> Vec<&NormalizedProposition> {
    propositions
        .iter()
        .filter(|p| p.proposition.is_price_barrier())
        .collect()
}

/// Group propositions by underlier
pub fn group_by_underlier(
    propositions: &[NormalizedProposition],
) -> std::collections::HashMap<String, Vec<&NormalizedProposition>> {
    let mut groups: std::collections::HashMap<String, Vec<&NormalizedProposition>> = 
        std::collections::HashMap::new();
    
    for prop in propositions {
        if let Some(underlier) = prop.proposition.underlier() {
            groups
                .entry(underlier.kind.clone())
                .or_default()
                .push(prop);
        }
    }
    
    groups
}

#[cfg(test)]
mod tests {
    use super::*;
    
    fn make_test_record(title: &str, rules: &str) -> RulesRecord {
        RulesRecord {
            venue: "test".to_string(),
            market_id: format!("test-{}", title.len()),
            outcome_id: None,
            url: None,
            fetched_ts: 0,
            title: title.to_string(),
            close_ts: Some(1234567890000),
            raw_rules_text: rules.to_string(),
            raw_resolution_source: None,
            raw_json: None,
        }
    }
    
    #[test]
    fn test_normalize_btc_barrier() {
        let record = make_test_record(
            "BTC >= $100k",
            "This market resolves Yes if Bitcoin is at or above $100000 at any time before close according to Coinbase."
        );
        
        let np = normalize_rules(&record);
        
        assert!(np.proposition.is_price_barrier());
        assert!(np.confidence > 0.6);
        assert!(!np.needs_review());
    }
    
    #[test]
    fn test_normalize_unknown() {
        let record = make_test_record(
            "Random event",
            "Something happens."
        );
        
        let np = normalize_rules(&record);
        
        assert!(matches!(np.proposition, PropositionKind::YesNoEvent { .. }));
        assert!(np.confidence < 0.6);
        assert!(np.needs_review());
    }
    
    #[test]
    fn test_filter_by_confidence() {
        let records = vec![
            make_test_record("BTC >= $100k", "Bitcoin at or above $100000 any time before close on Coinbase"),
            make_test_record("Unknown", "Something"),
        ];
        
        let props = normalize_batch(&records);
        let (high, low) = filter_by_confidence(&props, 0.6);
        
        assert_eq!(high.len(), 1);
        assert_eq!(low.len(), 1);
    }
    
    #[test]
    fn test_group_by_underlier() {
        let records = vec![
            make_test_record("BTC $100k", "Bitcoin above $100000"),
            make_test_record("BTC $110k", "Bitcoin above $110000"),
            make_test_record("ETH $4k", "Ethereum above $4000"),
        ];
        
        let props = normalize_batch(&records);
        let groups = group_by_underlier(&props);
        
        assert_eq!(groups.get("BTC").map(|v| v.len()), Some(2));
        assert_eq!(groups.get("ETH").map(|v| v.len()), Some(1));
    }
}
