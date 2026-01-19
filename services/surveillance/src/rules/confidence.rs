//! Confidence scoring for proposition extraction

use crate::rules::proposition::*;
use crate::rules::extract::ExtractionResult;

/// Compute confidence score for a proposition
/// 
/// Scoring rules:
/// - Base: 0.2
/// - +0.2 if underlier extracted
/// - +0.2 if numeric level extracted
/// - +0.2 if comparator extracted
/// - +0.1 if time window extracted
/// - +0.1 if price measure extracted
/// - +0.1 if source extracted
/// 
/// Penalties:
/// - -0.2 if conflicting phrases
/// - -0.2 if multiple numeric levels (ambiguous)
/// - -0.2 if time window ambiguous
pub fn compute_confidence(
    proposition: &PropositionKind,
    extraction: &ExtractionResult,
) -> f64 {
    let mut score: f64 = 0.2; // Base score
    
    // Additions
    if extraction.underlier.is_some() {
        score += 0.2;
    }
    
    if extraction.level.is_some() {
        score += 0.2;
    }
    
    if extraction.comparator.is_some() {
        score += 0.2;
    }
    
    if !matches!(extraction.window.kind, TimeWindowKind::Unknown) {
        score += 0.1;
    }
    
    if !matches!(extraction.measure, PriceMeasure::Unknown) {
        score += 0.1;
    }
    
    if !matches!(extraction.source, PriceSource::Unknown) {
        score += 0.1;
    }
    
    // Penalties from conflicts
    for conflict in &extraction.conflicts {
        if conflict.contains("time window") {
            score -= 0.2;
        } else if conflict.contains("Multiple price levels") {
            score -= 0.2;
        } else if conflict.contains("Ambiguous") {
            score -= 0.2;
        } else {
            score -= 0.1; // Generic conflict penalty
        }
    }
    
    // Additional proposition-specific adjustments
    match proposition {
        PropositionKind::PriceBarrier { window, measure, source, .. } => {
            // Boost for complete price barriers
            if !matches!(window.kind, TimeWindowKind::Unknown) &&
               !matches!(measure, PriceMeasure::Unknown) &&
               !matches!(source, PriceSource::Unknown) {
                score += 0.1; // Completeness bonus
            }
        }
        PropositionKind::Unknown => {
            score -= 0.3; // Major penalty for unknown
        }
        PropositionKind::YesNoEvent { .. } => {
            // Slight penalty for fallback
            score -= 0.1;
        }
        PropositionKind::RangePartition { .. } => {
            // No adjustment
        }
    }
    
    // Clamp to [0, 1]
    score.clamp(0.0, 1.0)
}

/// Threshold for routing to human review
pub const REVIEW_THRESHOLD: f64 = 0.6;

/// Check if a confidence score needs review
pub fn needs_review(confidence: f64) -> bool {
    confidence < REVIEW_THRESHOLD
}

/// Confidence level classification
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConfidenceLevel {
    High,     // >= 0.8
    Medium,   // >= 0.6
    Low,      // >= 0.4
    VeryLow,  // < 0.4
}

impl ConfidenceLevel {
    pub fn from_score(score: f64) -> Self {
        if score >= 0.8 {
            ConfidenceLevel::High
        } else if score >= 0.6 {
            ConfidenceLevel::Medium
        } else if score >= 0.4 {
            ConfidenceLevel::Low
        } else {
            ConfidenceLevel::VeryLow
        }
    }
    
