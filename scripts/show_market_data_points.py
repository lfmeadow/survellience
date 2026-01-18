#!/usr/bin/env python3
"""Display market data points summary with market descriptions"""
import sys
import polars as pl
import json
from pathlib import Path

def show_market_data_points(venue: str, date: str):
    """Show market data points summary by reading all parquet files for the date"""
    # Find all parquet files for the date
    base_path = Path(f'data/orderbook_snapshots/venue={venue}/date={date}')
    
    if not base_path.exists():
        print(f"Error: No data directory found at {base_path}")
        sys.exit(1)
    
    # Collect all parquet file paths
    parquet_files = []
    for hour_dir in sorted(base_path.glob("hour=*")):
        if hour_dir.is_dir():
            for parquet_file in hour_dir.glob("*.parquet"):
                parquet_files.append(parquet_file)
    
    if not parquet_files:
        print(f"Error: No parquet files found for {venue} on {date}")
        sys.exit(1)
    
    print(f"Reading {len(parquet_files)} parquet files...")
    
    # Read all parquet files and concatenate
    dfs = []
    for parquet_file in parquet_files:
        try:
            df = pl.read_parquet(parquet_file)
            dfs.append(df)
        except Exception as e:
            print(f"Warning: Failed to read {parquet_file}: {e}", file=sys.stderr)
            continue
    
    if not dfs:
        print("Error: No valid parquet files could be read")
        sys.exit(1)
    
    # Concatenate all dataframes
    df_all = pl.concat(dfs)
    
    # Group by market_id only (aggregate across all outcomes) - no translation first
    summary = df_all.group_by(['market_id']).agg([
        pl.len().alias('n_rows')
    ]).sort('n_rows', descending=True)
    
    # Load market titles from universe file for display
    universe_path = Path(f'data/metadata/venue={venue}/date={date}/universe.jsonl')
    market_info = {}
    if universe_path.exists():
        with open(universe_path, 'r') as f:
            for line in f:
                try:
                    market = json.loads(line.strip())
                    market_info[market['market_id']] = market.get('title', 'Unknown Market')
                except:
                    continue
    
    # Add market titles for display (but keep market_id for grouping)
    if market_info:
        summary = summary.with_columns([
            pl.col('market_id').map_elements(
                lambda x: market_info.get(x, f'Market {x[:20]}...'),
                return_dtype=pl.Utf8
            ).alias('market_title')
        ])
        result = summary.select(['market_id', 'market_title', 'n_rows'])
    else:
        result = summary.select(['market_id', 'n_rows'])

    # Print formatted table
    print("=" * 120)
    print(f"Market Data Points Summary - {venue.title()} - {date}")
    print("=" * 120)
    print()
    if 'market_title' in result.columns:
        print(f"{'Market ID':<80} {'Market Title':<80} {'Data Points':<12}")
        print("-" * 120)
        for row in result.iter_rows(named=True):
            market_id = row['market_id']
            if len(market_id) > 78:
                market_id = market_id[:75] + "..."
            title = row['market_title']
            if len(title) > 78:
                title = title[:75] + "..."
            print(f"{market_id:<80} {title:<80} {row['n_rows']:<12}")
    else:
        print(f"{'Market ID':<80} {'Data Points':<12}")
        print("-" * 120)
        for row in result.iter_rows(named=True):
            market_id = row['market_id']
            if len(market_id) > 78:
                market_id = market_id[:75] + "..."
            print(f"{market_id:<80} {row['n_rows']:<12}")

    print("-" * 120)
    print(f"{'TOTAL':<80} {result['n_rows'].sum():<12}")
    print("=" * 120)

if __name__ == '__main__':
    if len(sys.argv) < 3:
        print("Usage: show_market_data_points.py <venue> <date>")
        print("Example: show_market_data_points.py polymarket 2026-01-16")
        sys.exit(1)
    
    venue = sys.argv[1]
    date = sys.argv[2]
    show_market_data_points(venue, date)
