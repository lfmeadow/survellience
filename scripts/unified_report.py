#!/usr/bin/env python3
"""
Unified Report: One line per token-id with all static and dynamic information.

Static info (from universe/rules):
- token_id, market_id, outcome_id, title, close_ts, url
- rules_text (truncated), underlier, strike, proposition_kind, confidence

Dynamic info (from orderbook snapshots/stats):
- mid_price, spread, bid_depth, ask_depth
- update_count, last_update_ts
- mm_viability metrics (if available)
"""

import argparse
import json
import sys
from datetime import datetime, timezone
from pathlib import Path
from typing import Dict, List, Optional

try:
    import polars as pl
except ImportError:
    print("ERROR: polars not installed. Install with: pip install polars")
    sys.exit(1)


def load_universe(data_dir: Path, venue: str, date: str) -> Dict[str, dict]:
    """Load universe and index by market_id."""
    path = data_dir / "metadata" / f"venue={venue}" / f"date={date}" / "universe.jsonl"
    markets = {}
    if path.exists():
        for line in path.read_text().splitlines():
            if line.strip():
                try:
                    m = json.loads(line)
                    markets[m.get("market_id", "")] = m
                except json.JSONDecodeError:
                    pass
    return markets


def load_rules(data_dir: Path, venue: str, date: str) -> Dict[str, dict]:
    """Load rules and index by market_id."""
    path = data_dir / "rules" / f"venue={venue}" / f"date={date}" / "rules.jsonl"
    rules = {}
    if path.exists():
        for line in path.read_text().splitlines():
            if line.strip():
                try:
                    r = json.loads(line)
                    rules[r.get("market_id", "")] = r
                except json.JSONDecodeError:
                    pass
    return rules


def load_propositions(data_dir: Path, venue: str, date: str) -> Dict[str, dict]:
    """Load propositions and index by market_id."""
    path = data_dir / "logic" / f"venue={venue}" / f"date={date}" / "propositions.parquet"
    props = {}
    if path.exists():
        df = pl.read_parquet(path)
        for row in df.iter_rows(named=True):
            props[row.get("market_id", "")] = row
    return props


def load_latest_snapshots(data_dir: Path, venue: str, date: str) -> Dict[str, dict]:
    """Load latest orderbook snapshot per token_id."""
    snapshot_dir = data_dir / "orderbook_snapshots" / f"venue={venue}" / f"date={date}"
    snapshots = {}
    
    if not snapshot_dir.exists():
        return snapshots
    
    # Find all parquet files
    parquet_files = list(snapshot_dir.rglob("*.parquet"))
    if not parquet_files:
        return snapshots
    
    for pf in parquet_files:
        try:
            df = pl.read_parquet(pf)
            if df.is_empty():
                continue
            
            # Get latest snapshot per token (use actual column names)
            agg_cols = [
                pl.col("ts_recv").max().alias("last_ts"),
                pl.len().alias("update_count"),
            ]
            
            # Add columns that exist
            if "mid" in df.columns:
                agg_cols.append(pl.col("mid").last().alias("mid_price"))
            if "spread" in df.columns:
                agg_cols.append(pl.col("spread").last().alias("spread"))
            if "best_bid_sz" in df.columns:
                agg_cols.append(pl.col("best_bid_sz").last().alias("bid_depth"))
            if "best_ask_sz" in df.columns:
                agg_cols.append(pl.col("best_ask_sz").last().alias("ask_depth"))
            if "best_bid_px" in df.columns:
                agg_cols.append(pl.col("best_bid_px").last().alias("best_bid"))
            if "best_ask_px" in df.columns:
                agg_cols.append(pl.col("best_ask_px").last().alias("best_ask"))
            
            latest = df.group_by(["market_id", "outcome_id"]).agg(agg_cols)
            
            for row in latest.iter_rows(named=True):
                key = f"{row['market_id']}_{row['outcome_id']}"
                if key not in snapshots or row['last_ts'] > snapshots[key].get('last_ts', 0):
                    snapshots[key] = row
        except Exception as e:
            print(f"Warning: Failed to read {pf}: {e}", file=sys.stderr)
    
    return snapshots


def load_mm_viability(data_dir: Path, venue: str, date: str) -> Dict[str, dict]:
    """Load MM viability stats if available."""
    # Check for stats cache
    stats_path = data_dir / "stats" / f"venue={venue}" / f"date={date}" / "stats.parquet"
    stats = {}
    
    if stats_path.exists():
        try:
            df = pl.read_parquet(stats_path)
            for row in df.iter_rows(named=True):
                key = f"{row.get('market_id', '')}_{row.get('outcome_id', '')}"
                stats[key] = row
        except Exception:
            pass
    
    return stats


def format_ts(ts_ms: Optional[int]) -> str:
    """Format timestamp in milliseconds to readable string."""
    if ts_ms is None or ts_ms == 0:
        return ""
    try:
        dt = datetime.fromtimestamp(ts_ms / 1000, tz=timezone.utc)
        return dt.strftime("%Y-%m-%d %H:%M")
    except Exception:
        return ""


