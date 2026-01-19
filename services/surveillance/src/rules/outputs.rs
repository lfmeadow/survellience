//! Output writing for rules pipeline (Parquet and JSONL)

use anyhow::Result;
use polars::prelude::*;
use std::path::Path;
use crate::rules::proposition::*;
use crate::rules::constraints::Constraint;
use crate::rules::arb_detector::Violation;

/// Write normalized propositions to Parquet
pub fn write_propositions_parquet(
    data_dir: &str,
    venue: &str,
    date: &str,
    propositions: &[NormalizedProposition],
) -> Result<()> {
    if propositions.is_empty() {
        tracing::info!("No propositions to write");
        return Ok(());
    }
    
    let dir = Path::new(data_dir)
        .join("logic")
        .join(format!("venue={}", venue))
        .join(format!("date={}", date));
    
    std::fs::create_dir_all(&dir)?;
    
    let path = dir.join("propositions.parquet");
    
    // Build columns
    let venue_col: Vec<&str> = propositions.iter().map(|p| p.venue.as_str()).collect();
    let market_id_col: Vec<&str> = propositions.iter().map(|p| p.market_id.as_str()).collect();
    let outcome_id_col: Vec<Option<&str>> = propositions.iter()
        .map(|p| p.outcome_id.as_deref())
        .collect();
    let title_col: Vec<&str> = propositions.iter().map(|p| p.title.as_str()).collect();
    let raw_rules_hash_col: Vec<&str> = propositions.iter().map(|p| p.raw_rules_hash.as_str()).collect();
    let confidence_col: Vec<f64> = propositions.iter().map(|p| p.confidence).collect();
    
    // Serialize proposition to JSON string
    let proposition_json_col: Vec<String> = propositions.iter()
        .map(|p| serde_json::to_string(&p.proposition).unwrap_or_default())
        .collect();
    
    // Extract proposition type
    let proposition_type_col: Vec<&str> = propositions.iter()
        .map(|p| match &p.proposition {
            PropositionKind::Unknown => "unknown",
            PropositionKind::PriceBarrier { .. } => "price_barrier",
            PropositionKind::YesNoEvent { .. } => "yes_no_event",
            PropositionKind::RangePartition { .. } => "range_partition",
        })
        .collect();
    
    // Extract underlier if available
    let underlier_col: Vec<Option<String>> = propositions.iter()
        .map(|p| p.proposition.underlier().map(|u| u.kind.clone()))
        .collect();
    
    // Extract strike level for price barriers
    let strike_col: Vec<Option<f64>> = propositions.iter()
        .map(|p| {
            if let PropositionKind::PriceBarrier { level, .. } = &p.proposition {
                Some(*level)
            } else {
                None
            }
        })
        .collect();
    
    // Extract comparator
    let comparator_col: Vec<Option<String>> = propositions.iter()
        .map(|p| {
            if let PropositionKind::PriceBarrier { comparator, .. } = &p.proposition {
                Some(format!("{:?}", comparator))
            } else {
                None
            }
        })
        .collect();
    
    // Extract window end timestamp
    let window_end_col: Vec<Option<i64>> = propositions.iter()
        .map(|p| p.proposition.time_window().and_then(|w| w.end_ts))
        .collect();
    
    // Parse notes as JSON array
    let notes_col: Vec<String> = propositions.iter()
        .map(|p| serde_json::to_string(&p.parse_notes).unwrap_or_default())
        .collect();
    
    let df = DataFrame::new(vec![
        Series::new("venue", venue_col),
        Series::new("market_id", market_id_col),
        Series::new("outcome_id", outcome_id_col),
        Series::new("title", title_col),
        Series::new("raw_rules_hash", raw_rules_hash_col),
        Series::new("confidence", confidence_col),
        Series::new("proposition_type", proposition_type_col),
        Series::new("proposition_json", proposition_json_col),
        Series::new("underlier", underlier_col),
        Series::new("strike", strike_col),
        Series::new("comparator", comparator_col),
        Series::new("window_end_ts", window_end_col),
        Series::new("parse_notes", notes_col),
    ])?;
    
    let file = std::fs::File::create(&path)?;
    ParquetWriter::new(file).finish(&mut df.clone())?;
    
    tracing::info!("Wrote {} propositions to {:?}", propositions.len(), path);
    Ok(())
}

