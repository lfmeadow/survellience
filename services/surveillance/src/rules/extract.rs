//! Extract propositions from raw rules text using deterministic parsing

use regex::Regex;
use crate::rules::proposition::*;
use crate::rules::ingest::RulesRecord;

/// Extraction result with intermediate parse state
#[derive(Debug, Default)]
pub struct ExtractionResult {
    pub underlier: Option<Underlier>,
    pub comparator: Option<Comparator>,
    pub level: Option<f64>,
    pub measure: PriceMeasure,
    pub window: TimeWindow,
    pub source: PriceSource,
    pub notes: Vec<String>,
    pub conflicts: Vec<String>,
}

impl ExtractionResult {
    pub fn new() -> Self {
        Self::default()
    }
    
    pub fn add_note(&mut self, note: &str) {
        self.notes.push(note.to_string());
    }
    
    pub fn add_conflict(&mut self, conflict: &str) {
        self.conflicts.push(conflict.to_string());
    }
}

/// Extract proposition from rules record
pub fn extract_proposition(record: &RulesRecord) -> (PropositionKind, ExtractionResult) {
    let text = &record.raw_rules_text;
    let text_lower = text.to_lowercase();
    let text_normalized = normalize_text(&text_lower);
    
    let mut result = ExtractionResult::new();
    
    // Extract underlier
    result.underlier = extract_underlier(&text_normalized);
    if result.underlier.is_some() {
        result.add_note("Underlier extracted");
    }
    
    // Extract numeric level
    result.level = extract_level(&text_normalized);
    if result.level.is_some() {
        result.add_note("Price level extracted");
    }
    
    // Extract comparator
    result.comparator = extract_comparator(&text_normalized);
    if result.comparator.is_some() {
        result.add_note("Comparator extracted");
    }
    
    // Extract price measure
    result.measure = extract_measure(&text_normalized);
    if !matches!(result.measure, PriceMeasure::Unknown) {
        result.add_note("Price measure extracted");
    }
    
    // Extract time window
    result.window = extract_time_window(&text_normalized, record.close_ts);
    if !matches!(result.window.kind, TimeWindowKind::Unknown) {
        result.add_note("Time window extracted");
    }
    
    // Extract price source
    result.source = extract_source(&text_normalized);
    if !matches!(result.source, PriceSource::Unknown) {
        result.add_note("Price source extracted");
    }
    
    // Check for conflicts
    check_conflicts(&text_normalized, &mut result);
    
    // Build proposition
    let proposition = build_proposition(&result);
    
    (proposition, result)
}

/// Normalize text for parsing
fn normalize_text(text: &str) -> String {
    let mut s = text.to_lowercase();
    // Remove commas in numbers
    let re = Regex::new(r"(\d),(\d)").unwrap();
    s = re.replace_all(&s, "$1$2").to_string();
    // Normalize whitespace
    let re = Regex::new(r"\s+").unwrap();
    s = re.replace_all(&s, " ").to_string();
    s.trim().to_string()
}

/// Extract underlier symbol
fn extract_underlier(text: &str) -> Option<Underlier> {
    let patterns = [
        (r"(?i)\b(bitcoin|btc)\b", "BTC"),
        (r"(?i)\b(ethereum|eth)\b", "ETH"),
        (r"(?i)\b(solana|sol)\b", "SOL"),
        (r"(?i)\b(xrp|ripple)\b", "XRP"),
        (r"(?i)\b(dogecoin|doge)\b", "DOGE"),
        (r"(?i)\b(cardano|ada)\b", "ADA"),
        (r"(?i)\b(polkadot|dot)\b", "DOT"),
        (r"(?i)\b(chainlink|link)\b", "LINK"),
        (r"(?i)\b(avalanche|avax)\b", "AVAX"),
        (r"(?i)\b(polygon|matic)\b", "MATIC"),
        (r"(?i)\bs&?p\s*500\b", "SP500"),
        (r"(?i)\bnasdaq\b", "NASDAQ"),
        (r"(?i)\bdow\s*jones\b", "DJI"),
        (r"(?i)\bgold\b", "GOLD"),
        (r"(?i)\bsilver\b", "SILVER"),
        (r"(?i)\boil\b", "OIL"),
    ];
    
    for (pattern, symbol) in patterns {
        if let Ok(re) = Regex::new(pattern) {
            if re.is_match(text) {
                return Some(Underlier::new(symbol));
            }
        }
    }
    
    None
}

