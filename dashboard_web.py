#!/usr/bin/env python3
"""
HTML Dashboard for Market Surveillance System
Lightweight HTTP server with auto-refreshing view.
"""

from __future__ import annotations

import argparse
import json
import html
import os
import subprocess
from datetime import datetime, timezone
from http.server import BaseHTTPRequestHandler, HTTPServer
from pathlib import Path
from typing import Dict, List, Optional

try:
    import polars as pl
except ImportError:
    print("ERROR: polars not installed. Install with: pip install polars")
    raise


def utc_now_str() -> str:
    return datetime.now(timezone.utc).strftime("%Y-%m-%d %H:%M:%S UTC")


class DashboardData:
    def __init__(self, venue: str, date: str, data_dir: str, refresh: int) -> None:
        self.venue = venue
        self.date = date
        self.data_dir = Path(data_dir)
        self.refresh = refresh

    def load_universe(self) -> List[Dict]:
        universe_file = (
            self.data_dir
            / "metadata"
            / f"venue={self.venue}"
            / f"date={self.date}"
            / "universe.jsonl"
        )
        markets = []
        if universe_file.exists():
            for line in universe_file.read_text().splitlines():
                if not line.strip():
                    continue
                try:
                    markets.append(json.loads(line))
                except json.JSONDecodeError:
                    continue
        return markets

    def get_collector_status(self) -> Dict:
        status = {"running": False, "pid": None, "memory_mb": None}
        try:
            result = subprocess.run(
                ["systemctl", "is-active", "surveillance-collect"],
                capture_output=True,
                text=True,
            )
            if result.returncode == 0 and result.stdout.strip() == "active":
                status["running"] = True
                pid_result = subprocess.run(
                    ["systemctl", "show", "-p", "MainPID", "--value", "surveillance-collect"],
                    capture_output=True,
                    text=True,
                )
                if pid_result.returncode == 0:
                    pid = pid_result.stdout.strip()
                    status["pid"] = pid
                    mem_result = subprocess.run(
                        ["ps", "-p", pid, "-o", "rss="],
                        capture_output=True,
                        text=True,
                    )
                    if mem_result.returncode == 0 and mem_result.stdout.strip():
                        status["memory_mb"] = round(int(mem_result.stdout.strip()) / 1024, 1)
                return status
        except Exception:
            pass

        try:
            result = subprocess.run(["pgrep", "-f", "surveillance_collect"], capture_output=True, text=True)
            if result.returncode == 0:
                pid = result.stdout.strip().split("\n")[0]
                status["running"] = True
                status["pid"] = pid
                mem_result = subprocess.run(["ps", "-p", pid, "-o", "rss="], capture_output=True, text=True)
                if mem_result.returncode == 0 and mem_result.stdout.strip():
                    status["memory_mb"] = round(int(mem_result.stdout.strip()) / 1024, 1)
        except Exception:
            pass
        return status

    def get_journal_lines(self, limit: int = 200) -> List[str]:
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

    @staticmethod
    def parse_latest_metrics(lines: List[str]) -> Optional[str]:
        for line in reversed(lines):
            if "WebSocket metrics:" in line:
                return line.split("WebSocket metrics:", 1)[-1].strip()
        return None

    @staticmethod
    def parse_latest_cursor(lines: List[str]) -> Optional[str]:
        for line in reversed(lines):
            if "WARM cursor start=" in line:
                return line.split("INFO", 1)[-1].strip()
        return None

    @staticmethod
    def parse_latest_sizes(lines: List[str]) -> Optional[str]:
        for line in reversed(lines):
            if "Scheduler for" in line and "HOT" in line and "WARM" in line:
                return line.split("INFO", 1)[-1].strip()
        return None

    def get_data_stats(self) -> Dict:
        snapshot_dir = (
            self.data_dir / "orderbook_snapshots" / f"venue={self.venue}" / f"date={self.date}"
        )
        stats = {
            "total_files": 0,
            "total_size_gb": 0.0,
            "recent_files": 0,
            "hours_with_data": [],
        }
        if snapshot_dir.exists():
            parquet_files = list(snapshot_dir.rglob("*.parquet"))
            stats["total_files"] = len(parquet_files)
            total_size = sum(f.stat().st_size for f in parquet_files)
            stats["total_size_gb"] = round(total_size / (1024**3), 3)
            ten_min_ago = datetime.now().timestamp() - 600
            stats["recent_files"] = sum(1 for f in parquet_files if f.stat().st_mtime > ten_min_ago)
            hours = set()
            for f in parquet_files:
                for part in f.parts:
                    if part.startswith("hour="):
                        hours.add(part.replace("hour=", ""))
            stats["hours_with_data"] = sorted(hours)
        return stats

    def get_top_markets(self, limit: int = 10, title_map: Optional[Dict[str, str]] = None) -> List[Dict]:
        snapshot_dir = (
            self.data_dir / "orderbook_snapshots" / f"venue={self.venue}" / f"date={self.date}"
        )
        if not snapshot_dir.exists():
            return []
        parquet_files = list(snapshot_dir.rglob("*.parquet"))
        if not parquet_files:
            return []
        dfs = []
        for pf in parquet_files[:50]:
            try:
                df = pl.read_parquet(pf)
                dfs.append(df)
            except Exception:
                continue
        if not dfs:
            return []
        combined = pl.concat(dfs)
        top_markets = (
            combined.group_by(["market_id", "outcome_id"])
            .agg(
                [
                    pl.len().alias("updates"),
                    pl.mean("spread").alias("avg_spread"),
                    (pl.mean("best_bid_sz") + pl.mean("best_ask_sz")).alias("avg_depth"),
                ]
            )
            .sort("updates", descending=True)
            .head(limit)
        )
        rows = top_markets.to_dicts()
        if title_map:
            for row in rows:
                row["title"] = title_map.get(row["market_id"], "N/A")
        return rows

    def build_payload(self) -> Dict:
        markets = self.load_universe()
        title_map = {m.get("market_id", ""): m.get("title", "N/A") for m in markets if m.get("market_id")}
        journal = self.get_journal_lines()
        return {
            "venue": self.venue,
            "date": self.date,
            "refresh": self.refresh,
            "updated_at": utc_now_str(),
            "collector": self.get_collector_status(),
            "metrics": self.parse_latest_metrics(journal),
            "cursor": self.parse_latest_cursor(journal),
            "sizes": self.parse_latest_sizes(journal),
            "data_stats": self.get_data_stats(),
            "markets_total": len(markets),
            "markets_with_tokens": sum(1 for m in markets if m.get("token_ids")),
            "top_markets": self.get_top_markets(limit=10, title_map=title_map),
        }


