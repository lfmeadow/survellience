//! Rules → Logic → Constraints → Arb Detector CLI
//!
//! Usage:
//!   surveillance_rules ingest --venue polymarket --date 2026-01-19
//!   surveillance_rules normalize --venue polymarket --date 2026-01-19
//!   surveillance_rules constraints --venue polymarket --date 2026-01-19
//!   surveillance_rules detect-arb --venue polymarket --date 2026-01-19
//!   surveillance_rules run-all --venue polymarket --date 2026-01-19
//!   surveillance_rules run-all --mock --all-venues --date 2026-01-19

use anyhow::Result;
use clap::{Parser, Subcommand};
use chrono::Utc;
use std::collections::HashMap;

use surveillance::rules::{
    ingest::{
        IngestConfig, RulesIngestor, MockIngestor, PolymarketIngestor, KalshiIngestor,
        RulesRecord, run_ingest, write_rules_jsonl, generate_mock_universe,
    },
    normalize::normalize_batch,
    constraints::{generate_constraints, ConstraintConfig},
    arb_detector::{
        ArbDetectorConfig, DetectionMode, detect_violations,
        load_latest_prices, generate_mock_prices_with_violations,
    },
    review_queue::{create_review_item, write_review_queue, filter_for_review},
    outputs::{
        write_propositions_parquet, write_constraints_parquet, write_violations_parquet,
    },
};

#[derive(Parser)]
#[command(name = "surveillance_rules")]
#[command(about = "Rules → Logic → Constraints → Arb Detector pipeline")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Ingest rules text from venue APIs or mock
    Ingest {
        #[arg(long)]
        venue: Option<String>,
        #[arg(long)]
        date: Option<String>,
        #[arg(long)]
        all_venues: bool,
        #[arg(long)]
        mock: bool,
        #[arg(long)]
        force: bool,
        #[arg(long, default_value = "data")]
        data_dir: String,
        /// Limit number of markets to process (for testing)
        #[arg(long)]
        limit: Option<usize>,
    },
    /// Normalize rules into propositions
    Normalize {
        #[arg(long)]
        venue: Option<String>,
        #[arg(long)]
        date: Option<String>,
        #[arg(long)]
        all_venues: bool,
        #[arg(long, default_value = "data")]
        data_dir: String,
    },
    /// Generate constraints from propositions
    Constraints {
        #[arg(long)]
        venue: Option<String>,
        #[arg(long)]
        date: Option<String>,
        #[arg(long)]
        all_venues: bool,
        #[arg(long, default_value = "data")]
        data_dir: String,
    },
    /// Detect arbitrage violations
    DetectArb {
        #[arg(long)]
        venue: Option<String>,
        #[arg(long)]
        date: Option<String>,
        #[arg(long)]
        all_venues: bool,
        #[arg(long, default_value = "latest")]
        mode: String,
        #[arg(long)]
        window_mins: Option<u32>,
        #[arg(long, default_value = "0.01")]
        margin: f64,
        #[arg(long)]
        mock: bool,
        #[arg(long, default_value = "data")]
        data_dir: String,
    },
    /// Run full pipeline
    RunAll {
        #[arg(long)]
        venue: Option<String>,
        #[arg(long)]
        date: Option<String>,
        #[arg(long)]
        all_venues: bool,
        #[arg(long)]
        mock: bool,
        #[arg(long, default_value = "data")]
        data_dir: String,
    },
}

fn get_venues(venue: Option<String>, all_venues: bool) -> Vec<String> {
    if all_venues {
        vec!["polymarket".to_string(), "kalshi".to_string()]
    } else if let Some(v) = venue {
        vec![v]
    } else {
        vec!["polymarket".to_string()]
    }
}

fn get_date(date: Option<String>) -> String {
    date.unwrap_or_else(|| Utc::now().format("%Y-%m-%d").to_string())
}

fn get_ingestor(venue: &str, mock: bool) -> Box<dyn RulesIngestor> {
    if mock {
        Box::new(MockIngestor::new(venue))
    } else {
        match venue {
            "polymarket" => Box::new(PolymarketIngestor::new()),
            "kalshi" => Box::new(KalshiIngestor::new()),
            _ => Box::new(MockIngestor::new(venue)),
        }
    }
}