/// Extract numeric price level
fn extract_level(text: &str) -> Option<f64> {
    // Pattern: $100000 or $100,000 (already normalized)
    if let Ok(re) = Regex::new(r"\$\s*([0-9]+(?:\.[0-9]+)?)") {
        if let Some(caps) = re.captures(text) {
            if let Some(m) = caps.get(1) {
                if let Ok(v) = m.as_str().parse::<f64>() {
                    return Some(v);
                }
            }
        }
    }
    
    // Pattern: 100k or 100K
    if let Ok(re) = Regex::new(r"\b([0-9]+(?:\.[0-9]+)?)\s*[kK]\b") {
        if let Some(caps) = re.captures(text) {
            if let Some(m) = caps.get(1) {
                if let Ok(v) = m.as_str().parse::<f64>() {
                    return Some(v * 1000.0);
                }
            }
        }
    }
    
    // Pattern: plain number after comparator words
    if let Ok(re) = Regex::new(r"(?:above|below|reach|hit|exceed)\s+([0-9]+(?:\.[0-9]+)?)") {
        if let Some(caps) = re.captures(text) {
            if let Some(m) = caps.get(1) {
                if let Ok(v) = m.as_str().parse::<f64>() {
                    return Some(v);
                }
            }
        }
    }
    
    None
}

/// Extract comparator
fn extract_comparator(text: &str) -> Option<Comparator> {
    // Order matters - check more specific patterns first
    let patterns = [
        (r"at or above|>=|≥|at least", Comparator::GE),
        (r"at or below|<=|≤|at most", Comparator::LE),
        (r"above|greater than|exceed|over|reach|hit", Comparator::GT),
        (r"below|less than|under|dip to|fall to|drop to", Comparator::LT),
    ];
    
    for (pattern, comp) in patterns {
        if let Ok(re) = Regex::new(pattern) {
            if re.is_match(text) {
                return Some(comp);
            }
        }
    }
    
    None
}

/// Extract price measure
fn extract_measure(text: &str) -> PriceMeasure {
    if text.contains("closing price") || text.contains("close price") {
        PriceMeasure::Close
    } else if text.contains("spot price") || text.contains("spot rate") {
        PriceMeasure::Spot
    } else if text.contains("vwap") || text.contains("volume weighted") {
        PriceMeasure::VWAP
    } else if text.contains("twap") || text.contains("time weighted") {
        PriceMeasure::TWAP
    } else {
        PriceMeasure::Unknown
    }
}

/// Extract time window
fn extract_time_window(text: &str, close_ts: Option<i64>) -> TimeWindow {
    // "at any time before" pattern
    if text.contains("at any time before") || text.contains("any time before") || 
       text.contains("at any point before") {
        return TimeWindow {
            kind: TimeWindowKind::AnyTimeBefore,
            start_ts: None,
            end_ts: close_ts,
        };
    }
    
    // "closing price on" pattern
    if text.contains("closing price on") || text.contains("at close") || 
       text.contains("at market close") {
        return TimeWindow {
            kind: TimeWindowKind::AtClose,
            start_ts: None,
            end_ts: close_ts,
        };
    }
    
    // "before the market closes" pattern
    if text.contains("before the market closes") || text.contains("before market close") ||
       text.contains("before close") || text.contains("before expiration") {
        return TimeWindow {
            kind: TimeWindowKind::AnyTimeBefore,
            start_ts: None,
            end_ts: close_ts,
        };
    }
    
    // "at" specific time pattern - try to extract timestamp
    // TODO: Parse date expressions like "on January 20" or "at 4pm ET"
    
    // Default: use close_ts if available
    if close_ts.is_some() {
        return TimeWindow {
            kind: TimeWindowKind::Unknown,
            start_ts: None,
            end_ts: close_ts,
        };
    }
    
    TimeWindow::default()
}

/// Extract price source
fn extract_source(text: &str) -> PriceSource {
    let exchanges = [
        ("coinbase", "Coinbase"),
        ("binance", "Binance"),
        ("kraken", "Kraken"),
        ("bitstamp", "Bitstamp"),
        ("gemini", "Gemini"),
        ("ftx", "FTX"),
        ("okx", "OKX"),
        ("bybit", "Bybit"),
    ];
    
    for (pattern, name) in exchanges {
        if text.contains(pattern) {
            return PriceSource::Exchange(name.to_string());
        }
    }
    
    let indices = [
        ("coingecko", "CoinGecko"),
        ("coinmarketcap", "CoinMarketCap"),
        ("cryptocompare", "CryptoCompare"),
    ];
    
    for (pattern, name) in indices {
        if text.contains(pattern) {
            return PriceSource::Index(name.to_string());
        }
    }
    
    // Check for venue-defined sources
    if text.contains("polymarket") || text.contains("uma oracle") {
        return PriceSource::VenueDefined("UMA Oracle".to_string());
    }
    
    if text.contains("kalshi") {
        return PriceSource::VenueDefined("Kalshi".to_string());
    }
    
    PriceSource::Unknown
}