    pub fn as_str(&self) -> &'static str {
        match self {
            ConfidenceLevel::High => "high",
            ConfidenceLevel::Medium => "medium",
            ConfidenceLevel::Low => "low",
            ConfidenceLevel::VeryLow => "very_low",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    fn make_extraction(
        has_underlier: bool,
        has_level: bool,
        has_comparator: bool,
        has_window: bool,
        has_measure: bool,
        has_source: bool,
    ) -> ExtractionResult {
        let mut result = ExtractionResult::new();
        
        if has_underlier {
            result.underlier = Some(Underlier::new("BTC"));
        }
        if has_level {
            result.level = Some(100000.0);
        }
        if has_comparator {
            result.comparator = Some(Comparator::GE);
        }
        if has_window {
            result.window = TimeWindow::any_time_before(1234567890000);
        }
        if has_measure {
            result.measure = PriceMeasure::Spot;
        }
        if has_source {
            result.source = PriceSource::Exchange("Coinbase".to_string());
        }
        
        result
    }
    
    #[test]
    fn test_base_score() {
        let extraction = make_extraction(false, false, false, false, false, false);
        let prop = PropositionKind::Unknown;
        let score = compute_confidence(&prop, &extraction);
        
        // Base 0.2 - 0.3 (unknown penalty) = clamped to 0.0
        assert!(score < 0.2);
    }
    
    #[test]
    fn test_full_extraction_score() {
        let extraction = make_extraction(true, true, true, true, true, true);
        let prop = PropositionKind::PriceBarrier {
            underlier: Underlier::new("BTC"),
            comparator: Comparator::GE,
            level: 100000.0,
            measure: PriceMeasure::Spot,
            window: TimeWindow::any_time_before(1234567890000),
            source: PriceSource::Exchange("Coinbase".to_string()),
        };
        let score = compute_confidence(&prop, &extraction);
        
        // 0.2 base + 0.2 + 0.2 + 0.2 + 0.1 + 0.1 + 0.1 + 0.1 (completeness) = 1.2 clamped to 1.0
        assert_eq!(score, 1.0);
    }
    
    #[test]
    fn test_partial_extraction_score() {
        let extraction = make_extraction(true, true, true, false, false, false);
        let prop = PropositionKind::PriceBarrier {
            underlier: Underlier::new("BTC"),
            comparator: Comparator::GE,
            level: 100000.0,
            measure: PriceMeasure::Unknown,
            window: TimeWindow::default(),
            source: PriceSource::Unknown,
        };
        let score = compute_confidence(&prop, &extraction);
        
        // 0.2 base + 0.2 + 0.2 + 0.2 = 0.8
        assert!(score >= 0.7 && score <= 0.9);
    }
    
    #[test]
    fn test_conflict_penalty() {
        // Without conflict
        let extraction_clean = make_extraction(true, true, true, true, true, true);
        let prop = PropositionKind::PriceBarrier {
            underlier: Underlier::new("BTC"),
            comparator: Comparator::GE,
            level: 100000.0,
            measure: PriceMeasure::Spot,
            window: TimeWindow::any_time_before(1234567890000),
            source: PriceSource::Exchange("Coinbase".to_string()),
        };
        let score_clean = compute_confidence(&prop, &extraction_clean);
        
        // With conflict
        let mut extraction_conflict = make_extraction(true, true, true, true, true, true);
        extraction_conflict.conflicts.push("Conflicting time window".to_string());
        extraction_conflict.conflicts.push("Multiple price levels".to_string());
        
        let score_conflict = compute_confidence(&prop, &extraction_conflict);
        
        // Conflict should reduce score
        assert!(score_conflict < score_clean);
        assert!(score_conflict <= 1.0);
    }
    
    #[test]
    fn test_confidence_level() {
        assert_eq!(ConfidenceLevel::from_score(0.9), ConfidenceLevel::High);
        assert_eq!(ConfidenceLevel::from_score(0.7), ConfidenceLevel::Medium);
        assert_eq!(ConfidenceLevel::from_score(0.5), ConfidenceLevel::Low);
        assert_eq!(ConfidenceLevel::from_score(0.2), ConfidenceLevel::VeryLow);
    }
    
    #[test]
    fn test_needs_review() {
        assert!(!needs_review(0.8));
        assert!(!needs_review(0.6));
        assert!(needs_review(0.59));
        assert!(needs_review(0.3));
    }
}
