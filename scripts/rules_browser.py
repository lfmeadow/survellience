#!/usr/bin/env python3
"""
Rules Pipeline Browser - View propositions, constraints, and violations.
"""

from __future__ import annotations

import argparse
import json
import html
import os
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


class RulesData:
    def __init__(self, venue: str, date: str, data_dir: str) -> None:
        self.venue = venue
        self.date = date
        self.data_dir = Path(data_dir)

    def load_rules(self) -> List[Dict]:
        rules_file = (
            self.data_dir / "rules" / f"venue={self.venue}" / f"date={self.date}" / "rules.jsonl"
        )
        rules = []
        if rules_file.exists():
            for line in rules_file.read_text().splitlines():
                if line.strip():
                    try:
                        rules.append(json.loads(line))
                    except json.JSONDecodeError:
                        pass
        return rules

    def load_propositions(self) -> Optional[pl.DataFrame]:
        path = self.data_dir / "logic" / f"venue={self.venue}" / f"date={self.date}" / "propositions.parquet"
        if path.exists():
            return pl.read_parquet(path)
        return None

    def load_constraints(self) -> Optional[pl.DataFrame]:
        path = self.data_dir / "logic" / f"venue={self.venue}" / f"date={self.date}" / "constraints.parquet"
        if path.exists():
            return pl.read_parquet(path)
        return None

    def load_violations(self) -> Optional[pl.DataFrame]:
        path = self.data_dir / "logic" / f"venue={self.venue}" / f"date={self.date}" / "violations.parquet"
        if path.exists():
            return pl.read_parquet(path)
        return None

    def load_review_queue(self) -> List[Dict]:
        queue_file = (
            self.data_dir / "review_queue" / f"venue={self.venue}" / f"date={self.date}" / "queue.jsonl"
        )
        items = []
        if queue_file.exists():
            for line in queue_file.read_text().splitlines():
                if line.strip():
                    try:
                        items.append(json.loads(line))
                    except json.JSONDecodeError:
                        pass
        return items


def generate_html(data: RulesData, tab: str = "rules") -> str:
    rules = data.load_rules()
    propositions = data.load_propositions()
    constraints = data.load_constraints()
    violations = data.load_violations()
    review_queue = data.load_review_queue()

    # Count stats
    rules_count = len(rules)
    props_count = len(propositions) if propositions is not None else 0
    constraints_count = len(constraints) if constraints is not None else 0
    violations_count = len(violations) if violations is not None else 0
    review_count = len(review_queue)

    tabs = [
        ("rules", f"Rules ({rules_count})"),
        ("propositions", f"Propositions ({props_count})"),
        ("constraints", f"Constraints ({constraints_count})"),
        ("violations", f"Violations ({violations_count})"),
        ("review", f"Review Queue ({review_count})"),
    ]

    tab_html = ""
    for t, label in tabs:
        active = "active" if t == tab else ""
        tab_html += f'<a href="?tab={t}" class="tab {active}">{label}</a>'

    content = ""
    if tab == "rules":
        content = render_rules(rules)
    elif tab == "propositions":
        content = render_propositions(propositions)
    elif tab == "constraints":
        content = render_constraints(constraints)
    elif tab == "violations":
        content = render_violations(violations)
    elif tab == "review":
        content = render_review(review_queue)

    return f"""<!DOCTYPE html>
<html>
<head>
    <title>Rules Browser - {data.venue} - {data.date}</title>
    <meta charset="utf-8">
    <style>
        body {{ font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, sans-serif; margin: 0; padding: 20px; background: #f5f5f5; }}
        .header {{ background: #1a1a2e; color: white; padding: 20px; margin: -20px -20px 20px -20px; }}
        .header h1 {{ margin: 0 0 10px 0; }}
        .header .meta {{ color: #888; font-size: 14px; }}
        .tabs {{ display: flex; gap: 5px; margin-bottom: 20px; flex-wrap: wrap; }}
        .tab {{ padding: 10px 20px; background: #ddd; text-decoration: none; color: #333; border-radius: 5px 5px 0 0; }}
        .tab.active {{ background: white; font-weight: bold; }}
        .tab:hover {{ background: #ccc; }}
        .content {{ background: white; padding: 20px; border-radius: 5px; box-shadow: 0 2px 5px rgba(0,0,0,0.1); }}
        table {{ width: 100%; border-collapse: collapse; font-size: 13px; }}
        th, td {{ padding: 8px 12px; text-align: left; border-bottom: 1px solid #eee; }}
        th {{ background: #f8f8f8; font-weight: 600; position: sticky; top: 0; }}
        tr:hover {{ background: #f5f5f5; }}
        .truncate {{ max-width: 300px; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }}
        .badge {{ display: inline-block; padding: 2px 8px; border-radius: 10px; font-size: 11px; font-weight: bold; }}
        .badge-green {{ background: #d4edda; color: #155724; }}
        .badge-red {{ background: #f8d7da; color: #721c24; }}
        .badge-yellow {{ background: #fff3cd; color: #856404; }}
        .badge-blue {{ background: #cce5ff; color: #004085; }}
        .empty {{ text-align: center; padding: 40px; color: #888; }}
        pre {{ background: #f8f8f8; padding: 10px; border-radius: 5px; overflow-x: auto; font-size: 12px; }}
        .search {{ margin-bottom: 15px; }}
        .search input {{ padding: 8px 12px; width: 300px; border: 1px solid #ddd; border-radius: 5px; }}
    </style>
</head>
<body>
    <div class="header">
        <h1>Rules Pipeline Browser</h1>
        <div class="meta">Venue: {data.venue} | Date: {data.date} | Updated: {utc_now_str()}</div>
    </div>
    <div class="tabs">{tab_html}</div>
    <div class="content">{content}</div>
    <script>
        function filterTable(inputId, tableId) {{
            const filter = document.getElementById(inputId).value.toLowerCase();
            const rows = document.querySelectorAll('#' + tableId + ' tbody tr');
            rows.forEach(row => {{
                const text = row.textContent.toLowerCase();
                row.style.display = text.includes(filter) ? '' : 'none';
            }});
        }}
    </script>
</body>
</html>"""