/// Check for conflicting patterns
fn check_conflicts(text: &str, result: &mut ExtractionResult) {
    // Check for conflicting time window patterns
    let any_time = text.contains("any time") || text.contains("at any point");
    let at_close = text.contains("at close") || text.contains("closing price on");
    
    if any_time && at_close {
        result.add_conflict("Conflicting time window: 'any time' and 'at close'");
    }
    
    // Check for multiple price levels
    if let Ok(re) = Regex::new(r"\$\s*[0-9]+") {
        let matches: Vec<_> = re.find_iter(text).collect();
        if matches.len() > 1 {
            result.add_conflict("Multiple price levels found");
        }
    }
    
    // Check for ambiguous comparators
    let has_above = text.contains("above") || text.contains("reach") || text.contains("exceed");
    let has_below = text.contains("below") || text.contains("dip") || text.contains("fall");
    
    if has_above && has_below {
        result.add_conflict("Ambiguous comparator: both 'above' and 'below' present");
    }
}

/// Build proposition from extraction result
fn build_proposition(result: &ExtractionResult) -> PropositionKind {
    // Try to build PriceBarrier
    if let (Some(underlier), Some(comparator), Some(level)) = 
        (&result.underlier, &result.comparator, &result.level) 
    {
        return PropositionKind::PriceBarrier {
            underlier: underlier.clone(),
            comparator: *comparator,
            level: *level,
            measure: result.measure.clone(),
            window: result.window.clone(),
            source: result.source.clone(),
        };
    }
    
    // Try to build RangePartition (TODO: implement range detection)
    
    // Fallback to YesNoEvent
    PropositionKind::YesNoEvent {
        description: String::new(), // Will be filled from title
        window: result.window.clone(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_extract_underlier() {
        assert_eq!(extract_underlier("bitcoin price").unwrap().kind, "BTC");
        assert_eq!(extract_underlier("BTC will reach").unwrap().kind, "BTC");
        assert_eq!(extract_underlier("ethereum dips").unwrap().kind, "ETH");
        assert_eq!(extract_underlier("no crypto here"), None);
    }
    
    #[test]
    fn test_extract_level() {
        assert_eq!(extract_level("price above $100000"), Some(100000.0));
        assert_eq!(extract_level("reach 100k"), Some(100000.0));
        assert_eq!(extract_level("no number"), None);
    }
    
    #[test]
    fn test_extract_comparator() {
        assert_eq!(extract_comparator("at or above"), Some(Comparator::GE));
        assert_eq!(extract_comparator("above"), Some(Comparator::GT));
        assert_eq!(extract_comparator("below"), Some(Comparator::LT));
        assert_eq!(extract_comparator("at or below"), Some(Comparator::LE));
        assert_eq!(extract_comparator("dip to"), Some(Comparator::LT));
    }
    
    #[test]
    fn test_extract_time_window() {
        let close_ts = Some(1234567890000i64);
        
        let w = extract_time_window("at any time before the deadline", close_ts);
        assert!(matches!(w.kind, TimeWindowKind::AnyTimeBefore));
        
        let w = extract_time_window("closing price on january 20", close_ts);
        assert!(matches!(w.kind, TimeWindowKind::AtClose));
    }
    
    #[test]
    fn test_extract_source() {
        assert!(matches!(extract_source("according to coinbase"), PriceSource::Exchange(s) if s == "Coinbase"));
        assert!(matches!(extract_source("coingecko price"), PriceSource::Index(s) if s == "CoinGecko"));
    }
    
    #[test]
    fn test_full_extraction() {
        let record = RulesRecord {
            venue: "test".to_string(),
            market_id: "1".to_string(),
            outcome_id: None,
            url: None,
            fetched_ts: 0,
            title: "BTC >= $100k".to_string(),
            close_ts: Some(1234567890000),
            raw_rules_text: "This market resolves Yes if Bitcoin is at or above $100000 at any time before close according to Coinbase.".to_string(),
            raw_resolution_source: None,
            raw_json: None,
        };
        
        let (prop, result) = extract_proposition(&record);
        
        if let PropositionKind::PriceBarrier { underlier, comparator, level, source, window, .. } = prop {
            assert_eq!(underlier.kind, "BTC");
            assert_eq!(comparator, Comparator::GE);
            assert_eq!(level, 100000.0);
            assert!(matches!(source, PriceSource::Exchange(s) if s == "Coinbase"));
            assert!(matches!(window.kind, TimeWindowKind::AnyTimeBefore));
        } else {
            panic!("Expected PriceBarrier");
        }
        
        assert!(!result.notes.is_empty());
    }
}
