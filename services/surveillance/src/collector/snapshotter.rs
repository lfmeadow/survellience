use crate::collector::book::BookStore;
use crate::config::Config;
use crate::storage::ParquetWriter;
use chrono::Utc;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::Mutex;
use tokio::time::{interval, Duration};
use tracing::debug;
use serde_json;

pub struct Snapshotter {
    config: Arc<Config>,
    writer: Arc<ParquetWriter>,
    book_store: Arc<Mutex<BookStore>>,
    venue_name: String,
    next_snapshot: Arc<Mutex<HashMap<(String, String), std::time::Instant>>>,
    snapshot_interval_hot: Duration,
    snapshot_interval_warm: Duration,
    hot_set: Arc<Mutex<std::collections::HashSet<(String, String)>>>,
    warm_set: Arc<Mutex<std::collections::HashSet<(String, String)>>>,
}

impl Snapshotter {
    pub fn new(
        config: Arc<Config>,
        writer: Arc<ParquetWriter>,
        book_store: Arc<Mutex<BookStore>>,
        venue_name: String,
    ) -> Self {
        let venue_config = config
            .get_venue_config(&venue_name)
            .expect("Venue config not found");

        let snapshot_interval_hot = Duration::from_millis(venue_config.snapshot_interval_ms_hot);
        let snapshot_interval_warm = Duration::from_millis(venue_config.snapshot_interval_ms_warm);

        let snapshotter = Self {
            config,
            writer,
            book_store,
            venue_name,
            next_snapshot: Arc::new(Mutex::new(HashMap::new())),
            snapshot_interval_hot,
            snapshot_interval_warm,
            hot_set: Arc::new(Mutex::new(std::collections::HashSet::new())),
            warm_set: Arc::new(Mutex::new(std::collections::HashSet::new())),
        };

        // Start snapshot loop
        let snapshotter_clone = snapshotter.clone_for_task();
        tokio::spawn(async move {
            let mut interval = interval(Duration::from_millis(100));
            loop {
                interval.tick().await;
                if let Err(e) = snapshotter_clone.take_snapshots().await {
                    debug!("Snapshot error: {}", e);
                }
            }
        });

        snapshotter
    }

    fn clone_for_task(&self) -> SnapshotterTask {
        SnapshotterTask {
            writer: self.writer.clone(),
            book_store: self.book_store.clone(),
            venue_name: self.venue_name.clone(),
            config: self.config.clone(),
            next_snapshot: self.next_snapshot.clone(),
            snapshot_interval_hot: self.snapshot_interval_hot,
            snapshot_interval_warm: self.snapshot_interval_warm,
            hot_set: self.hot_set.clone(),
            warm_set: self.warm_set.clone(),
            market_to_token: Arc::new(Mutex::new(std::collections::HashMap::new())),
        }
    }

    pub async fn update_sets(
        &self,
        hot: std::collections::HashSet<(String, String)>,
        warm: std::collections::HashSet<(String, String)>,
    ) {
        *self.hot_set.lock().await = hot;
        *self.warm_set.lock().await = warm;
    }
}

struct SnapshotterTask {
    writer: Arc<ParquetWriter>,
    book_store: Arc<Mutex<BookStore>>,
    venue_name: String,
    config: Arc<Config>,
    next_snapshot: Arc<Mutex<HashMap<(String, String), std::time::Instant>>>,
    snapshot_interval_hot: Duration,
    snapshot_interval_warm: Duration,
    hot_set: Arc<Mutex<std::collections::HashSet<(String, String)>>>,
    warm_set: Arc<Mutex<std::collections::HashSet<(String, String)>>>,
    // Reverse mapping: (market_id, outcome_id) -> token_id
    market_to_token: Arc<Mutex<std::collections::HashMap<(String, String), String>>>,
}