def render_html(payload: Dict) -> str:
    collector = payload["collector"]
    collector_status = "RUNNING" if collector["running"] else "STOPPED"
    collector_status_class = "ok" if collector["running"] else "warn"
    rows_html = []
    for row in payload["top_markets"]:
        title = html.escape(row.get("title") or "N/A")
        rows_html.append(
            "<tr>"
            f"<td>{title}</td>"
            f"<td>{row.get('outcome_id')}</td>"
            f"<td>{row.get('updates')}</td>"
            f"<td>{(row.get('avg_spread') or 0):.6f}</td>"
            f"<td>{(row.get('avg_depth') or 0):.2f}</td>"
            "</tr>"
        )
    if not rows_html:
        rows_html.append('<tr><td colspan="5">No market data available</td></tr>')

    hours = payload["data_stats"]["hours_with_data"]
    hours_str = ", ".join(hours) if hours else "-"

    replacements = {
        "{{venue}}": html.escape(payload["venue"]),
        "{{date}}": html.escape(payload["date"]),
        "{{refresh}}": str(payload["refresh"]),
        "{{updated_at}}": html.escape(payload["updated_at"]),
        "{{collector_status}}": collector_status,
        "{{collector_status_class}}": collector_status_class,
        "{{collector_pid}}": str(collector.get("pid") or "-"),
        "{{collector_mem}}": str(collector.get("memory_mb") or "-"),
        "{{metrics}}": html.escape(payload.get("metrics") or "-"),
        "{{total_files}}": str(payload["data_stats"]["total_files"]),
        "{{total_size}}": str(payload["data_stats"]["total_size_gb"]),
        "{{recent_files}}": str(payload["data_stats"]["recent_files"]),
        "{{hours_with_data}}": html.escape(hours_str),
        "{{markets_total}}": str(payload["markets_total"]),
        "{{markets_with_tokens}}": str(payload["markets_with_tokens"]),
        "{{sizes}}": html.escape(payload.get("sizes") or "-"),
        "{{cursor}}": html.escape(payload.get("cursor") or "-"),
        "{{top_markets_rows}}": "\n".join(rows_html),
    }
    html_out = HTML_TEMPLATE
    for key, value in replacements.items():
        html_out = html_out.replace(key, value)
    return html_out