async fn run_ingest_command(
    venue: &str,
    date: &str,
    data_dir: &str,
    mock: bool,
    force: bool,
    limit: Option<usize>,
) -> Result<Vec<RulesRecord>> {
    tracing::info!("Ingesting rules for venue={}, date={}, limit={:?}", venue, date, limit);
    
    let ingestor = get_ingestor(venue, mock);
    
    // For mock mode, generate a mock universe first
    if mock {
        let mock_markets = generate_mock_universe(venue);
        let mut records = Vec::new();
        for m in &mock_markets {
            if let Ok(record) = ingestor.fetch_rules(m).await {
                records.push(record);
            }
        }
        
        write_rules_jsonl(data_dir, venue, date, &records, false)?;
        return Ok(records);
    }
    
    let config = IngestConfig {
        venue: venue.to_string(),
        date: date.to_string(),
        data_dir: data_dir.to_string(),
        force_refetch: force,
        concurrency: 2,
        rate_limit_ms: 100, // 100ms between requests
        limit,
    };
    
    let records = run_ingest(&config, ingestor.as_ref()).await?;
    write_rules_jsonl(data_dir, venue, date, &records, true)?;
    
    Ok(records)
}

fn load_rules_records(data_dir: &str, venue: &str, date: &str) -> Result<Vec<RulesRecord>> {
    let path = std::path::Path::new(data_dir)
        .join("rules")
        .join(format!("venue={}", venue))
        .join(format!("date={}", date))
        .join("rules.jsonl");
    
    if !path.exists() {
        anyhow::bail!("Rules file not found: {:?}. Run 'ingest' first.", path);
    }
    
    let content = std::fs::read_to_string(&path)?;
    let mut records = Vec::new();
    
    for line in content.lines() {
        if line.trim().is_empty() {
            continue;
        }
        let record: RulesRecord = serde_json::from_str(line)?;
        records.push(record);
    }
    
    Ok(records)
}

fn run_normalize_command(
    venue: &str,
    date: &str,
    data_dir: &str,
) -> Result<Vec<surveillance::rules::NormalizedProposition>> {
    tracing::info!("Normalizing rules for venue={}, date={}", venue, date);
    
    let records = load_rules_records(data_dir, venue, date)?;
    tracing::info!("Loaded {} rules records", records.len());
    
    let propositions = normalize_batch(&records);
    tracing::info!("Normalized {} propositions", propositions.len());
    
    // Write propositions
    write_propositions_parquet(data_dir, venue, date, &propositions)?;
    
    // Filter low confidence for review
    let for_review = filter_for_review(&propositions, None);
    if !for_review.is_empty() {
        tracing::info!("{} propositions need review (confidence < 0.6)", for_review.len());
        
        // Create review items
        let rules_map: HashMap<String, String> = records
            .iter()
            .map(|r| (r.market_id.clone(), r.raw_rules_text.clone()))
            .collect();
        
        let review_items: Vec<_> = for_review
            .iter()
            .map(|p| {
                let raw_text = rules_map.get(&p.market_id).cloned().unwrap_or_default();
                create_review_item(p, &raw_text)
            })
            .collect();
        
        write_review_queue(data_dir, venue, date, &review_items)?;
    }
    
    Ok(propositions)
}

fn load_propositions(
    data_dir: &str,
    venue: &str,
    date: &str,
) -> Result<Vec<surveillance::rules::NormalizedProposition>> {
    use polars::prelude::*;
    
    let path = std::path::Path::new(data_dir)
        .join("logic")
        .join(format!("venue={}", venue))
        .join(format!("date={}", date))
        .join("propositions.parquet");
    
    if !path.exists() {
        anyhow::bail!("Propositions file not found: {:?}. Run 'normalize' first.", path);
    }
    
    // Use ParquetReader directly to avoid Hive partitioning issues
    let file = std::fs::File::open(&path)?;
    let df = ParquetReader::new(file).finish()?;
    
    let mut propositions = Vec::new();
    
    for row_idx in 0..df.height() {
        let venue = df.column("venue")?.str()?.get(row_idx).unwrap_or("").to_string();
        let market_id = df.column("market_id")?.str()?.get(row_idx).unwrap_or("").to_string();
        let outcome_id = df.column("outcome_id")?.str()?.get(row_idx).map(|s| s.to_string());
        let title = df.column("title")?.str()?.get(row_idx).unwrap_or("").to_string();
        let raw_rules_hash = df.column("raw_rules_hash")?.str()?.get(row_idx).unwrap_or("").to_string();
        let confidence = df.column("confidence")?.f64()?.get(row_idx).unwrap_or(0.0);
        let proposition_json = df.column("proposition_json")?.str()?.get(row_idx).unwrap_or("{}");
        let notes_json = df.column("parse_notes")?.str()?.get(row_idx).unwrap_or("[]");
        
        let proposition: surveillance::rules::PropositionKind = 
            serde_json::from_str(proposition_json).unwrap_or_default();
        let parse_notes: Vec<String> = serde_json::from_str(notes_json).unwrap_or_default();
        
        propositions.push(surveillance::rules::NormalizedProposition {
            venue,
            market_id,
            outcome_id,
            title,
            raw_rules_hash,
            proposition,
            confidence,
            parse_notes,
        });
    }
    
    Ok(propositions)
}

