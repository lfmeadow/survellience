use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use tokio::sync::Mutex;
use tokio::time::{Duration, Instant};
use tracing::{info, warn};

/// Tracks WebSocket message statistics
#[derive(Clone)]
pub struct WebSocketMetrics {
    // Message counters
    total_messages_received: Arc<AtomicU64>,
    total_updates_processed: Arc<AtomicU64>,
    total_errors: Arc<AtomicU64>,
    
    // Rate tracking (per second)
    messages_per_second: Arc<Mutex<RateTracker>>,
    updates_per_second: Arc<Mutex<RateTracker>>,
    
    // Sequence gap tracking
    sequence_gaps: Arc<Mutex<HashMap<(String, String), SequenceTracker>>>,
    
    // Queue depth
    queue_depth: Arc<AtomicU64>,
    
    // Last report time
    last_report: Arc<Mutex<Instant>>,
    report_interval: Duration,
}

struct RateTracker {
    count: u64,
    window_start: Instant,
}

struct SequenceTracker {
    last_sequence: i64,
    gaps_detected: u64,
    out_of_order: u64,
}

impl RateTracker {
    fn new() -> Self {
        Self {
            count: 0,
            window_start: Instant::now(),
        }
    }

    fn increment(&mut self) {
        self.count += 1;
    }

    fn get_rate(&mut self) -> f64 {
        let elapsed = self.window_start.elapsed();
        if elapsed.as_secs() >= 1 {
            // Reset every second
            let rate = self.count as f64 / elapsed.as_secs_f64();
            self.count = 0;
            self.window_start = Instant::now();
            rate
        } else {
            // Extrapolate from partial second
            self.count as f64 / elapsed.as_secs_f64().max(0.1)
        }
    }
}

impl SequenceTracker {
    fn new() -> Self {
        Self {
            last_sequence: 0,
            gaps_detected: 0,
            out_of_order: 0,
        }
    }

    fn check_sequence(&mut self, new_sequence: i64) -> (bool, i64) {
        // Check for gaps or out-of-order
        let expected = self.last_sequence + 1;
        let gap_size = new_sequence - expected;
        
        if new_sequence < self.last_sequence {
            // Out of order
            self.out_of_order += 1;
            self.last_sequence = new_sequence.max(self.last_sequence);
            (false, 0)
        } else if gap_size > 0 {
            // Gap detected
            self.gaps_detected += gap_size as u64;
            self.last_sequence = new_sequence;
            (true, gap_size)
        } else {
            // Normal sequence
            self.last_sequence = new_sequence;
            (false, 0)
        }
    }
}

impl WebSocketMetrics {
    pub fn new(report_interval_secs: u64) -> Self {
        Self {
            total_messages_received: Arc::new(AtomicU64::new(0)),
            total_updates_processed: Arc::new(AtomicU64::new(0)),
            total_errors: Arc::new(AtomicU64::new(0)),
            messages_per_second: Arc::new(Mutex::new(RateTracker::new())),
            updates_per_second: Arc::new(Mutex::new(RateTracker::new())),
            sequence_gaps: Arc::new(Mutex::new(HashMap::new())),
            queue_depth: Arc::new(AtomicU64::new(0)),
            last_report: Arc::new(Mutex::new(Instant::now())),
            report_interval: Duration::from_secs(report_interval_secs),
        }
    }

    pub async fn record_message_received(&self) {
        self.total_messages_received.fetch_add(1, Ordering::Relaxed);
        self.queue_depth.fetch_add(1, Ordering::Relaxed);
        
        let mut rate_tracker = self.messages_per_second.lock().await;
        rate_tracker.increment();
    }

    pub async fn record_update_processed(&self, market_id: &str, outcome_id: &str, sequence: i64) {
        self.total_updates_processed.fetch_add(1, Ordering::Relaxed);
        self.queue_depth.fetch_sub(1, Ordering::Relaxed);
        
        let mut rate_tracker = self.updates_per_second.lock().await;
        rate_tracker.increment();

        // Check for sequence gaps
        let key = (market_id.to_string(), outcome_id.to_string());
        let mut trackers = self.sequence_gaps.lock().await;
        let tracker = trackers.entry(key).or_insert_with(SequenceTracker::new);
        let (gap_detected, gap_size) = tracker.check_sequence(sequence);
        
        if gap_detected {
            warn!(
                "Sequence gap detected: market={}, outcome={}, expected={}, got={}, gap={}, total_gaps={}",
                market_id,
                outcome_id,
                tracker.last_sequence - gap_size,
                sequence,
                gap_size,
                tracker.gaps_detected
            );
        }
    }

    pub fn record_error(&self) {
        self.total_errors.fetch_add(1, Ordering::Relaxed);
    }

    pub fn set_queue_depth(&self, depth: u64) {
        self.queue_depth.store(depth, Ordering::Relaxed);
    }

    pub async fn get_current_stats(&self) -> MetricsSnapshot {
        let msg_rate = {
            let mut tracker = self.messages_per_second.lock().await;
            tracker.get_rate()
        };
        
        let update_rate = {
            let mut tracker = self.updates_per_second.lock().await;
            tracker.get_rate()
        };

        let gap_stats = {
            let trackers = self.sequence_gaps.lock().await;
            let mut total_gaps = 0u64;
            let mut total_out_of_order = 0u64;
            let mut markets_with_gaps = 0u64;
            
            for tracker in trackers.values() {
                if tracker.gaps_detected > 0 || tracker.out_of_order > 0 {
                    markets_with_gaps += 1;
                }
                total_gaps += tracker.gaps_detected;
                total_out_of_order += tracker.out_of_order;
            }
            
            (total_gaps, total_out_of_order, markets_with_gaps)
        };

        MetricsSnapshot {
            total_messages: self.total_messages_received.load(Ordering::Relaxed),
            total_updates: self.total_updates_processed.load(Ordering::Relaxed),
            total_errors: self.total_errors.load(Ordering::Relaxed),
            message_rate: msg_rate,
            update_rate: update_rate,
            queue_depth: self.queue_depth.load(Ordering::Relaxed),
            sequence_gaps: gap_stats.0,
            out_of_order: gap_stats.1,
            markets_with_issues: gap_stats.2,
        }
    }

    pub async fn maybe_report(&self) {
        let mut last_report = self.last_report.lock().await;
        if last_report.elapsed() >= self.report_interval {
            let stats = self.get_current_stats().await;
            self.log_stats(&stats);
            *last_report = Instant::now();
        }
    }

    fn log_stats(&self, stats: &MetricsSnapshot) {
        info!(
            "WebSocket metrics: msg_rate={:.1}/s, update_rate={:.1}/s, queue_depth={}, total_msg={}, total_updates={}, errors={}, gaps={}, out_of_order={}, markets_with_issues={}",
            stats.message_rate,
            stats.update_rate,
            stats.queue_depth,
            stats.total_messages,
            stats.total_updates,
            stats.total_errors,
            stats.sequence_gaps,
            stats.out_of_order,
            stats.markets_with_issues
        );
    }
}

#[derive(Debug)]
pub struct MetricsSnapshot {
    pub total_messages: u64,
    pub total_updates: u64,
    pub total_errors: u64,
    pub message_rate: f64,
    pub update_rate: f64,
    pub queue_depth: u64,
    pub sequence_gaps: u64,
    pub out_of_order: u64,
    pub markets_with_issues: u64,
}
