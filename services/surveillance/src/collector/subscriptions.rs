use crate::config::Config;
use crate::venue::Venue;
use anyhow::Result;
use std::collections::HashSet;
use std::sync::Arc;
use tokio::sync::Mutex;
use tokio::time::{interval, Duration};
use tracing::{debug, info, warn};

pub struct SubscriptionManager {
    config: Arc<Config>,
    pub(crate) venue: Arc<Mutex<Box<dyn Venue>>>,
    venue_name: String,
    current: Arc<Mutex<HashSet<(String, String)>>>,
    pending_add: Arc<Mutex<Vec<(String, String)>>>,
    pending_remove: Arc<Mutex<Vec<(String, String)>>>,
    last_churn: Arc<Mutex<std::time::Instant>>,
    churn_count: Arc<Mutex<usize>>,
}

impl SubscriptionManager {
    pub fn new(
        config: Arc<Config>,
        venue: Box<dyn Venue>,
        venue_name: String,
    ) -> Self {
        Self {
            config,
            venue: Arc::new(Mutex::new(venue)),
            venue_name,
            current: Arc::new(Mutex::new(HashSet::new())),
            pending_add: Arc::new(Mutex::new(Vec::new())),
            pending_remove: Arc::new(Mutex::new(Vec::new())),
            last_churn: Arc::new(Mutex::new(std::time::Instant::now())),
            churn_count: Arc::new(Mutex::new(0)),
        }
    }

    pub async fn update_target(&self, target: HashSet<(String, String)>) -> Result<()> {
        let current = self.current.lock().await.clone();
        
        let to_add: Vec<_> = target.difference(&current).cloned().collect();
        let to_remove: Vec<_> = current.difference(&target).cloned().collect();

        if !to_add.is_empty() || !to_remove.is_empty() {
            info!(
                "Subscription update for {}: add {}, remove {}",
                self.venue_name,
                to_add.len(),
                to_remove.len()
            );
        }

        // Add to pending queues
        self.pending_add.lock().await.extend(to_add);
        self.pending_remove.lock().await.extend(to_remove);

        // Update current
        *self.current.lock().await = target;

        Ok(())
    }

    pub async fn process_pending(&self) -> Result<()> {
        let venue_config = self
            .config
            .get_venue_config(&self.venue_name)
            .ok_or_else(|| anyhow::anyhow!("Venue config not found"))?;

        let churn_limit = venue_config.subscription_churn_limit_per_minute;
        let mut last_churn = self.last_churn.lock().await;
        let mut churn_count = self.churn_count.lock().await;

        // Reset counter if minute has passed
        if last_churn.elapsed().as_secs() >= 60 {
            *churn_count = 0;
            *last_churn = std::time::Instant::now();
        }

        // Process adds first (subscribe before unsubscribe)
        let mut pending_add = self.pending_add.lock().await;
        while !pending_add.is_empty() && *churn_count < churn_limit {
            let (market_id, outcome_id) = pending_add.remove(0);
            let venue = self.venue.lock().await;
            venue.subscribe(&[market_id.clone()], &[outcome_id.clone()]).await?;
            *churn_count += 1;
            debug!("Subscribed to {}/{}", market_id, outcome_id);
        }

        // Process removes
        let mut pending_remove = self.pending_remove.lock().await;
        while !pending_remove.is_empty() && *churn_count < churn_limit {
            let (market_id, outcome_id) = pending_remove.remove(0);
            let venue = self.venue.lock().await;
            venue.unsubscribe(&[market_id.clone()], &[outcome_id.clone()]).await?;
            *churn_count += 1;
            debug!("Unsubscribed from {}/{}", market_id, outcome_id);
        }

        Ok(())
    }

    pub async fn start_processing_loop(&self) {
        let mut interval = interval(Duration::from_secs(1));
        loop {
            interval.tick().await;
            if let Err(e) = self.process_pending().await {
                warn!("Subscription processing error: {}", e);
            }
        }
    }
}
