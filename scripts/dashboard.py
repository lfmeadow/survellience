#!/usr/bin/env python3
"""
Live Dashboard for Market Surveillance System
Interactive dashboard with market exploration and drill-down capabilities
"""

import os
import sys
import json
import time
import signal
import argparse
import re
import shutil
from datetime import datetime, timedelta, timezone
from pathlib import Path
from typing import Optional, Dict, List, Tuple
from collections import defaultdict

try:
    import polars as pl
except ImportError:
    print("ERROR: polars not installed. Install with: pip install polars")
    sys.exit(1)


class Dashboard:
    def __init__(self, venue: str = "polymarket", date: Optional[str] = None, refresh_interval: int = 5):
        self.venue = venue
        self.date = date or datetime.now(timezone.utc).strftime("%Y-%m-%d")
        self.refresh_interval = refresh_interval
        self.running = True
        self.current_view = "overview"  # overview, markets, market_detail
        self.selected_market_index = 0
        self.markets = []
        self.market_stats = {}
        self.data_dir = Path("data")
        
        # Register signal handler for graceful exit
        signal.signal(signal.SIGINT, self._signal_handler)
        signal.signal(signal.SIGTERM, self._signal_handler)
    
    def _signal_handler(self, signum, frame):
        """Handle Ctrl+C gracefully"""
        self.running = False
        self.clear_screen()
        print("\nDashboard stopped.")
        sys.exit(0)
    
    def clear_screen(self):
        """Clear terminal screen"""
        os.system('clear' if os.name != 'nt' else 'cls')
    
    def load_universe(self) -> List[Dict]:
        """Load market universe from JSONL file"""
        universe_file = self.data_dir / "metadata" / f"venue={self.venue}" / f"date={self.date}" / "universe.jsonl"
        markets = []
        
        if universe_file.exists():
            with open(universe_file, 'r') as f:
                for line in f:
                    line = line.strip()
                    if line:
                        try:
                            market = json.loads(line)
                            markets.append(market)
                        except json.JSONDecodeError:
                            continue
        
        return markets
    
    def load_stats(self) -> Dict[str, Dict]:
        """Load market statistics from stats cache"""
        stats_file = self.data_dir / "stats" / f"venue={self.venue}" / f"date={self.date}" / "stats.csv"
        stats = {}
        
        if stats_file.exists():
            try:
                df = pl.read_csv(stats_file)
                for row in df.iter_rows(named=True):
                    key = f"{row['market_id']}|{row['outcome_id']}"
                    stats[key] = {
                        'avg_depth': row.get('avg_depth', 0),
                        'avg_spread': row.get('avg_spread', 0),
                        'update_count': row.get('update_count', 0)
                    }
            except Exception as e:
                pass
        
        return stats
    
    def get_collector_status(self) -> Tuple[bool, Optional[Dict]]:
        """Check if collector is running"""
        import subprocess
        try:
            result = subprocess.run(['pgrep', '-f', 'surveillance_collect'], 
                                  capture_output=True, text=True)
            is_running = result.returncode == 0
            
            info = {}
            if is_running:
                pid = result.stdout.strip().split('\n')[0]
                try:
                    # Get memory usage
                    result = subprocess.run(['ps', '-p', pid, '-o', 'rss='], 
                                          capture_output=True, text=True)
                    if result.returncode == 0:
                        rss_kb = int(result.stdout.strip())
                        info['pid'] = pid
                        info['memory_mb'] = rss_kb / 1024
                except:
                    pass
            
            return is_running, info if info else None
        except:
            return False, None

    def get_journal_lines(self, limit: int = 200) -> List[str]:
        """Fetch recent journald lines for collector"""
        import subprocess
        try:
            result = subprocess.run(
                ["journalctl", "-u", "surveillance-collect", "-n", str(limit), "--no-pager"],
                capture_output=True,
                text=True,
            )
            if result.returncode != 0:
                return []
            return [line for line in result.stdout.splitlines() if line.strip()]
        except Exception:
            return []

    def parse_latest_metrics(self, lines: List[str]) -> Optional[str]:
        for line in reversed(lines):
            if "WebSocket metrics:" in line:
                return line.split("WebSocket metrics:", 1)[-1].strip()
        return None

    def parse_latest_warm_cursor(self, lines: List[str]) -> Optional[str]:
        for line in reversed(lines):
            if "WARM cursor start=" in line:
                return line.split("INFO", 1)[-1].strip()
        return None

    def parse_latest_scheduler_sizes(self, lines: List[str]) -> Optional[str]:
        for line in reversed(lines):
            if "Scheduler for" in line and "HOT" in line and "WARM" in line:
                return line.split("INFO", 1)[-1].strip()
        return None
    
    def get_data_stats(self) -> Dict:
        """Get data collection statistics"""
        snapshot_dir = self.data_dir / "orderbook_snapshots" / f"venue={self.venue}" / f"date={self.date}"
        stats = {
            'total_files': 0,
            'total_size_gb': 0,
            'recent_files': 0,
            'hours_with_data': set()
        }
        
        if snapshot_dir.exists():
            parquet_files = list(snapshot_dir.rglob("*.parquet"))
            stats['total_files'] = len(parquet_files)
            
            total_size = sum(f.stat().st_size for f in parquet_files)
            stats['total_size_gb'] = total_size / (1024**3)
            
            # Count recent files (last 10 minutes)
            ten_min_ago = time.time() - 600
            stats['recent_files'] = sum(1 for f in parquet_files if f.stat().st_mtime > ten_min_ago)
            
            # Get hours with data
            for f in parquet_files:
                parts = f.parts
                for i, part in enumerate(parts):
                    if part.startswith("hour="):
                        hour = part.replace("hour=", "")
                        stats['hours_with_data'].add(hour)
        
        stats['hours_with_data'] = sorted(stats['hours_with_data'])
        return stats
    
    def get_top_markets(self, limit: int = 10) -> List[Dict]:
        """Get top markets by update count"""
        snapshot_dir = self.data_dir / "orderbook_snapshots" / f"venue={self.venue}" / f"date={self.date}"
        
        if not snapshot_dir.exists():
            return []
        
        try:
            # Read recent parquet files
            parquet_files = list(snapshot_dir.rglob("*.parquet"))
            if not parquet_files:
                return []
            
            # Read all files (this might be slow for large datasets, consider limiting)
            dfs = []
            for pf in parquet_files[:50]:  # Limit to 50 most recent files for performance
                try:
                    df = pl.read_parquet(pf)
                    dfs.append(df)
                except:
                    continue
            
            if not dfs:
                return []
            
            combined = pl.concat(dfs)
            
            # Group by market_id and outcome_id
            top_markets = combined.group_by(['market_id', 'outcome_id']).agg([
                pl.len().alias('updates'),
                pl.mean('spread').alias('avg_spread'),
                (pl.mean('best_bid_sz') + pl.mean('best_ask_sz')).alias('avg_depth'),
                pl.max('ts_recv').alias('last_update'),
            ]).sort('updates', descending=True).head(limit)
            
            # Convert to list of dicts
            result = []
            for row in top_markets.iter_rows(named=True):
                result.append({
                    'market_id': row['market_id'],
                    'outcome_id': row['outcome_id'],
                    'updates': row['updates'],
                    'avg_spread': row['avg_spread'],
                    'avg_depth': row['avg_depth'],
                    'last_update': row['last_update']
                })
            
            return result
        except Exception as e:
            return []
    
    def get_market_detail(self, market_id: str, outcome_id: str = "0") -> Optional[Dict]:
        """Get detailed data for a specific market"""
        snapshot_dir = self.data_dir / "orderbook_snapshots" / f"venue={self.venue}" / f"date={self.date}"
        
        if not snapshot_dir.exists():
            return None
        
        try:
            parquet_files = list(snapshot_dir.rglob("*.parquet"))
            
            # Filter for this market
            dfs = []
            for pf in parquet_files[:100]:  # Check up to 100 files
                try:
                    df = pl.read_parquet(pf)
                    filtered = df.filter(
                        (pl.col('market_id') == market_id) & 
                        (pl.col('outcome_id') == outcome_id)
                    )
                    if len(filtered) > 0:
                        dfs.append(filtered)
                except:
                    continue
            
            if not dfs:
                return None
            
            combined = pl.concat(dfs)
            
            # Get latest snapshot
            latest = combined.sort('ts_recv', descending=True).head(1)
            
            if len(latest) == 0:
                return None
            
            row = latest.iter_rows(named=True)[0]
            
            return {
                'market_id': row['market_id'],
                'outcome_id': row['outcome_id'],
                'snapshots': len(combined),
                'latest': {
                    'ts_recv': row['ts_recv'],
                    'best_bid_px': row.get('best_bid_px'),
                    'best_bid_sz': row.get('best_bid_sz'),
                    'best_ask_px': row.get('best_ask_px'),
                    'best_ask_sz': row.get('best_ask_sz'),
                    'mid': row.get('mid'),
                    'spread': row.get('spread'),
                },
                'stats': {
                    'avg_spread': combined['spread'].mean(),
                    'avg_depth': (combined['best_bid_sz'] + combined['best_ask_sz']).mean(),
                    'min_spread': combined['spread'].min(),
                    'max_spread': combined['spread'].max(),
                }
            }
        except Exception as e:
            return None
    
    def render_overview(self):
        """Render overview screen"""
        collector_running, collector_info = self.get_collector_status()
        data_stats = self.get_data_stats()
        top_markets = self.get_top_markets(limit=10)
        journal_lines = self.get_journal_lines()
        latest_metrics = self.parse_latest_metrics(journal_lines)
        latest_cursor = self.parse_latest_warm_cursor(journal_lines)
        latest_sizes = self.parse_latest_scheduler_sizes(journal_lines)
        now_str = datetime.now(timezone.utc).strftime("%Y-%m-%d %H:%M:%S UTC")
        
        term_width = shutil.get_terminal_size((120, 40)).columns
        col_width = max(40, (term_width - 3) // 2)

        def block(title: str, lines: List[str]) -> List[str]:
            header = f"{title}".ljust(col_width)
            underline = "-" * min(col_width, len(title) + 2)
            content = [line[:col_width].ljust(col_width) for line in lines]
            return [header, underline] + content

        header = (
            f"Market Surveillance Dashboard | {self.venue.upper()} | {self.date} | "
            f"refresh={self.refresh_interval}s | {now_str}"
        )
        print("=" * min(term_width, len(header)))
        print(header)
        print("=" * min(term_width, len(header)))
        print()

        # Left column blocks
        health_lines = []
        if collector_running:
            status_str = "RUNNING"
            if collector_info:
                status_str += f" (PID {collector_info.get('pid', '?')}, {collector_info.get('memory_mb', 0):.1f} MB)"
            health_lines.append(f"Collector: {status_str}")
        else:
            health_lines.append("Collector: NOT RUNNING")
        if latest_metrics:
            health_lines.append(f"WS: {latest_metrics}")

        universe_lines = [f"Markets: {len(self.markets)}"]
        if self.markets:
            with_tokens = sum(1 for m in self.markets if m.get('token_ids'))
            universe_lines.append(f"With token IDs: {with_tokens}")
        if latest_sizes:
            universe_lines.append(latest_sizes)
        if latest_cursor:
            universe_lines.append(latest_cursor)

        left_blocks = block("SYSTEM HEALTH", health_lines) + [" " * col_width] + block("UNIVERSE PROGRESS", universe_lines)

        # Right column blocks
        coverage_lines = [
            f"Total files: {data_stats['total_files']}",
            f"Total size: {data_stats['total_size_gb']:.2f} GB",
            f"Recent files (10m): {data_stats['recent_files']}",
        ]
        if data_stats['hours_with_data']:
            coverage_lines.append(f"Hours: {', '.join(data_stats['hours_with_data'])}")

        nav_lines = [
            "m: markets list",
            "b: back",
            "q: quit",
        ]

        right_blocks = block("DATA COVERAGE", coverage_lines) + [" " * col_width] + block("NAVIGATION", nav_lines)

        max_rows = max(len(left_blocks), len(right_blocks))
        left_blocks.extend([" " * col_width] * (max_rows - len(left_blocks)))
        right_blocks.extend([" " * col_width] * (max_rows - len(right_blocks)))

        for l, r in zip(left_blocks, right_blocks):
            print(f"{l} | {r}")

        print()
        print("TOP MARKETS (updates)")
        print("-" * min(term_width, 80))
        if top_markets:
            print(f"{'Title':<50} {'Out':<4} {'Upd':<6} {'Spr':<10} {'Depth':<10}")
            print("-" * min(term_width, 80))
            for m in top_markets[:10]:
                market_title = next((mkt['title'] for mkt in self.markets if mkt['market_id'] == m['market_id']), 'N/A')
                if len(market_title) > 48:
                    market_title = market_title[:45] + "..."
                print(
                    f"{market_title:<50} {m['outcome_id']:<4} {m['updates']:<6} "
                    f"{m['avg_spread']:<10.6f} {m['avg_depth']:<10.2f}"
                )
        else:
            print("No market data available yet")
        print()
    
    def render_markets_list(self):
        """Render markets list screen"""
        print("=" * 80)
        print(f"  Markets List - {self.venue.upper()} | {self.date}")
        print("=" * 80)
        print()
        
        if not self.markets:
            print("No markets found. Run scanner to discover markets.")
            print()
            print("Press 'b' to go back | 'q' to quit")
            return
        
        # Show markets with stats if available
        page_size = 15
        start_idx = max(0, min(self.selected_market_index - page_size // 2, len(self.markets) - page_size))
        end_idx = min(len(self.markets), start_idx + page_size)
        
        print(f"Showing markets {start_idx + 1}-{end_idx} of {len(self.markets)} (selected: {self.selected_market_index + 1})")
        print("-" * 80)
        
        # Display markets (scrollable window)
        for i in range(start_idx, end_idx):
            market = self.markets[i]
            marker = "▶ " if i == self.selected_market_index else "  "
            
            # Get stats for this market
            stats_info = ""
            total_updates = 0
            for outcome_id in market.get('outcome_ids', []):
                key = f"{market['market_id']}|{outcome_id}"
                if key in self.market_stats:
                    s = self.market_stats[key]
                    total_updates += s['update_count']
                    if stats_info:
                        stats_info += ", "
                    stats_info += f"{outcome_id}: {s['update_count']} updates"
            
            title = market['title'][:65] + "..." if len(market['title']) > 65 else market['title']
            status_icon = "🟢" if market.get('status') == 'active' else "🔴"
            
            print(f"{marker}{status_icon} {title}")
            print(f"    ID: {market['market_id'][:55]}...")
            if market.get('outcome_ids'):
                print(f"    Outcomes: {', '.join(market.get('outcome_ids', []))}")
            if stats_info:
                print(f"    Stats: {stats_info}")
            print()
        
        print("-" * 80)
        print("↑/↓ or j/k: Navigate | Enter: View details | 'b': Back | 'q': Quit")
    
    def render_market_detail(self, market_index: int):
        """Render detailed view for a specific market"""
        if market_index < 0 or market_index >= len(self.markets):
            return
        
        market = self.markets[market_index]
        
        print("=" * 80)
        print(f"  Market Details")
        print("=" * 80)
        print()
        print(f"Title: {market['title']}")
        print(f"Market ID: {market['market_id']}")
        print(f"Status: {market.get('status', 'N/A')}")
        print(f"Outcomes: {', '.join(market.get('outcome_ids', []))}")
        if market.get('tags'):
            print(f"Tags: {', '.join(market['tags'])}")
        if market.get('close_ts'):
            close_dt = datetime.fromtimestamp(market['close_ts'] / 1000)
            print(f"Close Date: {close_dt.strftime('%Y-%m-%d %H:%M:%S UTC')}")
        print()
        
        # Show data for each outcome
        for outcome_id in market.get('outcome_ids', ['0']):
            print(f"-" * 80)
            print(f"Outcome: {outcome_id}")
            print("-" * 80)
            
            detail = self.get_market_detail(market['market_id'], outcome_id)
            if detail:
                print(f"Snapshots collected: {detail['snapshots']}")
                print()
                print("Latest Snapshot:")
                if detail['latest']:
                    latest = detail['latest']
                    print(f"  Timestamp: {datetime.fromtimestamp(latest['ts_recv']/1000).strftime('%Y-%m-%d %H:%M:%S UTC')}")
                    print(f"  Best Bid: {latest.get('best_bid_px', 0):.6f} @ {latest.get('best_bid_sz', 0):.2f}")
                    print(f"  Best Ask: {latest.get('best_ask_px', 0):.6f} @ {latest.get('best_ask_sz', 0):.2f}")
                    print(f"  Mid Price: {latest.get('mid', 0):.6f}")
                    print(f"  Spread: {latest.get('spread', 0):.6f}")
                print()
                print("Statistics:")
                stats = detail['stats']
                print(f"  Avg Spread: {stats['avg_spread']:.6f}")
                print(f"  Min Spread: {stats['min_spread']:.6f}")
                print(f"  Max Spread: {stats['max_spread']:.6f}")
                print(f"  Avg Depth: {stats['avg_depth']:.2f}")
            else:
                print("No data collected yet for this outcome")
            print()
        
        print("-" * 80)
        print("Press 'b' to go back | 'q' to quit")
    
    def handle_input(self) -> bool:
        """Handle keyboard input (non-blocking)"""
        try:
            import select
            import tty
            import termios
            
            old_settings = termios.tcgetattr(sys.stdin)
            try:
                tty.setcbreak(sys.stdin.fileno())
                
                if select.select([sys.stdin], [], [], 0.0)[0]:
                    char = sys.stdin.read(1)
                    
                    if char == 'q':
                        return False
                    elif char == 'r':
                        # Refresh
                        return True
                    elif char == 'm' and self.current_view == "overview":
                        self.current_view = "markets"
                        self.selected_market_index = 0
                    elif char == 'b' and self.current_view != "overview":
                        if self.current_view == "market_detail":
                            self.current_view = "markets"
                        else:
                            self.current_view = "overview"
                    elif char == '\n' and self.current_view == "markets":
                        # Enter key - view market detail
                        self.current_view = "market_detail"
                    elif char == 'k' and self.current_view == "markets":  # Up (vi-style)
                        self.selected_market_index = max(0, self.selected_market_index - 1)
                    elif char == 'j' and self.current_view == "markets":  # Down (vi-style)
                        self.selected_market_index = min(len(self.markets) - 1, self.selected_market_index + 1)
                    elif char == '\x1b':  # ESC sequence (arrow keys)
                        # Handle arrow keys
                        if select.select([sys.stdin], [], [], 0.0)[0]:
                            seq = sys.stdin.read(2)
                            if seq == '[A' and self.current_view == "markets":  # Up arrow
                                self.selected_market_index = max(0, self.selected_market_index - 1)
                            elif seq == '[B' and self.current_view == "markets":  # Down arrow
                                self.selected_market_index = min(len(self.markets) - 1, self.selected_market_index + 1)
                    
            finally:
                termios.tcsetattr(sys.stdin, termios.TCSADRAIN, old_settings)
        except (ImportError, OSError):
            # Fallback for systems without termios (Windows, etc.)
            # Just continue without input handling
            pass
        
        return True
    
    def run(self):
        """Main dashboard loop"""
        next_refresh = time.monotonic()
        while self.running:
            now = time.monotonic()
            if now >= next_refresh:
                # Load data
                self.markets = self.load_universe()
                self.market_stats = self.load_stats()

                # Clear and render
                self.clear_screen()

                if self.current_view == "overview":
                    self.render_overview()
                elif self.current_view == "markets":
                    self.render_markets_list()
                elif self.current_view == "market_detail":
                    self.render_market_detail(self.selected_market_index)

                next_refresh = now + self.refresh_interval

            # Handle input (non-blocking)
            if not self.handle_input():
                break

            time.sleep(0.05)


def main():
    parser = argparse.ArgumentParser(description="Live Dashboard for Market Surveillance System")
    parser.add_argument("venue", nargs="?", default="polymarket", help="Venue name (default: polymarket)")
    parser.add_argument("--date", help="Date in YYYY-MM-DD format (default: today UTC)")
    parser.add_argument("--refresh", type=int, default=5, help="Refresh interval in seconds (default: 5)")
    
    args = parser.parse_args()
    
    dashboard = Dashboard(
        venue=args.venue,
        date=args.date,
        refresh_interval=args.refresh
    )
    
    try:
        dashboard.run()
    except KeyboardInterrupt:
        dashboard.clear_screen()
        print("\nDashboard stopped.")


if __name__ == "__main__":
    main()