def truncate(s: str, max_len: int = 60) -> str:
    """Truncate string to max length."""
    if not s:
        return ""
    s = s.replace("\n", " ").replace("\r", " ")
    if len(s) > max_len:
        return s[:max_len-3] + "..."
    return s


def generate_report(data_dir: Path, venue: str, date: str, output_format: str = "csv") -> None:
    """Generate unified report."""
    print(f"Loading data for {venue}/{date}...", file=sys.stderr)
    
    universe = load_universe(data_dir, venue, date)
    rules = load_rules(data_dir, venue, date)
    propositions = load_propositions(data_dir, venue, date)
    snapshots = load_latest_snapshots(data_dir, venue, date)
    mm_stats = load_mm_viability(data_dir, venue, date)
    
    print(f"  Universe: {len(universe)} markets", file=sys.stderr)
    print(f"  Rules: {len(rules)} markets", file=sys.stderr)
    print(f"  Propositions: {len(propositions)} markets", file=sys.stderr)
    print(f"  Snapshots: {len(snapshots)} tokens", file=sys.stderr)
    print(f"  MM Stats: {len(mm_stats)} tokens", file=sys.stderr)
    
    # Build unified records
    rows = []
    
    # Iterate through universe (primary source)
    for market_id, market in universe.items():
        token_ids = market.get("token_ids", [])
        outcome_ids = market.get("outcome_ids", [])
        
        # If no token_ids, use outcome_ids as tokens
        if not token_ids:
            token_ids = outcome_ids if outcome_ids else ["0", "1"]
        
        rule = rules.get(market_id, {})
        prop = propositions.get(market_id, {})
        
        for i, token_id in enumerate(token_ids):
            outcome_id = outcome_ids[i] if i < len(outcome_ids) else str(i)
            snap_key = f"{market_id}_{outcome_id}"
            
            snap = snapshots.get(snap_key, {})
            mm = mm_stats.get(snap_key, {})
            
            row = {
                # Identifiers
                "token_id": str(token_id) if token_id else "",
                "market_id": (market_id[:20] + "...") if len(market_id) > 20 else market_id,
                "outcome_id": str(outcome_id) if outcome_id else "",
                
                # Static info
                "title": truncate(str(market.get("title", "")), 50),
                "close_ts": format_ts(market.get("close_ts")),
                "status": str(market.get("status", "")),
                
                # Rules info
                "rules_text": truncate(str(rule.get("raw_rules_text", "")), 80),
                "url": str(rule.get("url", "") or ""),
                
                # Proposition info
                "underlier": str(prop.get("underlier", "") or ""),
                "strike": str(prop.get("strike_level", "") or ""),
                "comparator": str(prop.get("comparator", "") or ""),
                "prop_kind": str(prop.get("proposition_kind", "") or ""),
                "confidence": f"{prop.get('confidence', 0):.2f}" if prop.get("confidence") else "",
                
                # Dynamic info (snapshots)
                "mid_price": f"{snap.get('mid_price', 0):.4f}" if snap.get("mid_price") else "",
                "spread": f"{snap.get('spread', 0):.4f}" if snap.get("spread") else "",
                "bid_depth": f"{snap.get('bid_depth', 0):.2f}" if snap.get("bid_depth") else "",
                "ask_depth": f"{snap.get('ask_depth', 0):.2f}" if snap.get("ask_depth") else "",
                "updates": str(snap.get("update_count", "")),
                "last_update": format_ts(snap.get("last_ts")),
                
                # MM viability metrics
                "avg_spread": f"{mm.get('avg_spread', 0):.4f}" if mm.get("avg_spread") else "",
                "toxicity": f"{mm.get('toxicity_30s', 0):.4f}" if mm.get("toxicity_30s") else "",
            }
            rows.append(row)
    
    if not rows:
        print("No data found.", file=sys.stderr)
        return
    
    # Create DataFrame and output
    df = pl.DataFrame(rows)
    
    if output_format == "csv":
        print(df.write_csv())
    elif output_format == "json":
        print(df.write_json(row_oriented=True))
    else:  # table
        print(df)


def main():
    parser = argparse.ArgumentParser(description="Unified Report: all info per token-id")
    parser.add_argument("--venue", default="polymarket", help="Venue name")
    parser.add_argument("--date", default=datetime.now(timezone.utc).strftime("%Y-%m-%d"), help="Date (YYYY-MM-DD)")
    parser.add_argument("--data-dir", default="data", help="Data directory")
    parser.add_argument("--format", choices=["csv", "json", "table"], default="table", help="Output format")
    args = parser.parse_args()
    
    data_dir = Path(args.data_dir)
    generate_report(data_dir, args.venue, args.date, args.format)


if __name__ == "__main__":
    main()
