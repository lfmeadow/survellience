use arrow2::datatypes::{DataType, Field, Schema};
use std::sync::Arc;

#[derive(Debug, Clone)]
pub struct SnapshotRow {
    pub ts_recv: i64,
    pub venue: String,
    pub market_id: String,
    pub outcome_id: String,
    pub seq: i64,
    pub best_bid_px: f64,
    pub best_bid_sz: f64,
    pub best_ask_px: f64,
    pub best_ask_sz: f64,
    pub mid: f64,
    pub spread: f64,
    pub bid_px: Vec<f64>,
    pub bid_sz: Vec<f64>,
    pub ask_px: Vec<f64>,
    pub ask_sz: Vec<f64>,
    pub status: String,
    pub err: String,
    pub source_ts: Option<i64>,
}

impl SnapshotRow {
    pub fn new(
        ts_recv: i64,
        venue: String,
        market_id: String,
        outcome_id: String,
        seq: i64,
        bid_px: Vec<f64>,
        bid_sz: Vec<f64>,
        ask_px: Vec<f64>,
        ask_sz: Vec<f64>,
        source_ts: Option<i64>,
    ) -> Self {
        // Ensure bids are sorted descending, asks ascending
        let mut bid_px = bid_px;
        let mut bid_sz = bid_sz;
        let mut ask_px = ask_px;
        let mut ask_sz = ask_sz;

        // Sort bids descending by price
        let mut bid_indices: Vec<usize> = (0..bid_px.len()).collect();
        bid_indices.sort_by(|&a, &b| bid_px[b].partial_cmp(&bid_px[a]).unwrap_or(std::cmp::Ordering::Equal));
        let sorted_bid_px: Vec<f64> = bid_indices.iter().map(|&i| bid_px[i]).collect();
        let sorted_bid_sz: Vec<f64> = bid_indices.iter().map(|&i| bid_sz[i]).collect();

        // Sort asks ascending by price
        let mut ask_indices: Vec<usize> = (0..ask_px.len()).collect();
        ask_indices.sort_by(|&a, &b| ask_px[a].partial_cmp(&ask_px[b]).unwrap_or(std::cmp::Ordering::Equal));
        let sorted_ask_px: Vec<f64> = ask_indices.iter().map(|&i| ask_px[i]).collect();
        let sorted_ask_sz: Vec<f64> = ask_indices.iter().map(|&i| ask_sz[i]).collect();

        // Cap to matching lengths
        let min_bid_len = sorted_bid_px.len().min(sorted_bid_sz.len());
        let min_ask_len = sorted_ask_px.len().min(sorted_ask_sz.len());
        
        let bid_px = sorted_bid_px[..min_bid_len].to_vec();
        let bid_sz = sorted_bid_sz[..min_bid_len].to_vec();
        let ask_px = sorted_ask_px[..min_ask_len].to_vec();
        let ask_sz = sorted_ask_sz[..min_ask_len].to_vec();

        // Compute best bid/ask
        let best_bid_px = bid_px.first().copied().unwrap_or(f64::NAN);
        let best_bid_sz = if !bid_px.is_empty() { bid_sz[0] } else { 0.0 };
        let best_ask_px = ask_px.first().copied().unwrap_or(f64::NAN);
        let best_ask_sz = if !ask_px.is_empty() { ask_sz[0] } else { 0.0 };

        // Compute mid and spread
        let (mid, spread, status) = if !best_bid_px.is_nan() && !best_ask_px.is_nan() {
            let mid_val = (best_bid_px + best_ask_px) / 2.0;
            let spread_val = best_ask_px - best_bid_px;
            (mid_val, spread_val, "ok".to_string())
        } else if !best_bid_px.is_nan() || !best_ask_px.is_nan() {
            (f64::NAN, f64::NAN, "partial".to_string())
        } else {
            (f64::NAN, f64::NAN, "empty".to_string())
        };

        Self {
            ts_recv,
            venue,
            market_id,
            outcome_id,
            seq,
            best_bid_px,
            best_bid_sz,
            best_ask_px,
            best_ask_sz,
            mid,
            spread,
            bid_px,
            bid_sz,
            ask_px,
            ask_sz,
            status,
            err: String::new(),
            source_ts,
        }
    }

