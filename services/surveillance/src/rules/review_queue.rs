//! Human-in-the-loop review queue for low confidence propositions

use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::io::Write;
use std::path::Path;
use crate::rules::proposition::NormalizedProposition;
use crate::rules::confidence::REVIEW_THRESHOLD;

/// Review queue item
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReviewItem {
    pub venue: String,
    pub market_id: String,
    pub outcome_id: Option<String>,
    pub title: String,
    pub raw_rules_text: String,
    pub extracted_proposition: serde_json::Value,
    pub confidence: f64,
    pub parse_notes: Vec<String>,
    pub status: ReviewStatus,
    pub created_at: i64,
}

/// Review status
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReviewStatus {
    Pending,
    Approved,
    Rejected,
    Modified,
}

impl Default for ReviewStatus {
    fn default() -> Self {
        ReviewStatus::Pending
    }
}

/// Create review item from proposition and raw rules
pub fn create_review_item(
    proposition: &NormalizedProposition,
    raw_rules_text: &str,
) -> ReviewItem {
    let extracted_proposition = serde_json::to_value(&proposition.proposition)
        .unwrap_or(serde_json::Value::Null);
    
    ReviewItem {
        venue: proposition.venue.clone(),
        market_id: proposition.market_id.clone(),
        outcome_id: proposition.outcome_id.clone(),
        title: proposition.title.clone(),
        raw_rules_text: raw_rules_text.to_string(),
        extracted_proposition,
        confidence: proposition.confidence,
        parse_notes: proposition.parse_notes.clone(),
        status: ReviewStatus::Pending,
        created_at: chrono::Utc::now().timestamp_millis(),
    }
}

/// Filter propositions that need review
pub fn filter_for_review(
    propositions: &[NormalizedProposition],
    threshold: Option<f64>,
) -> Vec<&NormalizedProposition> {
    let threshold = threshold.unwrap_or(REVIEW_THRESHOLD);
    propositions
        .iter()
        .filter(|p| p.confidence < threshold)
        .collect()
}

/// Write review queue to JSONL file
pub fn write_review_queue(
    data_dir: &str,
    venue: &str,
    date: &str,
    items: &[ReviewItem],
) -> Result<()> {
    let dir = Path::new(data_dir)
        .join("review_queue")
        .join(format!("venue={}", venue))
        .join(format!("date={}", date));
    
    std::fs::create_dir_all(&dir)?;
    
    let path = dir.join("queue.jsonl");
    let mut file = std::fs::File::create(&path)?;
    
    for item in items {
        let json = serde_json::to_string(item)?;
        writeln!(file, "{}", json)?;
    }
    
    tracing::info!("Wrote {} review items to {:?}", items.len(), path);
    Ok(())
}

/// Load review queue from JSONL file
pub fn load_review_queue(
    data_dir: &str,
    venue: &str,
    date: &str,
) -> Result<Vec<ReviewItem>> {
    let path = Path::new(data_dir)
        .join("review_queue")
        .join(format!("venue={}", venue))
        .join(format!("date={}", date))
        .join("queue.jsonl");
    
    if !path.exists() {
        return Ok(Vec::new());
    }
    
    let content = std::fs::read_to_string(&path)?;
    let mut items = Vec::new();
    
    for line in content.lines() {
        if line.trim().is_empty() {
            continue;
        }
        let item: ReviewItem = serde_json::from_str(line)?;
        items.push(item);
    }
    
    Ok(items)
}

/// Summary statistics for review queue
#[derive(Debug, Clone, Default)]
pub struct ReviewStats {
    pub total: usize,
    pub pending: usize,
    pub approved: usize,
    pub rejected: usize,
    pub modified: usize,
    pub avg_confidence: f64,
}

impl ReviewStats {
    pub fn from_items(items: &[ReviewItem]) -> Self {
        let total = items.len();
        if total == 0 {
            return Self::default();
        }
        
        let pending = items.iter().filter(|i| i.status == ReviewStatus::Pending).count();
        let approved = items.iter().filter(|i| i.status == ReviewStatus::Approved).count();
        let rejected = items.iter().filter(|i| i.status == ReviewStatus::Rejected).count();
        let modified = items.iter().filter(|i| i.status == ReviewStatus::Modified).count();
        let avg_confidence = items.iter().map(|i| i.confidence).sum::<f64>() / total as f64;
        
        Self {
            total,
            pending,
            approved,
            rejected,
            modified,
            avg_confidence,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rules::proposition::*;
    
    fn make_low_confidence_prop() -> NormalizedProposition {
        NormalizedProposition {
            venue: "test".to_string(),
            market_id: "test-1".to_string(),
            outcome_id: None,
            title: "Unknown event".to_string(),
            raw_rules_hash: "abc".to_string(),
            proposition: PropositionKind::YesNoEvent {
                description: "Something".to_string(),
                window: TimeWindow::default(),
            },
            confidence: 0.3,
            parse_notes: vec!["Low confidence".to_string()],
        }
    }
    
    #[test]
    fn test_filter_for_review() {
        let props = vec![
            make_low_confidence_prop(),
            NormalizedProposition {
                confidence: 0.9,
                ..make_low_confidence_prop()
            },
        ];
        
        let for_review = filter_for_review(&props, None);
        assert_eq!(for_review.len(), 1);
        assert_eq!(for_review[0].confidence, 0.3);
    }
    
    #[test]
    fn test_create_review_item() {
        let prop = make_low_confidence_prop();
        let item = create_review_item(&prop, "Raw rules text here");
        
        assert_eq!(item.market_id, "test-1");
        assert_eq!(item.confidence, 0.3);
        assert_eq!(item.status, ReviewStatus::Pending);
    }
    
    #[test]
    fn test_review_stats() {
        let items = vec![
            ReviewItem {
                venue: "test".to_string(),
                market_id: "1".to_string(),
                outcome_id: None,
                title: "Test".to_string(),
                raw_rules_text: "".to_string(),
                extracted_proposition: serde_json::Value::Null,
                confidence: 0.3,
                parse_notes: vec![],
                status: ReviewStatus::Pending,
                created_at: 0,
            },
            ReviewItem {
                confidence: 0.5,
                status: ReviewStatus::Approved,
                ..Default::default()
            },
        ];
        
        let stats = ReviewStats::from_items(&items);
        assert_eq!(stats.total, 2);
        assert_eq!(stats.pending, 1);
        assert_eq!(stats.approved, 1);
    }
}

impl Default for ReviewItem {
    fn default() -> Self {
        Self {
            venue: String::new(),
            market_id: String::new(),
            outcome_id: None,
            title: String::new(),
            raw_rules_text: String::new(),
            extracted_proposition: serde_json::Value::Null,
            confidence: 0.0,
            parse_notes: Vec::new(),
            status: ReviewStatus::Pending,
            created_at: 0,
        }
    }
}
