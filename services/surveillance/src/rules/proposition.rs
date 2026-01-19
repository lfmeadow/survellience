//! Core proposition types for normalized market rules

use serde::{Deserialize, Serialize};

/// Source of price data for resolution
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum PriceSource {
    Unknown,
    Exchange(String),    // e.g. "Coinbase", "Binance"
    Index(String),       // e.g. "CoinGecko", "CoinMarketCap"
    VenueDefined(String), // venue-specific definition
}

impl Default for PriceSource {
    fn default() -> Self {
        Self::Unknown
    }
}

/// How price is measured
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum PriceMeasure {
    Unknown,
    Spot,
    Close,
    VWAP,
    TWAP,
}

impl Default for PriceMeasure {
    fn default() -> Self {
        Self::Unknown
    }
}

/// Comparison operator
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Comparator {
    GE, // >=
    GT, // >
    LE, // <=
    LT, // <
}

impl Comparator {
    /// Returns true if this is an upward comparator (GE/GT)
    pub fn is_upward(&self) -> bool {
        matches!(self, Comparator::GE | Comparator::GT)
    }
    
    /// Returns true if this is a downward comparator (LE/LT)
    pub fn is_downward(&self) -> bool {
        matches!(self, Comparator::LE | Comparator::LT)
    }
}

/// Kind of time window for evaluation
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum TimeWindowKind {
    Unknown,
    AnyTimeBefore,   // barrier crossing before deadline
    AtClose,         // evaluated at close timestamp
    AtTime,          // evaluated at a specific timestamp
    DuringInterval,  // evaluated during interval
}

impl Default for TimeWindowKind {
    fn default() -> Self {
        Self::Unknown
    }
}

/// Time window specification
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct TimeWindow {
    pub kind: TimeWindowKind,
    pub start_ts: Option<i64>,  // epoch ms
    pub end_ts: Option<i64>,    // deadline epoch ms
}

impl TimeWindow {
    pub fn unknown() -> Self {
        Self::default()
    }
    
    pub fn any_time_before(end_ts: i64) -> Self {
        Self {
            kind: TimeWindowKind::AnyTimeBefore,
            start_ts: None,
            end_ts: Some(end_ts),
        }
    }
    
    pub fn at_time(ts: i64) -> Self {
        Self {
            kind: TimeWindowKind::AtTime,
            start_ts: None,
            end_ts: Some(ts),
        }
    }
    
    pub fn at_close(ts: i64) -> Self {
        Self {
            kind: TimeWindowKind::AtClose,
            start_ts: None,
            end_ts: Some(ts),
        }
    }
}

/// Underlying asset specification
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Underlier {
    pub kind: String,              // "BTC", "ETH", "SP500", etc.
    pub venue_symbol: Option<String>,
}

impl Underlier {
    pub fn new(kind: &str) -> Self {
        Self {
            kind: kind.to_uppercase(),
            venue_symbol: None,
        }
    }
    
    pub fn with_symbol(kind: &str, symbol: &str) -> Self {
        Self {
            kind: kind.to_uppercase(),
            venue_symbol: Some(symbol.to_string()),
        }
    }
}

/// Kind of proposition extracted from market rules
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum PropositionKind {
    Unknown,
    PriceBarrier {
        underlier: Underlier,
        comparator: Comparator,
        level: f64,
        measure: PriceMeasure,
        window: TimeWindow,
        source: PriceSource,
    },
    YesNoEvent {
        description: String,
        window: TimeWindow,
    },
    RangePartition {
        underlier: Underlier,
        low: Option<f64>,
        high: Option<f64>,
        window: TimeWindow,
        source: PriceSource,
    },
}

impl Default for PropositionKind {
    fn default() -> Self {
        Self::Unknown
    }
}

impl PropositionKind {
    /// Extract underlier if this is a price-related proposition
    pub fn underlier(&self) -> Option<&Underlier> {
        match self {
            PropositionKind::PriceBarrier { underlier, .. } => Some(underlier),
            PropositionKind::RangePartition { underlier, .. } => Some(underlier),
            _ => None,
        }
    }
    
    /// Extract time window
    pub fn time_window(&self) -> Option<&TimeWindow> {
        match self {
            PropositionKind::PriceBarrier { window, .. } => Some(window),
            PropositionKind::YesNoEvent { window, .. } => Some(window),
            PropositionKind::RangePartition { window, .. } => Some(window),
            PropositionKind::Unknown => None,
        }
    }
    
    /// Check if this is a price barrier proposition
    pub fn is_price_barrier(&self) -> bool {
        matches!(self, PropositionKind::PriceBarrier { .. })
    }
}

/// Normalized proposition with metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NormalizedProposition {
    pub venue: String,
    pub market_id: String,
    pub outcome_id: Option<String>,
    pub title: String,
    pub raw_rules_hash: String,     // sha256 of raw text
    pub proposition: PropositionKind,
    pub confidence: f64,            // 0..1
    pub parse_notes: Vec<String>,   // reasons/warnings
}

impl NormalizedProposition {
    pub fn new(
        venue: &str,
        market_id: &str,
        title: &str,
        raw_rules_hash: &str,
    ) -> Self {
        Self {
            venue: venue.to_string(),
            market_id: market_id.to_string(),
            outcome_id: None,
            title: title.to_string(),
            raw_rules_hash: raw_rules_hash.to_string(),
            proposition: PropositionKind::Unknown,
            confidence: 0.0,
            parse_notes: Vec::new(),
        }
    }
    
    pub fn with_outcome(mut self, outcome_id: &str) -> Self {
        self.outcome_id = Some(outcome_id.to_string());
        self
    }
    
    pub fn with_proposition(mut self, prop: PropositionKind) -> Self {
        self.proposition = prop;
        self
    }
    
    pub fn with_confidence(mut self, confidence: f64) -> Self {
        self.confidence = confidence.clamp(0.0, 1.0);
        self
    }
    
    pub fn add_note(mut self, note: &str) -> Self {
        self.parse_notes.push(note.to_string());
        self
    }
    
    /// Check if this proposition needs human review
    pub fn needs_review(&self) -> bool {
        self.confidence < 0.6
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_comparator_direction() {
        assert!(Comparator::GE.is_upward());
        assert!(Comparator::GT.is_upward());
        assert!(Comparator::LE.is_downward());
        assert!(Comparator::LT.is_downward());
    }
    
    #[test]
    fn test_price_barrier() {
        let prop = PropositionKind::PriceBarrier {
            underlier: Underlier::new("BTC"),
            comparator: Comparator::GE,
            level: 100000.0,
            measure: PriceMeasure::Spot,
            window: TimeWindow::any_time_before(1234567890000),
            source: PriceSource::Exchange("Coinbase".to_string()),
        };
        
        assert!(prop.is_price_barrier());
        assert_eq!(prop.underlier().unwrap().kind, "BTC");
    }
    
    #[test]
    fn test_normalized_proposition() {
        let np = NormalizedProposition::new("polymarket", "0x123", "BTC > 100k", "abc123")
            .with_confidence(0.8)
            .add_note("Parsed successfully");
        
        assert!(!np.needs_review());
        assert_eq!(np.confidence, 0.8);
    }
}