impl SnapshotterTask {
    async fn load_market_to_token_mapping(&self) {
        if self.venue_name != "polymarket" {
            return; // Only needed for Polymarket
        }
        
        let mut mapping = self.market_to_token.lock().await;
        if !mapping.is_empty() {
            return; // Already loaded
        }
        
        // Load universe file to create reverse mapping
        use chrono::Utc;
        let today = Utc::now().date_naive();
        let date_str = today.format("%Y-%m-%d").to_string();
        
        let universe_path = std::path::Path::new(&self.config.data_dir)
            .join("metadata")
            .join(format!("venue={}", self.venue_name))
            .join(format!("date={}", date_str))
            .join("universe.jsonl");
        
        if let Ok(content) = std::fs::read_to_string(&universe_path) {
            for line in content.lines() {
                if let Ok(market_info) = serde_json::from_str::<crate::venue::MarketInfo>(line) {
                    for (idx, token_id) in market_info.token_ids.iter().enumerate() {
                        let outcome_id = if idx < market_info.outcome_ids.len() {
                            market_info.outcome_ids[idx].clone()
                        } else {
                            format!("{}", idx)
                        };
                        mapping.insert((market_info.market_id.clone(), outcome_id), token_id.clone());
                    }
                }
            }
            debug!("Loaded {} (market_id, outcome_id) -> token_id mappings for snapshotter", mapping.len());
        }
    }

    async fn take_snapshots(&self) -> anyhow::Result<()> {
        // Load mapping if needed (only once)
        self.load_market_to_token_mapping().await;
        
        let now = std::time::Instant::now();
        let hot_set = self.hot_set.lock().await.clone();
        let warm_set = self.warm_set.lock().await.clone();
        let mut next_snapshot = self.next_snapshot.lock().await;
        let book_store = self.book_store.lock().await;

        let keys = book_store.keys();
        drop(book_store);

        for key in keys {
            // For Polymarket: hot/warm sets contain (token_id, ""), but book_store uses (market_id, outcome_id)
            // Need to map (market_id, outcome_id) -> token_id, then check if (token_id, "") is in hot/warm sets
            let is_hot = if self.venue_name == "polymarket" {
                let mapping = self.market_to_token.lock().await;
                if let Some(token_id) = mapping.get(&key) {
                    hot_set.contains(&(token_id.clone(), "".to_string()))
                } else {
                    false
                }
            } else {
                hot_set.contains(&key)
            };
            
            let is_warm = if self.venue_name == "polymarket" {
                let mapping = self.market_to_token.lock().await;
                if let Some(token_id) = mapping.get(&key) {
                    warm_set.contains(&(token_id.clone(), "".to_string()))
                } else {
                    false
                }
            } else {
                warm_set.contains(&key)
            };
            
            let interval = if is_hot {
                self.snapshot_interval_hot
            } else if is_warm {
                self.snapshot_interval_warm
            } else {
                continue; // Not subscribed
            };

            let should_snapshot = next_snapshot
                .get(&key)
                .map(|&next| now >= next)
                .unwrap_or(true);

            if should_snapshot {
                let book_store = self.book_store.lock().await;
                if let Some(book) = book_store.get(&key.0, &key.1) {
                    let ts_recv = Utc::now().timestamp_millis();
                    let row = book.to_snapshot_row(&self.venue_name, ts_recv, None);
                    drop(book_store);
                    self.writer.write(row).await?;
                    next_snapshot.insert(key.clone(), now + interval);
                    debug!("Created snapshot: market={}, outcome={}", key.0, key.1);
                }
            }
        }

        Ok(())
    }
}

impl Clone for Snapshotter {
    fn clone(&self) -> Self {
        Self {
            config: self.config.clone(),
            writer: self.writer.clone(),
            book_store: self.book_store.clone(),
            venue_name: self.venue_name.clone(),
            next_snapshot: self.next_snapshot.clone(),
            snapshot_interval_hot: self.snapshot_interval_hot,
            snapshot_interval_warm: self.snapshot_interval_warm,
            hot_set: self.hot_set.clone(),
            warm_set: self.warm_set.clone(),
        }
    }
}
