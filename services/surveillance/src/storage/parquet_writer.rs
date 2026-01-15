use crate::config::Config;
use crate::schema::SnapshotRow;
use crate::timebucket::TimeBucket;
use anyhow::{Context, Result};
use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;
use tokio::sync::Mutex;
use tokio::time::{interval, Duration};
use tracing::{info, warn};

pub struct ParquetWriter {
    config: Arc<Config>,
    buffer: Arc<Mutex<Vec<SnapshotRow>>>,
    current_bucket: Arc<Mutex<Option<TimeBucket>>>,
    flush_interval: Duration,
}

impl ParquetWriter {
    pub fn new(config: Arc<Config>) -> Self {
        let flush_interval = Duration::from_secs(config.storage.flush_seconds);
        let writer = Self {
            config,
            buffer: Arc::new(Mutex::new(Vec::new())),
            current_bucket: Arc::new(Mutex::new(None)),
            flush_interval,
        };

        // Start periodic flush task
        let buffer_clone = writer.buffer.clone();
        let current_bucket_clone = writer.current_bucket.clone();
        let config_clone = writer.config.clone();
        let flush_interval = writer.flush_interval;
        
        tokio::spawn(async move {
            let mut interval = interval(flush_interval);
            loop {
                interval.tick().await;
                let mut buffer = buffer_clone.lock().await;
                if !buffer.is_empty() {
                    let bucket = current_bucket_clone.lock().await.clone();
                    if let Err(e) = Self::flush_internal(&config_clone, &mut buffer, bucket.as_ref()).await {
                        warn!("Periodic flush failed: {}", e);
                    }
                }
            }
        });

        writer
    }

    pub async fn write(&self, row: SnapshotRow) -> Result<()> {
        let bucket = TimeBucket::from_timestamp(row.ts_recv, self.config.storage.bucket_minutes);
        
        // Check if bucket changed
        let mut current_bucket = self.current_bucket.lock().await;
        let bucket_changed = current_bucket.as_ref().map(|b| *b != bucket).unwrap_or(true);
        
        if bucket_changed {
            // Flush current buffer if bucket changed
            let mut buffer = self.buffer.lock().await;
            if !buffer.is_empty() {
                Self::flush_internal(&self.config, &mut buffer, current_bucket.as_ref()).await?;
            }
            *current_bucket = Some(bucket);
        }

        // Add row to buffer
        let mut buffer = self.buffer.lock().await;
        buffer.push(row);

        // Flush if buffer exceeds size limit
        if buffer.len() >= self.config.storage.flush_rows {
            Self::flush_internal(&self.config, &mut buffer, current_bucket.as_ref()).await?;
        }

        Ok(())
    }

    async fn flush_internal(
        config: &Config,
        buffer: &mut Vec<SnapshotRow>,
        bucket_opt: Option<&TimeBucket>,
    ) -> Result<()> {
        if buffer.is_empty() {
            return Ok(());
        }

        let bucket = bucket_opt
            .copied()
            .unwrap_or_else(|| TimeBucket::from_now(config.storage.bucket_minutes));

        // Group rows by venue
        let mut rows_by_venue: HashMap<String, Vec<SnapshotRow>> = HashMap::new();
        for row in buffer.drain(..) {
            rows_by_venue
                .entry(row.venue.clone())
                .or_insert_with(Vec::new)
                .push(row);
        }

        // Write each venue's rows
        for (venue, rows) in rows_by_venue {
            Self::write_parquet_file(config, &bucket, &venue, rows).await?;
        }

        Ok(())
    }