fn run_constraints_command(
    venue: &str,
    date: &str,
    data_dir: &str,
) -> Result<Vec<surveillance::rules::Constraint>> {
    tracing::info!("Generating constraints for venue={}, date={}", venue, date);
    
    let propositions = load_propositions(data_dir, venue, date)?;
    tracing::info!("Loaded {} propositions", propositions.len());
    
    let config = ConstraintConfig::default();
    let constraints = generate_constraints(&propositions, &config);
    tracing::info!("Generated {} constraints", constraints.len());
    
    write_constraints_parquet(data_dir, venue, date, &constraints)?;
    
    Ok(constraints)
}

fn load_constraints(
    data_dir: &str,
    venue: &str,
    date: &str,
) -> Result<Vec<surveillance::rules::Constraint>> {
    use polars::prelude::*;
    
    let path = std::path::Path::new(data_dir)
        .join("logic")
        .join(format!("venue={}", venue))
        .join(format!("date={}", date))
        .join("constraints.parquet");
    
    if !path.exists() {
        anyhow::bail!("Constraints file not found: {:?}. Run 'constraints' first.", path);
    }
    
    // Use ParquetReader directly to avoid Hive partitioning issues
    let file = std::fs::File::open(&path)?;
    let df = ParquetReader::new(file).finish()?;
    
    let mut constraints = Vec::new();
    
    for row_idx in 0..df.height() {
        let id = df.column("id")?.str()?.get(row_idx).unwrap_or("").to_string();
        let venue = df.column("venue")?.str()?.get(row_idx).unwrap_or("").to_string();
        let constraint_type = df.column("constraint_type")?.str()?.get(row_idx).unwrap_or("").to_string();
        let a_market_id = df.column("a_market_id")?.str()?.get(row_idx).unwrap_or("").to_string();
        let a_outcome_id = df.column("a_outcome_id")?.str()?.get(row_idx).map(|s| s.to_string());
        let b_market_id = df.column("b_market_id")?.str()?.get(row_idx).unwrap_or("").to_string();
        let b_outcome_id = df.column("b_outcome_id")?.str()?.get(row_idx).map(|s| s.to_string());
        let relation = df.column("relation")?.str()?.get(row_idx).unwrap_or("").to_string();
        let confidence = df.column("confidence")?.f64()?.get(row_idx).unwrap_or(0.0);
        let group_key = df.column("group_key")?.str()?.get(row_idx).unwrap_or("").to_string();
        let notes_json = df.column("notes")?.str()?.get(row_idx).unwrap_or("[]");
        let notes: Vec<String> = serde_json::from_str(notes_json).unwrap_or_default();
        
        constraints.push(surveillance::rules::Constraint {
            id,
            venue,
            constraint_type,
            a_market_id,
            a_outcome_id,
            b_market_id,
            b_outcome_id,
            relation,
            confidence,
            notes,
            group_key,
        });
    }
    
    Ok(constraints)
}