/// Write constraints to Parquet
pub fn write_constraints_parquet(
    data_dir: &str,
    venue: &str,
    date: &str,
    constraints: &[Constraint],
) -> Result<()> {
    if constraints.is_empty() {
        tracing::info!("No constraints to write");
        return Ok(());
    }
    
    let dir = Path::new(data_dir)
        .join("logic")
        .join(format!("venue={}", venue))
        .join(format!("date={}", date));
    
    std::fs::create_dir_all(&dir)?;
    
    let path = dir.join("constraints.parquet");
    
    // Build columns
    let id_col: Vec<&str> = constraints.iter().map(|c| c.id.as_str()).collect();
    let venue_col: Vec<&str> = constraints.iter().map(|c| c.venue.as_str()).collect();
    let constraint_type_col: Vec<&str> = constraints.iter().map(|c| c.constraint_type.as_str()).collect();
    let a_market_id_col: Vec<&str> = constraints.iter().map(|c| c.a_market_id.as_str()).collect();
    let a_outcome_id_col: Vec<Option<&str>> = constraints.iter()
        .map(|c| c.a_outcome_id.as_deref())
        .collect();
    let b_market_id_col: Vec<&str> = constraints.iter().map(|c| c.b_market_id.as_str()).collect();
    let b_outcome_id_col: Vec<Option<&str>> = constraints.iter()
        .map(|c| c.b_outcome_id.as_deref())
        .collect();
    let relation_col: Vec<&str> = constraints.iter().map(|c| c.relation.as_str()).collect();
    let confidence_col: Vec<f64> = constraints.iter().map(|c| c.confidence).collect();
    let group_key_col: Vec<&str> = constraints.iter().map(|c| c.group_key.as_str()).collect();
    let notes_col: Vec<String> = constraints.iter()
        .map(|c| serde_json::to_string(&c.notes).unwrap_or_default())
        .collect();
    
    let df = DataFrame::new(vec![
        Series::new("id", id_col),
        Series::new("venue", venue_col),
        Series::new("constraint_type", constraint_type_col),
        Series::new("a_market_id", a_market_id_col),
        Series::new("a_outcome_id", a_outcome_id_col),
        Series::new("b_market_id", b_market_id_col),
        Series::new("b_outcome_id", b_outcome_id_col),
        Series::new("relation", relation_col),
        Series::new("confidence", confidence_col),
        Series::new("group_key", group_key_col),
        Series::new("notes", notes_col),
    ])?;
    
    let file = std::fs::File::create(&path)?;
    ParquetWriter::new(file).finish(&mut df.clone())?;
    
    tracing::info!("Wrote {} constraints to {:?}", constraints.len(), path);
    Ok(())
}