def render_rules(rules: List[Dict]) -> str:
    if not rules:
        return '<div class="empty">No rules fetched yet. Run: cargo run --bin surveillance_rules -- ingest</div>'

    rows = ""
    for r in rules[:500]:  # Limit to 500 for performance
        title = html.escape(r.get("title", "")[:80])
        market_id = r.get("market_id", "")[:20] + "..."
        rules_text = html.escape(r.get("raw_rules_text", "")[:150])
        url = r.get("url", "")
        
        rows += f"""<tr>
            <td class="truncate" title="{html.escape(r.get('market_id', ''))}">{market_id}</td>
            <td class="truncate" title="{html.escape(r.get('title', ''))}">{title}</td>
            <td class="truncate" title="{html.escape(r.get('raw_rules_text', ''))}">{rules_text}</td>
            <td><a href="{url}" target="_blank">Link</a></td>
        </tr>"""

    return f"""
    <div class="search">
        <input type="text" id="rulesSearch" placeholder="Filter rules..." onkeyup="filterTable('rulesSearch', 'rulesTable')">
        <span style="color:#888; margin-left:10px;">Showing {min(len(rules), 500)} of {len(rules)}</span>
    </div>
    <table id="rulesTable">
        <thead><tr><th>Market ID</th><th>Title</th><th>Rules Text</th><th>URL</th></tr></thead>
        <tbody>{rows}</tbody>
    </table>"""


def render_propositions(df: Optional[pl.DataFrame]) -> str:
    if df is None or len(df) == 0:
        return '<div class="empty">No propositions yet. Run: cargo run --bin surveillance_rules -- normalize</div>'

    rows = ""
    for row in df.head(500).iter_rows(named=True):
        confidence = row.get("confidence", 0)
        conf_class = "badge-green" if confidence >= 0.8 else "badge-yellow" if confidence >= 0.6 else "badge-red"
        
        rows += f"""<tr>
            <td class="truncate">{row.get('market_id', '')[:20]}...</td>
            <td>{html.escape(str(row.get('underlier', '') or ''))}</td>
            <td>{row.get('strike_level', '')}</td>
            <td>{row.get('comparator', '')}</td>
            <td>{row.get('proposition_kind', '')}</td>
            <td><span class="badge {conf_class}">{confidence:.2f}</span></td>
        </tr>"""

    return f"""
    <div class="search">
        <input type="text" id="propsSearch" placeholder="Filter propositions..." onkeyup="filterTable('propsSearch', 'propsTable')">
        <span style="color:#888; margin-left:10px;">Showing {min(len(df), 500)} of {len(df)}</span>
    </div>
    <table id="propsTable">
        <thead><tr><th>Market ID</th><th>Underlier</th><th>Strike</th><th>Comparator</th><th>Kind</th><th>Confidence</th></tr></thead>
        <tbody>{rows}</tbody>
    </table>"""