    pub fn cap_to_top_k(&mut self, top_k: usize) {
        if self.bid_px.len() > top_k {
            self.bid_px.truncate(top_k);
            self.bid_sz.truncate(top_k);
        }
        if self.ask_px.len() > top_k {
            self.ask_px.truncate(top_k);
            self.ask_sz.truncate(top_k);
        }
    }
}

pub fn create_snapshot_schema() -> Arc<Schema> {
    Arc::new(Schema::from(vec![
        Field::new("ts_recv", DataType::Int64, false),
        Field::new("venue", DataType::Utf8, false),
        Field::new("market_id", DataType::Utf8, false),
        Field::new("outcome_id", DataType::Utf8, false),
        Field::new("seq", DataType::Int64, false),
        Field::new("best_bid_px", DataType::Float64, false),
        Field::new("best_bid_sz", DataType::Float64, false),
        Field::new("best_ask_px", DataType::Float64, false),
        Field::new("best_ask_sz", DataType::Float64, false),
        Field::new("mid", DataType::Float64, false),
        Field::new("spread", DataType::Float64, false),
        Field::new(
            "bid_px",
            DataType::List(Box::new(Field::new("item", DataType::Float64, false))),
            false,
        ),
        Field::new(
            "bid_sz",
            DataType::List(Box::new(Field::new("item", DataType::Float64, false))),
            false,
        ),
        Field::new(
            "ask_px",
            DataType::List(Box::new(Field::new("item", DataType::Float64, false))),
            false,
        ),
        Field::new(
            "ask_sz",
            DataType::List(Box::new(Field::new("item", DataType::Float64, false))),
            false,
        ),
        Field::new("status", DataType::Utf8, false),
        Field::new("err", DataType::Utf8, false),
        Field::new("source_ts", DataType::Int64, true),
    ]))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_snapshot_row_creation() {
        let row = SnapshotRow::new(
            1000,
            "polymarket".to_string(),
            "market1".to_string(),
            "outcome1".to_string(),
            1,
            vec![0.5, 0.6, 0.4],
            vec![100.0, 200.0, 50.0],
            vec![0.7, 0.8, 0.65],
            vec![150.0, 100.0, 200.0],
            None,
        );

        // Bids should be sorted descending
        assert_eq!(row.bid_px[0], 0.6);
        assert_eq!(row.bid_px[1], 0.5);
        assert_eq!(row.bid_px[2], 0.4);

        // Asks should be sorted ascending
        assert_eq!(row.ask_px[0], 0.65);
        assert_eq!(row.ask_px[1], 0.7);
        assert_eq!(row.ask_px[2], 0.8);

        // Best bid/ask should be correct
        assert_eq!(row.best_bid_px, 0.6);
        assert_eq!(row.best_ask_px, 0.65);
        assert!((row.mid - 0.625).abs() < 0.001);
        assert!((row.spread - 0.05).abs() < 0.001);
        assert_eq!(row.status, "ok");
    }

    #[test]
    fn test_snapshot_row_partial() {
        let row = SnapshotRow::new(
            1000,
            "polymarket".to_string(),
            "market1".to_string(),
            "outcome1".to_string(),
            1,
            vec![0.5],
            vec![100.0],
            vec![],
            vec![],
            None,
        );

        assert_eq!(row.status, "partial");
        assert!(row.mid.is_nan());
    }

    #[test]
    fn test_cap_to_top_k() {
        let mut row = SnapshotRow::new(
            1000,
            "polymarket".to_string(),
            "market1".to_string(),
            "outcome1".to_string(),
            1,
            (0..100).map(|i| i as f64).collect(),
            (0..100).map(|i| i as f64).collect(),
            (0..100).map(|i| i as f64).collect(),
            (0..100).map(|i| i as f64).collect(),
            None,
        );

        row.cap_to_top_k(10);
        assert_eq!(row.bid_px.len(), 10);
        assert_eq!(row.ask_px.len(), 10);
    }
}
