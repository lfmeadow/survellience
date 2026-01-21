#!/usr/bin/env python3
"""
Check parsed constraints against market prices to find arbitrage opportunities.
"""

import argparse
import json
from pathlib import Path
from datetime import datetime, timezone
from typing import Dict, List, Optional
import sys

try:
    import polars as pl
except ImportError:
    print("pip install polars")
    sys.exit(1)


def load_parsed_propositions(data_dir: Path) -> tuple[List[dict], List[dict]]:
    """Load all LLM-parsed propositions and constraints."""
    parsed_dir = data_dir / "llm_parsed"
    
    all_props = []
    all_constraints = []
    
    for f in parsed_dir.glob("*.json"):
        data = json.loads(f.read_text())
        all_props.extend(data.get("propositions", []))
        all_constraints.extend(data.get("constraints", []))
    
    return all_props, all_constraints


def load_prices(data_dir: Path, venue: str, date: str) -> Dict[str, float]:
    """Load latest prices from snapshots."""
    snap_dir = data_dir / "orderbook_snapshots" / f"venue={venue}" / f"date={date}"
    
    prices = {}
    if not snap_dir.exists():
        return prices
    
    for pq in snap_dir.glob("**/*.parquet"):
        try:
            df = pl.read_parquet(pq)
            for row in df.iter_rows(named=True):
                mid = row.get("mid")
                if mid is not None:
                    prices[row["market_id"]] = mid
        except:
            pass
    
    return prices


def check_time_ladder(constraint: dict, prices: Dict[str, float]) -> Optional[dict]:
    """Check if a time ladder constraint is violated."""
    market_ids = constraint.get("market_ids", [])
    
    # Get prices for all markets in ladder
    ladder_prices = []
    for mid in market_ids:
        if mid in prices:
            ladder_prices.append((mid, prices[mid]))
    
    if len(ladder_prices) < 2:
        return None
    
    # Check monotonicity (prices should be non-decreasing for time ladders)
    violations = []
    for i in range(len(ladder_prices) - 1):
        if ladder_prices[i][1] > ladder_prices[i+1][1] + 0.01:  # 1% tolerance
            violations.append({
                "earlier_market": ladder_prices[i][0],
                "earlier_price": ladder_prices[i][1],
                "later_market": ladder_prices[i+1][0],
                "later_price": ladder_prices[i+1][1],
                "violation_magnitude": ladder_prices[i][1] - ladder_prices[i+1][1],
            })
    
    if violations:
        return {
            "constraint_type": "time_ladder",
            "group": constraint.get("group"),
            "relation": constraint.get("relation"),
            "violations": violations,
        }
    return None


def check_exhaustive_partition(constraint: dict, prices: Dict[str, float]) -> Optional[dict]:
    """Check if bucket probabilities sum to ~1."""
    market_ids = constraint.get("market_ids", [])
    
    total = 0.0
    found = 0
    for mid in market_ids:
        if mid in prices:
            total += prices[mid]
            found += 1
    
    if found < len(market_ids) * 0.8:  # Need at least 80% of markets
        return None
    
    # Should sum to ~1 (within 5%)
    if abs(total - 1.0) > 0.05:
        return {
            "constraint_type": "exhaustive_partition",
            "group": constraint.get("group"),
            "relation": constraint.get("relation"),
            "expected_sum": 1.0,
            "actual_sum": total,
            "violation_magnitude": abs(total - 1.0),
            "arbitrage_direction": "BUY_ALL" if total < 1.0 else "SELL_ALL",
        }
    return None


def check_implied_threshold(constraint: dict, prices: Dict[str, float]) -> Optional[dict]:
    """Check if threshold market equals sum of buckets above threshold."""
    market_ids = constraint.get("market_ids", [])
    if len(market_ids) < 2:
        return None
    
    threshold_id = market_ids[0]  # First one is threshold market
    bucket_ids = market_ids[1:]
    
    if threshold_id not in prices:
        return None
    
    bucket_sum = sum(prices.get(mid, 0) for mid in bucket_ids)
    threshold_price = prices[threshold_id]
    
    diff = abs(threshold_price - bucket_sum)
    if diff > 0.02:  # 2% tolerance
        return {
            "constraint_type": "implied_threshold",
            "group": constraint.get("group"),
            "threshold_market": threshold_id,
            "threshold_price": threshold_price,
            "bucket_sum": bucket_sum,
            "violation_magnitude": diff,
        }
    return None


def main():
    parser = argparse.ArgumentParser(description="Check constraints for arbitrage")
    parser.add_argument("--data-dir", default="data", help="Data directory")
    parser.add_argument("--venue", default="polymarket", help="Venue")
    parser.add_argument("--date", default=datetime.now(timezone.utc).strftime("%Y-%m-%d"))
    args = parser.parse_args()
    
    data_dir = Path(args.data_dir)
    
    # Load parsed data
    props, constraints = load_parsed_propositions(data_dir)
    print(f"Loaded {len(props)} propositions and {len(constraints)} constraints")
    
    # Load prices
    prices = load_prices(data_dir, args.venue, args.date)
    print(f"Loaded prices for {len(prices)} markets")
    
    # Check each constraint
    violations = []
    for constraint in constraints:
        ctype = constraint.get("constraint_type")
        
        if ctype == "time_ladder":
            v = check_time_ladder(constraint, prices)
        elif ctype == "exhaustive_partition":
            v = check_exhaustive_partition(constraint, prices)
        elif ctype == "implied_threshold":
            v = check_implied_threshold(constraint, prices)
        else:
            v = None
        
        if v:
            violations.append(v)
    
    # Report
    print(f"\n{'='*60}")
    print(f"CONSTRAINT VIOLATIONS ({len(violations)} found)")
    print('='*60)
    
    if not violations:
        print("No violations found - constraints are satisfied!")
    else:
        for v in violations:
            print(f"\n📛 {v['constraint_type'].upper()}: {v.get('group', 'unknown')}")
            print(f"   Relation: {v.get('relation', 'N/A')}")
            if 'violations' in v:
                for sub_v in v['violations']:
                    print(f"   ⚠️  {sub_v['earlier_price']:.2%} > {sub_v['later_price']:.2%} (diff: {sub_v['violation_magnitude']:.2%})")
            elif 'actual_sum' in v:
                print(f"   ⚠️  Sum = {v['actual_sum']:.2%}, expected 100%")
                print(f"   💡 Arb: {v.get('arbitrage_direction')}")
            elif 'bucket_sum' in v:
                print(f"   ⚠️  Threshold = {v['threshold_price']:.2%}, Bucket sum = {v['bucket_sum']:.2%}")


if __name__ == "__main__":
    main()