fn run_detect_arb_command(
    venue: &str,
    date: &str,
    data_dir: &str,
    mode: &str,
    window_mins: Option<u32>,
    margin: f64,
    mock: bool,
) -> Result<Vec<surveillance::rules::Violation>> {
    tracing::info!("Detecting arb violations for venue={}, date={}", venue, date);
    
    let constraints = load_constraints(data_dir, venue, date)?;
    tracing::info!("Loaded {} constraints", constraints.len());
    
    let detection_mode = match mode {
        "rolling" => DetectionMode::Rolling,
        _ => DetectionMode::Latest,
    };
    
    let config = ArbDetectorConfig {
        margin,
        mode: detection_mode,
        window_minutes: window_mins,
    };
    
    // Get prices
    let prices = if mock {
        generate_mock_prices_with_violations(&constraints)
    } else {
        load_latest_prices(data_dir, venue, date)?
    };
    
    tracing::info!("Loaded {} price records", prices.len());
    
    let violations = detect_violations(&constraints, &prices, &config);
    tracing::info!("Detected {} violations", violations.len());
    
    write_violations_parquet(data_dir, venue, date, &violations)?;
    
    // Print summary
    for v in &violations {
        println!(
            "VIOLATION: {} | P({})={:.3} > P({})={:.3} + {:.3} | magnitude={:.3}",
            v.constraint_type,
            v.a_market_id,
            v.p_a,
            v.b_market_id,
            v.p_b,
            v.margin,
            v.violation_magnitude
        );
    }
    
    Ok(violations)
}

async fn run_all_command(
    venue: &str,
    date: &str,
    data_dir: &str,
    mock: bool,
) -> Result<()> {
    tracing::info!("Running full pipeline for venue={}, date={}, mock={}", venue, date, mock);
    
    // 1. Ingest
    let records = run_ingest_command(venue, date, data_dir, mock, false, None).await?;
    tracing::info!("Step 1/4: Ingested {} rules", records.len());
    
    // 2. Normalize
    let propositions = run_normalize_command(venue, date, data_dir)?;
    tracing::info!("Step 2/4: Normalized {} propositions", propositions.len());
    
    // 3. Constraints
    let constraints = run_constraints_command(venue, date, data_dir)?;
    tracing::info!("Step 3/4: Generated {} constraints", constraints.len());
    
    // 4. Detect violations
    let violations = run_detect_arb_command(venue, date, data_dir, "latest", None, 0.01, mock)?;
    tracing::info!("Step 4/4: Detected {} violations", violations.len());
    
    // Print summary
    println!("\n=== Pipeline Summary ===");
    println!("Venue: {}", venue);
    println!("Date: {}", date);
    println!("Rules ingested: {}", records.len());
    println!("Propositions: {}", propositions.len());
    println!("Constraints: {}", constraints.len());
    println!("Violations: {}", violations.len());
    
    let high_conf = propositions.iter().filter(|p| p.confidence >= 0.6).count();
    let low_conf = propositions.len() - high_conf;
    println!("High confidence propositions: {}", high_conf);
    println!("Low confidence (review queue): {}", low_conf);
    
    Ok(())
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .init();
    
    let cli = Cli::parse();
    
    match cli.command {
        Commands::Ingest { venue, date, all_venues, mock, force, data_dir, limit } => {
            let venues = get_venues(venue, all_venues);
            let date = get_date(date);
            
            for v in venues {
                run_ingest_command(&v, &date, &data_dir, mock, force, limit).await?;
            }
        }
        Commands::Normalize { venue, date, all_venues, data_dir } => {
            let venues = get_venues(venue, all_venues);
            let date = get_date(date);
            
            for v in venues {
                run_normalize_command(&v, &date, &data_dir)?;
            }
        }
        Commands::Constraints { venue, date, all_venues, data_dir } => {
            let venues = get_venues(venue, all_venues);
            let date = get_date(date);
            
            for v in venues {
                run_constraints_command(&v, &date, &data_dir)?;
            }
        }
        Commands::DetectArb { venue, date, all_venues, mode, window_mins, margin, mock, data_dir } => {
            let venues = get_venues(venue, all_venues);
            let date = get_date(date);
            
            for v in venues {
                run_detect_arb_command(&v, &date, &data_dir, &mode, window_mins, margin, mock)?;
            }
        }
        Commands::RunAll { venue, date, all_venues, mock, data_dir } => {
            let venues = get_venues(venue, all_venues);
            let date = get_date(date);
            
            for v in venues {
                run_all_command(&v, &date, &data_dir, mock).await?;
            }
        }
    }
    
    Ok(())
}
