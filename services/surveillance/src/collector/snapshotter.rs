use crate::collector::book::BookStore;
use crate::config::Config;
use crate::schema::SnapshotRow;
use crate::storage::ParquetWriter;
use chrono::Utc;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::Mutex;
use tokio::time::{interval, Duration};
use tracing::{debug, info};

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
            next_snapshot: self.next_snapshot.clone(),
            snapshot_interval_hot: self.snapshot_interval_hot,
            snapshot_interval_warm: self.snapshot_interval_warm,
            hot_set: self.hot_set.clone(),
            warm_set: self.warm_set.clone(),
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
    next_snapshot: Arc<Mutex<HashMap<(String, String), std::time::Instant>>>,
    snapshot_interval_hot: Duration,
    snapshot_interval_warm: Duration,
    hot_set: Arc<Mutex<std::collections::HashSet<(String, String)>>>,
    warm_set: Arc<Mutex<std::collections::HashSet<(String, String)>>>,
}

impl SnapshotterTask {
    async fn take_snapshots(&self) -> anyhow::Result<()> {
        let now = std::time::Instant::now();
        let hot_set = self.hot_set.lock().await.clone();
        let warm_set = self.warm_set.lock().await.clone();
        let mut next_snapshot = self.next_snapshot.lock().await;
        let book_store = self.book_store.lock().await;

        let keys = book_store.keys();
        drop(book_store);

        for key in keys {
            let interval = if hot_set.contains(&key) {
                self.snapshot_interval_hot
            } else if warm_set.contains(&key) {
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
