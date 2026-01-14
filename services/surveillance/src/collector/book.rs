use crate::schema::SnapshotRow;
use crate::venue::OrderBookLevel;
use std::collections::HashMap;

#[derive(Debug, Clone)]
pub struct BookState {
    pub market_id: String,
    pub outcome_id: String,
    pub last_update_ts: i64,
    pub bids: Vec<OrderBookLevel>,
    pub asks: Vec<OrderBookLevel>,
    pub sequence: i64,
}

impl BookState {
    pub fn new(market_id: String, outcome_id: String) -> Self {
        Self {
            market_id,
            outcome_id,
            last_update_ts: 0,
            bids: Vec::new(),
            asks: Vec::new(),
            sequence: 0,
        }
    }

    pub fn update(&mut self, bids: Vec<OrderBookLevel>, asks: Vec<OrderBookLevel>, ts: i64, seq: i64) {
        self.bids = bids;
        self.asks = asks;
        self.last_update_ts = ts;
        self.sequence = seq;
    }

    pub fn to_snapshot_row(&self, venue: &str, ts_recv: i64, source_ts: Option<i64>) -> SnapshotRow {
        let bid_px: Vec<f64> = self.bids.iter().map(|l| l.price).collect();
        let bid_sz: Vec<f64> = self.bids.iter().map(|l| l.size).collect();
        let ask_px: Vec<f64> = self.asks.iter().map(|l| l.price).collect();
        let ask_sz: Vec<f64> = self.asks.iter().map(|l| l.size).collect();

        SnapshotRow::new(
            ts_recv,
            venue.to_string(),
            self.market_id.clone(),
            self.outcome_id.clone(),
            self.sequence,
            bid_px,
            bid_sz,
            ask_px,
            ask_sz,
            source_ts,
        )
    }
}

pub struct BookStore {
    books: HashMap<(String, String), BookState>,
}

impl BookStore {
    pub fn new() -> Self {
        Self {
            books: HashMap::new(),
        }
    }

    pub fn get_or_create(&mut self, market_id: String, outcome_id: String) -> &mut BookState {
        self.books
            .entry((market_id.clone(), outcome_id.clone()))
            .or_insert_with(|| BookState::new(market_id, outcome_id))
    }

    pub fn get(&self, market_id: &str, outcome_id: &str) -> Option<&BookState> {
        self.books.get(&(market_id.to_string(), outcome_id.to_string()))
    }

    pub fn get_mut(&mut self, market_id: &str, outcome_id: &str) -> Option<&mut BookState> {
        self.books.get_mut(&(market_id.to_string(), outcome_id.to_string()))
    }

    pub fn remove(&mut self, market_id: &str, outcome_id: &str) {
        self.books.remove(&(market_id.to_string(), outcome_id.to_string()));
    }

    pub fn keys(&self) -> Vec<(String, String)> {
        self.books.keys().cloned().collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_book_state() {
        let mut book = BookState::new("market1".to_string(), "yes".to_string());
        assert_eq!(book.sequence, 0);

        book.update(
            vec![OrderBookLevel { price: 0.5, size: 100.0 }],
            vec![OrderBookLevel { price: 0.6, size: 200.0 }],
            1000,
            1,
        );

        assert_eq!(book.bids.len(), 1);
        assert_eq!(book.asks.len(), 1);
        assert_eq!(book.sequence, 1);
    }

    #[test]
    fn test_book_store() {
        let mut store = BookStore::new();
        let book = store.get_or_create("market1".to_string(), "yes".to_string());
        assert_eq!(book.market_id, "market1");
        assert_eq!(book.outcome_id, "yes");
    }
}