/// Write violations to Parquet
pub fn write_violations_parquet(
    data_dir: &str,
    venue: &str,
    date: &str,
    violations: &[Violation],
) -> Result<()> {
    if violations.is_empty() {
        tracing::info!("No violations to write");
        return Ok(());
    }
    
    let dir = Path::new(data_dir)
        .join("logic")
        .join(format!("venue={}", venue))
        .join(format!("date={}", date));
    
    std::fs::create_dir_all(&dir)?;
    
    let path = dir.join("violations.parquet");
    
    // Build columns
    let ts_col: Vec<i64> = violations.iter().map(|v| v.ts).collect();
    let constraint_id_col: Vec<&str> = violations.iter().map(|v| v.constraint_id.as_str()).collect();
    let constraint_type_col: Vec<&str> = violations.iter().map(|v| v.constraint_type.as_str()).collect();
    let a_market_id_col: Vec<&str> = violations.iter().map(|v| v.a_market_id.as_str()).collect();
    let a_outcome_id_col: Vec<Option<&str>> = violations.iter()
        .map(|v| v.a_outcome_id.as_deref())
        .collect();
    let b_market_id_col: Vec<&str> = violations.iter().map(|v| v.b_market_id.as_str()).collect();
    let b_outcome_id_col: Vec<Option<&str>> = violations.iter()
        .map(|v| v.b_outcome_id.as_deref())
        .collect();
    let p_a_col: Vec<f64> = violations.iter().map(|v| v.p_a).collect();
    let p_b_col: Vec<f64> = violations.iter().map(|v| v.p_b).collect();
    let violation_magnitude_col: Vec<f64> = violations.iter().map(|v| v.violation_magnitude).collect();
    let margin_col: Vec<f64> = violations.iter().map(|v| v.margin).collect();
    let confidence_col: Vec<f64> = violations.iter().map(|v| v.confidence).collect();
    let a_bid_col: Vec<Option<f64>> = violations.iter().map(|v| v.a_bid).collect();
    let a_ask_col: Vec<Option<f64>> = violations.iter().map(|v| v.a_ask).collect();
    let b_bid_col: Vec<Option<f64>> = violations.iter().map(|v| v.b_bid).collect();
    let b_ask_col: Vec<Option<f64>> = violations.iter().map(|v| v.b_ask).collect();
    
    let df = DataFrame::new(vec![
        Series::new("ts", ts_col),
        Series::new("constraint_id", constraint_id_col),
        Series::new("constraint_type", constraint_type_col),
        Series::new("a_market_id", a_market_id_col),
        Series::new("a_outcome_id", a_outcome_id_col),
        Series::new("b_market_id", b_market_id_col),
        Series::new("b_outcome_id", b_outcome_id_col),
        Series::new("p_a", p_a_col),
        Series::new("p_b", p_b_col),
        Series::new("violation_magnitude", violation_magnitude_col),
        Series::new("margin", margin_col),
        Series::new("confidence", confidence_col),
        Series::new("a_bid", a_bid_col),
        Series::new("a_ask", a_ask_col),
        Series::new("b_bid", b_bid_col),
        Series::new("b_ask", b_ask_col),
    ])?;
    
    let file = std::fs::File::create(&path)?;
    ParquetWriter::new(file).finish(&mut df.clone())?;
    
    tracing::info!("Wrote {} violations to {:?}", violations.len(), path);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;
    
    #[test]
    fn test_write_propositions() {
        let temp_dir = TempDir::new().unwrap();
        let data_dir = temp_dir.path().to_str().unwrap();
        
        let props = vec![NormalizedProposition {
            venue: "test".to_string(),
            market_id: "test-1".to_string(),
            outcome_id: None,
            title: "Test".to_string(),
            raw_rules_hash: "abc".to_string(),
            proposition: PropositionKind::PriceBarrier {
                underlier: Underlier::new("BTC"),
                comparator: Comparator::GE,
                level: 100000.0,
                measure: PriceMeasure::Spot,
                window: TimeWindow::any_time_before(1234567890000),
                source: PriceSource::Exchange("Coinbase".to_string()),
            },
            confidence: 0.9,
            parse_notes: vec!["Test note".to_string()],
        }];
        
        write_propositions_parquet(data_dir, "test", "2026-01-19", &props).unwrap();
        
        let path = temp_dir.path()
            .join("logic/venue=test/date=2026-01-19/propositions.parquet");
        assert!(path.exists());
    }
    
    #[test]
    fn test_write_constraints() {
        let temp_dir = TempDir::new().unwrap();
        let data_dir = temp_dir.path().to_str().unwrap();
        
        let constraints = vec![Constraint {
            id: "test-1".to_string(),
            venue: "test".to_string(),
            constraint_type: "monotonic_ladder".to_string(),
            a_market_id: "a".to_string(),
            a_outcome_id: None,
            b_market_id: "b".to_string(),
            b_outcome_id: None,
            relation: "P(A) <= P(B)".to_string(),
            confidence: 0.9,
            notes: vec![],
            group_key: "test".to_string(),
        }];
        
        write_constraints_parquet(data_dir, "test", "2026-01-19", &constraints).unwrap();
        
        let path = temp_dir.path()
            .join("logic/venue=test/date=2026-01-19/constraints.parquet");
        assert!(path.exists());
    }
    
    #[test]
    fn test_write_violations() {
        let temp_dir = TempDir::new().unwrap();
        let data_dir = temp_dir.path().to_str().unwrap();
        
        let violations = vec![Violation {
            ts: 1234567890000,
            constraint_id: "test-1".to_string(),
            constraint_type: "monotonic_ladder".to_string(),
            a_market_id: "a".to_string(),
            a_outcome_id: None,
            b_market_id: "b".to_string(),
            b_outcome_id: None,
            p_a: 0.6,
            p_b: 0.4,
            violation_magnitude: 0.2,
            margin: 0.01,
            confidence: 0.9,
            a_bid: Some(0.59),
            a_ask: Some(0.61),
            b_bid: Some(0.39),
            b_ask: Some(0.41),
        }];
        
        write_violations_parquet(data_dir, "test", "2026-01-19", &violations).unwrap();
        
        let path = temp_dir.path()
            .join("logic/venue=test/date=2026-01-19/violations.parquet");
        assert!(path.exists());
    }
}
