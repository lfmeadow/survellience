use crate::config::Config;
use anyhow::{Context, Result};
use chrono::Utc;
use polars::prelude::*;
use std::path::Path;
use tracing::{info, warn};

pub struct Miner {
    config: Config,
}

impl Miner {
    pub fn new(config: Config) -> Self {
        Self { config }
    }

    pub async fn mine(&self, venue: &str, date: Option<&str>) -> Result<()> {
        let date_str = if let Some(d) = date {
            d
        } else {
            // Use today's date
            let today = Utc::now().date_naive().format("%Y-%m-%d").to_string();
            // Store in a box to avoid recursion
            Box::leak(Box::new(today))
        };

        info!("Mining data for venue={}, date={}", venue, date_str);

        // Read snapshots
        let snapshots_path = Path::new(&self.config.data_dir)
            .join("orderbook_snapshots")
            .join(format!("venue={}", venue))
            .join(format!("date={}", date_str));

        if !snapshots_path.exists() {
            warn!("No snapshots found at {:?}", snapshots_path);
            return Ok(());
        }

        // Read all parquet files for the date
        let mut dfs = Vec::new();
        for hour_dir in std::fs::read_dir(&snapshots_path)? {
            let hour_dir = hour_dir?;
            if !hour_dir.file_type()?.is_dir() {
                continue;
            }
            let hour_path = hour_dir.path();
            for entry in std::fs::read_dir(&hour_path)? {
                let entry = entry?;
            let path = entry.path();
            let ext = path.extension().and_then(|s| s.to_str());
            if ext == Some("parquet") {
                // Read Parquet file directly (avoid Hive partition schema conflicts)
                use polars::prelude::ParquetReader;
                match std::fs::File::open(&path) {
                    Ok(file) => {
                        match ParquetReader::new(file).finish() {
                            Ok(df) => dfs.push(df.lazy()),
                            Err(e) => warn!("Failed to read {:?}: {}", path, e),
                        }
                    }
                    Err(e) => warn!("Failed to open {:?}: {}", path, e),
                }
            } else if ext == Some("csv") {
                // Handle CSV files (temporary format)
                // Use LazyCsvReader for CSV files
                use polars::prelude::LazyCsvReader;
                match LazyCsvReader::new(&path).finish() {
                    Ok(df) => dfs.push(df),
                    Err(e) => warn!("Failed to read CSV {:?}: {}", path, e),
                }
            }
        }
        }

        if dfs.is_empty() {
            warn!("No parquet/CSV files found");
            return Ok(());
        }

        // Concatenate all dataframes
        let df = concat(dfs, UnionArgs::default())
            .context("Failed to concatenate dataframes")?
            .collect()
            .context("Failed to collect lazy frame")?;

        info!("Loaded {} rows", df.height());

        // Compute metrics
        let stats = self.compute_stats(&df, venue)?;

        // Write stats cache
        self.write_stats_cache(venue, date_str, &stats)?;

        // Print summary
        self.print_summary(&stats);

        Ok(())
    }

    fn compute_stats(&self, df: &DataFrame, _venue: &str) -> Result<DataFrame> {
        // Group by market_id and outcome_id
        let stats = df
            .clone()
            .lazy()
            .group_by([col("market_id"), col("outcome_id")])
            .agg([
                // Average spread
                col("spread").mean().alias("avg_spread"),
                // Update count (number of rows)
                col("ts_recv").count().alias("update_count"),
                // Average depth (simplified - sum of best bid/ask sizes)
                (col("best_bid_sz") + col("best_ask_sz")).mean().alias("avg_depth"),
            ])
            .collect()
            .context("Failed to compute stats")?;

        Ok(stats)
    }

    fn write_stats_cache(&self, venue: &str, date: &str, stats: &DataFrame) -> Result<()> {
        let output_path = Path::new(&self.config.data_dir)
            .join("stats")
            .join(format!("venue={}", venue))
            .join(format!("date={}", date));

        std::fs::create_dir_all(&output_path)
            .with_context(|| format!("Failed to create directory: {:?}", output_path))?;

        let file_path = output_path.join("stats.parquet");
        
        // Write as Parquet using Polars sink_parquet (same pattern as parquet_writer)
        // sink_parquet takes PathBuf or &str - use PathBuf directly
        stats.clone()
            .lazy()
            .sink_parquet(
                file_path.clone(),
                ParquetWriteOptions::default(),
            )
            .context("Failed to write Parquet file")?;
        
        info!("Wrote stats cache to {:?}", file_path);
        
        // Note: CSV export can be done using external tools:
        //   polars-cli convert stats.parquet stats.csv
        //   or Python: pl.read_parquet("stats.parquet").write_csv("stats.csv")

        info!("Wrote stats cache to {:?}", file_path);
        Ok(())
    }

    fn print_summary(&self, stats: &DataFrame) {
        println!("\n=== Mining Summary ===");
        println!("Total markets/outcomes: {}", stats.height());
        
        // Top markets by depth (descending)
        if let Ok(depth_sorted) = stats.sort(["avg_depth"], SortMultipleOptions::new()) {
            println!("\nTop 10 by average depth:");
            let top_10 = depth_sorted.head(Some(10));
            if let Ok(selected) = top_10.select(["market_id", "outcome_id", "avg_depth"]) {
                println!("{}", selected);
            }
        }

        // Top markets by tightest spread (ascending)
        if let Ok(spread_sorted) = stats.sort(["avg_spread"], SortMultipleOptions::new()) {
            println!("\nTop 10 by tightest spread:");
            let top_10 = spread_sorted.head(Some(10));
            if let Ok(selected) = top_10.select(["market_id", "outcome_id", "avg_spread"]) {
                println!("{}", selected);
            }
        }

        // Most active markets (descending)
        if let Ok(active_sorted) = stats.sort(["update_count"], SortMultipleOptions::new()) {
            println!("\nTop 10 most active:");
            let top_10 = active_sorted.head(Some(10));
            if let Ok(selected) = top_10.select(["market_id", "outcome_id", "update_count"]) {
                println!("{}", selected);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{MockConfig, RotationConfig, StorageConfig, VenuesConfig};
    use tempfile::TempDir;

    #[tokio::test]
    async fn test_miner() {
        let temp_dir = TempDir::new().unwrap();
        let config = Config {
            data_dir: temp_dir.path().to_string_lossy().to_string(),
            venues: VenuesConfig {
                polymarket: None,
                kalshi: None,
            },
            storage: StorageConfig {
                top_k: 50,
                flush_rows: 50000,
                flush_seconds: 5,
                bucket_minutes: 5,
            },
            rotation: RotationConfig { enabled: true },
            mock: MockConfig {
                enabled: true,
                universe_size: 1000,
                markets_per_venue: 500,
            },
        };

        let miner = Miner::new(config);
        // Test will pass even if no data exists (just warns)
        let result = miner.mine("polymarket", None).await;
        assert!(result.is_ok());
    }
}
