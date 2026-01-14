use super::traits::{MarketInfo, OrderBookLevel, OrderBookUpdate, Venue};
use anyhow::Result;
use async_trait::async_trait;
use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use tokio::sync::Mutex;
use tokio::time::{sleep, Duration};

pub struct MockVenue {
    name: String,
    market_count: usize,
    connected: Arc<AtomicBool>,
    sequence: Arc<AtomicU64>,
    updates: Arc<Mutex<VecDeque<OrderBookUpdate>>>,
    subscribed: Arc<Mutex<Vec<(String, String)>>>,
}

impl MockVenue {
    pub fn new(name: String, market_count: usize) -> Self {
        let venue = Self {
            name,
            market_count,
            connected: Arc::new(AtomicBool::new(false)),
            sequence: Arc::new(AtomicU64::new(1)),
            updates: Arc::new(Mutex::new(VecDeque::new())),
            subscribed: Arc::new(Mutex::new(Vec::new())),
        };

        // Start update generator
        let updates_clone = venue.updates.clone();
        let subscribed_clone = venue.subscribed.clone();
        let sequence_clone = venue.sequence.clone();
        tokio::spawn(async move {
            let mut rng = fastrand::Rng::new();
            loop {
                sleep(Duration::from_millis(100)).await;
                let subscribed = subscribed_clone.lock().await.clone();
                if !subscribed.is_empty() {
                    let (market_id, outcome_id) = subscribed[rng.usize(..subscribed.len())].clone();
                    let seq = sequence_clone.fetch_add(1, Ordering::Relaxed);
                    
                    let update = OrderBookUpdate {
                        market_id,
                        outcome_id,
                        bids: generate_levels(&mut rng, true),
                        asks: generate_levels(&mut rng, false),
                        timestamp_ms: Some(chrono::Utc::now().timestamp_millis()),
                        sequence: seq as i64,
                    };
                    updates_clone.lock().await.push_back(update);
                }
            }
        });

        venue
    }
}

fn generate_levels(rng: &mut fastrand::Rng, is_bid: bool) -> Vec<OrderBookLevel> {
    let count = rng.usize(3..10);
    let mut levels = Vec::new();
    let base_price = if is_bid { 0.4 } else { 0.6 };
    
    for i in 0..count {
        let price_offset = (i as f64) * 0.01;
        let price = if is_bid {
            base_price - price_offset
        } else {
            base_price + price_offset
        };
        let size = rng.f64() * 1000.0 + 10.0;
        levels.push(OrderBookLevel { price, size });
    }
    levels
}

#[async_trait]
impl Venue for MockVenue {
    fn name(&self) -> &str {
        &self.name
    }

    async fn discover_markets(&self) -> Result<Vec<MarketInfo>> {
        let mut markets = Vec::new();
        for i in 0..self.market_count {
            markets.push(MarketInfo {
                market_id: format!("market_{}", i),
                title: format!("Mock Market {}", i),
                outcome_ids: vec!["yes".to_string(), "no".to_string()],
                close_ts: Some(chrono::Utc::now().timestamp_millis() + 86400_000),
                status: "active".to_string(),
                tags: vec!["mock".to_string()],
            });
        }
        Ok(markets)
    }

    async fn connect_websocket(&self) -> Result<()> {
        self.connected.store(true, Ordering::Relaxed);
        Ok(())
    }

    async fn subscribe(&self, market_ids: &[String], outcome_ids: &[String]) -> Result<()> {
        let mut subscribed = self.subscribed.lock().await;
        for market_id in market_ids {
            for outcome_id in outcome_ids {
                subscribed.push((market_id.clone(), outcome_id.clone()));
            }
        }
        Ok(())
    }

    async fn unsubscribe(&self, market_ids: &[String], outcome_ids: &[String]) -> Result<()> {
        let mut subscribed = self.subscribed.lock().await;
        subscribed.retain(|(m, o)| {
            !market_ids.contains(m) || !outcome_ids.contains(o)
        });
        Ok(())
    }

    async fn receive_update(&mut self) -> Result<Option<OrderBookUpdate>> {
        let mut updates = self.updates.lock().await;
        Ok(updates.pop_front())
    }

    fn is_connected(&self) -> bool {
        self.connected.load(Ordering::Relaxed)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_mock_venue_discover() {
        let venue = MockVenue::new("test".to_string(), 10);
        let markets = venue.discover_markets().await.unwrap();
        assert_eq!(markets.len(), 10);
        assert_eq!(markets[0].market_id, "market_0");
    }

    #[tokio::test]
    async fn test_mock_venue_connect() {
        let venue = MockVenue::new("test".to_string(), 5);
        assert!(!venue.is_connected());
        venue.connect_websocket().await.unwrap();
        assert!(venue.is_connected());
    }

    #[tokio::test]
    async fn test_mock_venue_subscribe() {
        let venue = MockVenue::new("test".to_string(), 5);
        venue.connect_websocket().await.unwrap();
        venue.subscribe(&["market_0".to_string()], &["yes".to_string()]).await.unwrap();
        
        // Wait a bit for updates
        tokio::time::sleep(Duration::from_millis(200)).await;
        
        let mut venue_mut = venue;
        let update = venue_mut.receive_update().await.unwrap();
        assert!(update.is_some());
        let update = update.unwrap();
        assert_eq!(update.market_id, "market_0");
        assert_eq!(update.outcome_id, "yes");
        assert!(!update.bids.is_empty());
        assert!(!update.asks.is_empty());
    }
}