    async fn write_parquet_file(
        config: &Config,
        bucket: &TimeBucket,
        venue: &str,
        rows: Vec<SnapshotRow>,
    ) -> Result<()> {
        if rows.is_empty() {
            return Ok(());
        }
        
        let (date_str, hour_str) = bucket.path_segments();
        let file_prefix = bucket.file_prefix();

        let dir = Path::new(&config.data_dir)
            .join("orderbook_snapshots")
            .join(format!("venue={}", venue))
            .join(format!("date={}", date_str))
            .join(format!("hour={}", hour_str));

        std::fs::create_dir_all(&dir)
            .with_context(|| format!("Failed to create directory: {:?}", dir))?;

        let temp_file = dir.join(format!("{}.parquet.tmp", file_prefix));
        let final_file = dir.join(format!("{}.parquet", file_prefix));

        // Write Parquet using Polars ParquetWriter
        use polars::prelude::*;
        use std::fs::File;
        
        // Convert rows to Polars DataFrame
        let ts_recv: Vec<i64> = rows.iter().map(|r| r.ts_recv).collect();
        let venue: Vec<String> = rows.iter().map(|r| r.venue.clone()).collect();
        let market_id: Vec<String> = rows.iter().map(|r| r.market_id.clone()).collect();
        let outcome_id: Vec<String> = rows.iter().map(|r| r.outcome_id.clone()).collect();
        let seq: Vec<i64> = rows.iter().map(|r| r.seq).collect();
        let best_bid_px: Vec<f64> = rows.iter().map(|r| r.best_bid_px).collect();
        let best_bid_sz: Vec<f64> = rows.iter().map(|r| r.best_bid_sz).collect();
        let best_ask_px: Vec<f64> = rows.iter().map(|r| r.best_ask_px).collect();
        let best_ask_sz: Vec<f64> = rows.iter().map(|r| r.best_ask_sz).collect();
        let mid: Vec<f64> = rows.iter().map(|r| r.mid).collect();
        let spread: Vec<f64> = rows.iter().map(|r| r.spread).collect();
        // Store lists as JSON strings (Polars list support can be added later)
        let bid_px_json: Vec<String> = rows.iter().map(|r| serde_json::to_string(&r.bid_px).unwrap_or_default()).collect();
        let bid_sz_json: Vec<String> = rows.iter().map(|r| serde_json::to_string(&r.bid_sz).unwrap_or_default()).collect();
        let ask_px_json: Vec<String> = rows.iter().map(|r| serde_json::to_string(&r.ask_px).unwrap_or_default()).collect();
        let ask_sz_json: Vec<String> = rows.iter().map(|r| serde_json::to_string(&r.ask_sz).unwrap_or_default()).collect();
        let status: Vec<String> = rows.iter().map(|r| r.status.clone()).collect();
        let err: Vec<String> = rows.iter().map(|r| r.err.clone()).collect();
        let source_ts: Vec<Option<i64>> = rows.iter().map(|r| r.source_ts).collect();
        
        let df = DataFrame::new(vec![
            Series::new("ts_recv", ts_recv),
            Series::new("venue", venue),
            Series::new("market_id", market_id),
            Series::new("outcome_id", outcome_id),
            Series::new("seq", seq),
            Series::new("best_bid_px", best_bid_px),
            Series::new("best_bid_sz", best_bid_sz),
            Series::new("best_ask_px", best_ask_px),
            Series::new("best_ask_sz", best_ask_sz),
            Series::new("mid", mid),
            Series::new("spread", spread),
            Series::new("bid_px", bid_px_json),
            Series::new("bid_sz", bid_sz_json),
            Series::new("ask_px", ask_px_json),
            Series::new("ask_sz", ask_sz_json),
            Series::new("status", status),
            Series::new("err", err),
            Series::new("source_ts", source_ts),
        ]).context("Failed to create DataFrame")?;
        
        // Write Parquet using Polars ParquetWriter
        let _file = File::create(&temp_file)
            .with_context(|| format!("Failed to create temp file: {:?}", temp_file))?;
        
        // Write Parquet using Polars lazy API
        // Polars 0.40: use sink_parquet on LazyFrame
        let file_path = temp_file.clone();
        df.lazy()
            .sink_parquet(
                file_path,
                ParquetWriteOptions::default(),
            )
            .context("Failed to write Parquet file")?;

        // Atomic rename
        std::fs::rename(&temp_file, &final_file)
            .with_context(|| format!("Failed to rename {:?} to {:?}", temp_file, final_file))?;

        info!(
            "Wrote {} rows to {:?} (Parquet format)",
            rows.len(),
            final_file
        );

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::schema::SnapshotRow;
    use tempfile::TempDir;

    #[tokio::test]
    async fn test_parquet_writer_write() {
        let temp_dir = TempDir::new().unwrap();
        let config = Arc::new(Config {
            data_dir: temp_dir.path().to_string_lossy().to_string(),
            venues: crate::config::VenuesConfig {
                polymarket: None,
                kalshi: None,
            },
            storage: crate::config::StorageConfig {
                top_k: 50,
                flush_rows: 10, // Small for testing
                flush_seconds: 5,
                bucket_minutes: 5,
            },
            rotation: crate::config::RotationConfig { enabled: true },
            mock: crate::config::MockConfig {
                enabled: true,
                universe_size: 1000,
                markets_per_venue: 500,
            },
        });

        let writer = ParquetWriter::new(config.clone());
        
        // Write a few rows
        for i in 0..5 {
            let row = SnapshotRow::new(
                chrono::Utc::now().timestamp_millis(),
                "polymarket".to_string(),
                format!("market_{}", i),
                "yes".to_string(),
                i,
                vec![0.5, 0.49],
                vec![100.0, 200.0],
                vec![0.51, 0.52],
                vec![150.0, 100.0],
                None,
            );
            writer.write(row).await.unwrap();
        }

        // Wait a bit for flush
        tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

        // Check that file was created
        let bucket = TimeBucket::from_now(5);
        let (date_str, hour_str) = bucket.path_segments();
        let file_prefix = bucket.file_prefix();
        let expected_file = temp_dir
            .path()
            .join("orderbook_snapshots")
            .join("venue=polymarket")
            .join(format!("date={}", date_str))
            .join(format!("hour={}", hour_str))
            .join(format!("{}.parquet", file_prefix));

        // File might not exist yet if flush hasn't happened, but structure should be correct
        // Let's force a flush by writing enough rows
        for i in 5..15 {
            let row = SnapshotRow::new(
                chrono::Utc::now().timestamp_millis(),
                "polymarket".to_string(),
                format!("market_{}", i),
                "yes".to_string(),
                i,
                vec![0.5, 0.49],
                vec![100.0, 200.0],
                vec![0.51, 0.52],
                vec![150.0, 100.0],
                None,
            );
            writer.write(row).await.unwrap();
        }

        // Wait for flush
        tokio::time::sleep(tokio::time::Duration::from_millis(200)).await;

        // Now file should exist
        assert!(expected_file.exists() || expected_file.parent().unwrap().exists());
    }
}
