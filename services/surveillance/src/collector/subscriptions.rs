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
        // For Polymarket, batch token_ids together (CLOB WebSocket expects all token IDs in one message)
        let mut pending_add = self.pending_add.lock().await;
        if self.venue_name == "polymarket" {
            let max_pending = venue_config.max_subs;
            if pending_add.len() > max_pending {
                let excess = pending_add.len() - max_pending;
                pending_add.drain(0..excess);
                warn!(
                    "Pending subscriptions exceeded cap ({}). Dropped {} oldest entries.",
                    max_pending,
                    excess
                );
            }

            // Collect all token_ids to subscribe to
            let mut token_ids: Vec<String> = pending_add.iter()
                .map(|(token_id, _)| token_id.clone())
                .collect();
            token_ids.sort();
            token_ids.dedup();

            let max_batch = 500usize;
            if !token_ids.is_empty() && *churn_count < churn_limit {
                if token_ids.len() > max_batch {
                    warn!(
                        "Pending subscription batch exceeds cap ({}). Will send {} now and keep {} queued.",
                        max_batch,
                        max_batch,
                        token_ids.len() - max_batch
                    );
                    token_ids.truncate(max_batch);
                }
                let venue = self.venue.lock().await;
                venue.subscribe(&token_ids, &[]).await?;
                *churn_count += 1;
                debug!("Subscribed to {} token IDs (Polymarket)", token_ids.len());
                let sent: HashSet<String> = token_ids.into_iter().collect();
                pending_add.retain(|(token_id, _)| !sent.contains(token_id));
            }
        } else {
            // Other venues: subscribe one at a time
            while !pending_add.is_empty() && *churn_count < churn_limit {
                let (market_id, outcome_id) = pending_add.remove(0);
                let venue = self.venue.lock().await;
                venue.subscribe(&[market_id.clone()], &[outcome_id.clone()]).await?;
                *churn_count += 1;
                debug!("Subscribed to {}/{}", market_id, outcome_id);
            }
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
                warn!(
                    "Subscription processing error: {}. Stopping loop to preserve pending state.",
                    e
                );
                break;
            }
        }
    }
}
