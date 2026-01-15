use anyhow::Result;
use clap::{ArgAction, Parser, Subcommand};
use surveillance::analytics::mm_viability::run_mm_viability;
use surveillance::analytics::Miner;
use surveillance::config::Config;
use tracing_subscriber;

#[derive(Parser, Debug)]
#[command(author, version, about)]
struct Cli {
    #[arg(long, default_value = "config/surveillance.toml")]
    config: String,

    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand, Debug)]
enum Commands {
    Mine {
        #[arg(long, default_value = "polymarket")]
        venue: String,
        #[arg(long)]
        date: Option<String>,
    },
    MmViability {
        #[arg(long, default_value = "polymarket")]
        venue: String,
        #[arg(long)]
        date: String,
        #[arg(long, default_value = "all")]
        hours: String,
        #[arg(long, default_value_t = 0.0)]
        fee_estimate: f64,
        #[arg(long, default_value_t = 20)]
        top: usize,
        #[arg(long, default_value_t = true, action = ArgAction::Set)]
        write_report: bool,
    },
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .init();

    let cli = Cli::parse();
    let config = Config::load(&cli.config)?;

    match cli.command {
        Commands::Mine { venue, date } => {
            let miner = Miner::new(config);
            miner.mine(&venue, date.as_deref()).await?;
        }
        Commands::MmViability {
            venue,
            date,
            hours,
            fee_estimate,
            top,
            write_report,
        } => {
            run_mm_viability(
                &config,
                &venue,
                &date,
                &hours,
                fee_estimate,
                top,
                write_report,
            )?;
        }
    }

    Ok(())
}