HTML_TEMPLATE = """<!doctype html>
<html>
  <head>
    <meta charset="utf-8" />
    <title>Surveillance Dashboard</title>
    <meta http-equiv="refresh" content="{{refresh}}" />
    <style>
      body { font-family: Arial, sans-serif; margin: 16px; color: #111; }
      .header { display: flex; justify-content: space-between; align-items: center; }
      .grid { display: grid; grid-template-columns: 1fr 1fr; gap: 16px; margin-top: 16px; }
      .card { border: 1px solid #ddd; border-radius: 6px; padding: 12px; }
      .card h3 { margin: 0 0 8px 0; }
      table { width: 100%; border-collapse: collapse; margin-top: 8px; }
      th, td { padding: 6px 8px; border-bottom: 1px solid #eee; text-align: left; }
      .muted { color: #666; }
      .ok { color: #0a7; font-weight: bold; }
      .warn { color: #b50; font-weight: bold; }
      code { font-family: Menlo, monospace; font-size: 12px; }
    </style>
  </head>
  <body>
    <div class="header">
      <div>
        <h2>Surveillance Dashboard</h2>
        <div class="muted">Venue: {{venue}} | Date: {{date}} | Refresh: {{refresh}}s</div>
      </div>
      <div class="muted">Updated: {{updated_at}}</div>
    </div>

    <div class="grid">
      <div class="card">
        <h3>System Health</h3>
        <div>Collector: <span class="{{collector_status_class}}">{{collector_status}}</span></div>
        <div class="muted">PID: {{collector_pid}} | Mem: {{collector_mem}} MB</div>
        <div class="muted">WS metrics: <code>{{metrics}}</code></div>
      </div>
      <div class="card">
        <h3>Data Coverage</h3>
        <div>Total files: {{total_files}}</div>
        <div>Total size: {{total_size}} GB</div>
        <div>Recent files (10m): {{recent_files}}</div>
        <div>Hours: {{hours_with_data}}</div>
      </div>
      <div class="card">
        <h3>Universe Progress</h3>
        <div>Markets: {{markets_total}}</div>
        <div>With token IDs: {{markets_with_tokens}}</div>
        <div class="muted">Sizes: <code>{{sizes}}</code></div>
        <div class="muted">Cursor: <code>{{cursor}}</code></div>
      </div>
      <div class="card">
        <h3>Navigation</h3>
        <div class="muted">This is a read-only dashboard. Use CLI tools for actions.</div>
        <div class="muted">Log tail: <code>sudo journalctl -u surveillance-collect -f</code></div>
      </div>
    </div>

    <div class="card" style="margin-top:16px;">
      <h3>Top Markets (updates)</h3>
      <table>
        <thead>
          <tr><th>Market</th><th>Outcome</th><th>Updates</th><th>Avg Spread</th><th>Avg Depth</th></tr>
        </thead>
        <tbody>
          {{top_markets_rows}}
        </tbody>
      </table>
    </div>
  </body>
</html>
"""


class Handler(BaseHTTPRequestHandler):
    def do_GET(self) -> None:
        if self.path == "/":
            payload = self.server.data_builder.build_payload()
            content = render_html(payload).encode()
            self.send_response(200)
            self.send_header("Content-Type", "text/html; charset=utf-8")
            self.send_header("Content-Length", str(len(content)))
            self.end_headers()
            self.wfile.write(content)
            return
        if self.path == "/data":
            payload = self.server.data_builder.build_payload()
            content = json.dumps(payload).encode()
            self.send_response(200)
            self.send_header("Content-Type", "application/json")
            self.send_header("Content-Length", str(len(content)))
            self.end_headers()
            self.wfile.write(content)
            return
        self.send_response(404)
        self.end_headers()


def main() -> None:
    parser = argparse.ArgumentParser(description="HTML Dashboard for Market Surveillance")
    parser.add_argument("venue", nargs="?", default="polymarket", help="Venue name")
    parser.add_argument("--date", help="Date in YYYY-MM-DD format (default: today UTC)")
    parser.add_argument("--refresh", type=int, default=5, help="Refresh interval in seconds")
    parser.add_argument("--port", type=int, default=8787, help="HTTP port (default: 8787)")
    parser.add_argument("--data-dir", default="data", help="Base data directory (default: data)")
    args = parser.parse_args()

    date = args.date or datetime.now(timezone.utc).strftime("%Y-%m-%d")
    data_builder = DashboardData(args.venue, date, args.data_dir, args.refresh)

    server = HTTPServer(("0.0.0.0", args.port), Handler)
    server.data_builder = data_builder  # type: ignore[attr-defined]
    server.refresh = args.refresh  # type: ignore[attr-defined]

    print(f"Dashboard running on http://localhost:{args.port}")
    try:
        server.serve_forever()
    except KeyboardInterrupt:
        pass


if __name__ == "__main__":
    main()