def render_constraints(df: Optional[pl.DataFrame]) -> str:
    if df is None or len(df) == 0:
        return '<div class="empty">No constraints yet. Run: cargo run --bin surveillance_rules -- constraints</div>'

    rows = ""
    for row in df.head(500).iter_rows(named=True):
        rows += f"""<tr>
            <td>{row.get('constraint_type', '')}</td>
            <td>{html.escape(str(row.get('underlier', '') or ''))}</td>
            <td class="truncate">{row.get('market_ids', '')}</td>
            <td><pre>{html.escape(str(row.get('constraint_expr', '') or ''))}</pre></td>
        </tr>"""

    return f"""
    <div class="search">
        <input type="text" id="constSearch" placeholder="Filter constraints..." onkeyup="filterTable('constSearch', 'constTable')">
        <span style="color:#888; margin-left:10px;">{len(df)} constraints</span>
    </div>
    <table id="constTable">
        <thead><tr><th>Type</th><th>Underlier</th><th>Market IDs</th><th>Constraint</th></tr></thead>
        <tbody>{rows}</tbody>
    </table>"""


def render_violations(df: Optional[pl.DataFrame]) -> str:
    if df is None or len(df) == 0:
        return '<div class="empty">No violations detected. This is good! (or run: cargo run --bin surveillance_rules -- detect-arb)</div>'

    rows = ""
    for row in df.head(500).iter_rows(named=True):
        severity = row.get('severity', 'low')
        sev_class = "badge-red" if severity == "high" else "badge-yellow" if severity == "medium" else "badge-blue"
        
        rows += f"""<tr>
            <td><span class="badge {sev_class}">{severity}</span></td>
            <td>{row.get('constraint_type', '')}</td>
            <td class="truncate">{row.get('market_ids', '')}</td>
            <td>{row.get('expected', '')}</td>
            <td>{row.get('actual', '')}</td>
            <td>{row.get('margin', '')}</td>
        </tr>"""

    return f"""
    <div style="background:#f8d7da;padding:15px;border-radius:5px;margin-bottom:15px;">
        <strong>⚠️ {len(df)} violations detected!</strong> These may indicate arbitrage opportunities.
    </div>
    <table id="violTable">
        <thead><tr><th>Severity</th><th>Type</th><th>Markets</th><th>Expected</th><th>Actual</th><th>Margin</th></tr></thead>
        <tbody>{rows}</tbody>
    </table>"""


def render_review(items: List[Dict]) -> str:
    if not items:
        return '<div class="empty">No items in review queue. All propositions have high confidence!</div>'

    rows = ""
    for item in items[:500]:
        rows += f"""<tr>
            <td class="truncate">{item.get('market_id', '')[:20]}...</td>
            <td class="truncate">{html.escape(item.get('title', '')[:60])}</td>
            <td>{item.get('confidence', 0):.2f}</td>
            <td class="truncate">{html.escape(item.get('reason', ''))}</td>
            <td>{item.get('status', 'pending')}</td>
        </tr>"""

    return f"""
    <div style="background:#fff3cd;padding:15px;border-radius:5px;margin-bottom:15px;">
        <strong>📋 {len(items)} items need review</strong> - These propositions have low confidence and may need manual verification.
    </div>
    <table id="reviewTable">
        <thead><tr><th>Market ID</th><th>Title</th><th>Confidence</th><th>Reason</th><th>Status</th></tr></thead>
        <tbody>{rows}</tbody>
    </table>"""


class RequestHandler(BaseHTTPRequestHandler):
    data: RulesData

    def do_GET(self):
        from urllib.parse import urlparse, parse_qs
        parsed = urlparse(self.path)
        params = parse_qs(parsed.query)
        tab = params.get("tab", ["rules"])[0]

        self.send_response(200)
        self.send_header("Content-type", "text/html")
        self.end_headers()
        self.wfile.write(generate_html(self.data, tab).encode())

    def log_message(self, format, *args):
        pass  # Suppress logs


def main():
    parser = argparse.ArgumentParser(description="Rules Pipeline Browser")
    parser.add_argument("--venue", default="polymarket", help="Venue name")
    parser.add_argument("--date", default=datetime.now(timezone.utc).strftime("%Y-%m-%d"), help="Date (YYYY-MM-DD)")
    parser.add_argument("--data-dir", default="data", help="Data directory")
    parser.add_argument("--port", type=int, default=8081, help="HTTP port")
    args = parser.parse_args()

    data = RulesData(args.venue, args.date, args.data_dir)
    RequestHandler.data = data

    server = HTTPServer(("0.0.0.0", args.port), RequestHandler)
    print(f"Rules Browser running at http://localhost:{args.port}")
    print(f"Venue: {args.venue}, Date: {args.date}")
    print("Press Ctrl+C to stop")

    try:
        server.serve_forever()
    except KeyboardInterrupt:
        print("\nStopping...")
        server.shutdown()


if __name__ == "__main__":
    main()
