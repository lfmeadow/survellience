use crate::collector::book::BookStore;
use crate::collector::snapshotter::Snapshotter;
use crate::collector::subscriptions::SubscriptionManager;
use crate::config::Config;
use crate::scheduler::Scheduler;
use crate::storage::ParquetWriter;
use crate::venue::Venue;
use anyhow::{Context, Result};
use std::collections::HashSet;
use std::sync::Arc;
use tokio::sync::Mutex;
use tokio::time::{interval, Duration};
use tracing::{error, info, warn};

pub struct Collector {
    config: Arc<Config>,
    venue_name: String,
    writer: Arc<ParquetWriter>,
    scheduler: Arc<Scheduler>,
    book_store: Arc<Mutex<BookStore>>,
    subscription_manager: Arc<SubscriptionManager>,
    snapshotter: Arc<Snapshotter>,
}

impl Collector {
    pub fn new(
        config: Arc<Config>,
        venue: Box<dyn Venue>,
        venue_name: String,
        writer: Arc<ParquetWriter>,
        scheduler: Arc<Scheduler>,
    ) -> Self {
        let book_store = Arc::new(Mutex::new(BookStore::new()));
        let snapshotter = Arc::new(Snapshotter::new(
            config.clone(),
            writer.clone(),
            book_store.clone(),
            venue_name.clone(),
        ));

        let subscription_manager = Arc::new(SubscriptionManager::new(
            config.clone(),
            venue,
            venue_name.clone(),
        ));

        Self {
            config,
            venue_name,
            writer,
            scheduler,
            book_store,
            subscription_manager,
            snapshotter,
        }
    }

    pub async fn run(&mut self) -> Result<()> {
        info!("Starting collector for venue: {}", self.venue_name);

        // Connect WebSocket
        {
            let mut venue = self.subscription_manager.venue.lock().await;
            venue.connect_websocket().await
                .with_context(|| format!("Failed to connect WebSocket for {}", self.venue_name))?;
        }

        // Start subscription processing loop
        let sub_mgr = self.subscription_manager.clone();
        tokio::spawn(async move {
            sub_mgr.start_processing_loop().await;
        });

        // Start update processing loop
        let book_store = self.book_store.clone();
        let subscription_manager = self.subscription_manager.clone();
        tokio::spawn(async move {
            loop {
                let mut venue = subscription_manager.venue.lock().await;
                match venue.receive_update().await {
                    Ok(Some(update)) => {
                        let mut store = book_store.lock().await;
                        let book = store.get_or_create(
                            update.market_id.clone(),
                            update.outcome_id.clone(),
                        );
                        book.update(
                            update.bids,
                            update.asks,
                            update.timestamp_ms.unwrap_or_else(|| chrono::Utc::now().timestamp_millis()),
                            update.sequence,
                        );
                    }
                    Ok(None) => {
                        tokio::time::sleep(Duration::from_millis(10)).await;
                    }
                    Err(e) => {
                        warn!("Error receiving update: {}", e);
                        tokio::time::sleep(Duration::from_millis(1000)).await;
                    }
                }
            }
        });

        // Main loop: rotation and scheduling
        let mut rotation_interval = interval(Duration::from_secs(10));
        loop {
            rotation_interval.tick().await;

            if self.scheduler.should_rotate(&self.venue_name) {
                info!("Rotating subscriptions for {}", self.venue_name);
                
                // Use unsafe to get mutable access - in production, use Arc<Mutex<Scheduler>>
                let scheduler = unsafe { &mut *(Arc::as_ptr(&self.scheduler) as *mut Scheduler) };
                let (hot, warm) = scheduler.get_target_subscriptions(&self.venue_name)?;
                
                // Update snapshotter sets
                self.snapshotter.update_sets(hot.clone(), warm.clone()).await;
                
                // Update subscription manager
                let mut target = hot;
                target.extend(warm);
                self.subscription_manager.update_target(target).await?;
                
                scheduler.mark_rotated();
            }
        }
    }
}
