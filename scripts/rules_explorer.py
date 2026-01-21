#!/usr/bin/env python3
"""
Interactive Rules Explorer - Visual browser for rules pipeline data.
Shows propositions, constraints, and violations with clickable navigation.
"""

import argparse
import json
import html
from datetime import datetime, timezone
from http.server import BaseHTTPRequestHandler, HTTPServer
from pathlib import Path
from urllib.parse import urlparse, parse_qs
from typing import Dict, List, Optional, Any

try:
    import polars as pl
except ImportError:
    print("ERROR: polars not installed. Install with: pip install polars")
    raise


class RulesExplorer:
    def __init__(self, venue: str, date: str, data_dir: str):
        self.venue = venue
        self.date = date
        self.data_dir = Path(data_dir)
        self._cache = {}
    
    def load_rules(self) -> Dict[str, dict]:
        if "rules" in self._cache:
            return self._cache["rules"]
        path = self.data_dir / "rules" / f"venue={self.venue}" / f"date={self.date}" / "rules.jsonl"
        rules = {}
        if path.exists():
            for line in path.read_text().splitlines():
                if line.strip():
                    try:
                        r = json.loads(line)
                        rules[r.get("market_id", "")] = r
                    except:
                        pass
        self._cache["rules"] = rules
        return rules
    
    def load_propositions(self) -> pl.DataFrame:
        if "propositions" in self._cache:
            return self._cache["propositions"]
        path = self.data_dir / "logic" / f"venue={self.venue}" / f"date={self.date}" / "propositions.parquet"
        if path.exists():
            df = pl.read_parquet(path)
            self._cache["propositions"] = df
            return df
        return pl.DataFrame()
    
    def load_constraints(self) -> pl.DataFrame:
        if "constraints" in self._cache:
            return self._cache["constraints"]
        path = self.data_dir / "logic" / f"venue={self.venue}" / f"date={self.date}" / "constraints.parquet"
        if path.exists():
            df = pl.read_parquet(path)
            self._cache["constraints"] = df
            return df
        return pl.DataFrame()
    
    def load_violations(self) -> pl.DataFrame:
        if "violations" in self._cache:
            return self._cache["violations"]
        path = self.data_dir / "logic" / f"venue={self.venue}" / f"date={self.date}" / "violations.parquet"
        if path.exists():
            df = pl.read_parquet(path)
            self._cache["violations"] = df
            return df
        return pl.DataFrame()
    
    def get_underliers(self) -> List[str]:
        props = self.load_propositions()
        if props.is_empty():
            return []
        return sorted([u for u in props["underlier"].unique().to_list() if u])
    
    def get_ladders(self) -> Dict[str, List[dict]]:
        """Group markets into ladders by underlier."""
        props = self.load_propositions()
        rules = self.load_rules()
        
        ladders = {}
        if props.is_empty():
            return ladders
        
        # Group by underlier
        for underlier in self.get_underliers():
            markets = props.filter(
                (pl.col("underlier") == underlier) &
                (pl.col("strike").is_not_null())
            ).sort("strike")
            
            if len(markets) > 1:
                ladder = []
                for row in markets.iter_rows(named=True):
                    rule = rules.get(row["market_id"], {})
                    ladder.append({
                        "market_id": row["market_id"],
                        "title": row["title"],
                        "strike": row["strike"],
                        "comparator": row["comparator"],
                        "confidence": row["confidence"],
                        "url": rule.get("url", ""),
                    })
                ladders[underlier] = ladder
        
        return ladders


def css_styles() -> str:
    return """
    <style>
        * { box-sizing: border-box; }
        body { 
            font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, sans-serif; 
            margin: 0; padding: 0; background: #1a1a2e; color: #eee;
        }
        .header { 
            background: linear-gradient(135deg, #16213e, #1a1a2e); 
            padding: 20px 30px; border-bottom: 1px solid #333;
        }
        .header h1 { margin: 0 0 5px 0; color: #00d4ff; }
        .header .meta { color: #888; font-size: 13px; }
        .nav { 
            background: #16213e; padding: 15px 30px; 
            display: flex; gap: 15px; flex-wrap: wrap; border-bottom: 1px solid #333;
        }
        .nav a { 
            color: #00d4ff; text-decoration: none; padding: 8px 16px; 
            border-radius: 5px; background: #0f3460; transition: all 0.2s;
        }
        .nav a:hover { background: #1a4a7a; }
        .nav a.active { background: #00d4ff; color: #000; }
        .container { max-width: 1400px; margin: 0 auto; padding: 20px 30px; }
        .card { 
            background: #16213e; border-radius: 10px; padding: 20px; 
            margin-bottom: 20px; border: 1px solid #333;
        }
        .card h2 { margin: 0 0 15px 0; color: #00d4ff; font-size: 18px; }
        .card h3 { margin: 15px 0 10px 0; color: #fff; font-size: 16px; }
        table { width: 100%; border-collapse: collapse; font-size: 13px; }
        th, td { padding: 10px 12px; text-align: left; border-bottom: 1px solid #333; }
        th { background: #0f3460; color: #00d4ff; font-weight: 500; }
        tr:hover { background: #1f2a4a; }
        .ladder { 
            display: flex; flex-direction: column; gap: 2px; 
            background: #0f3460; border-radius: 8px; padding: 15px; margin: 10px 0;
        }
        .ladder-item { 
            display: flex; align-items: center; gap: 15px; 
            background: #16213e; padding: 12px 15px; border-radius: 5px;
            border-left: 4px solid #00d4ff; transition: all 0.2s;
        }
        .ladder-item:hover { background: #1f2a4a; transform: translateX(5px); }
        .ladder-item .strike { 
            font-size: 18px; font-weight: bold; color: #00d4ff; 
            min-width: 100px;
        }
        .ladder-item .title { flex: 1; color: #ccc; }
        .ladder-item .confidence { 
            padding: 3px 10px; border-radius: 15px; font-size: 12px;
        }
        .conf-high { background: #0d7a3e; color: #fff; }
        .conf-med { background: #856404; color: #fff; }
        .conf-low { background: #721c24; color: #fff; }
        .link { color: #00d4ff; text-decoration: none; }
        .link:hover { text-decoration: underline; }
        .badge { 
            display: inline-block; padding: 3px 10px; border-radius: 15px; 
            font-size: 11px; font-weight: 500;
        }
        .badge-blue { background: #0f3460; color: #00d4ff; }
        .badge-green { background: #0d7a3e; color: #fff; }
        .badge-red { background: #721c24; color: #fff; }
        .badge-yellow { background: #856404; color: #fff; }
        .stats { display: flex; gap: 20px; flex-wrap: wrap; margin-bottom: 20px; }
        .stat { 
            background: #0f3460; padding: 15px 25px; border-radius: 8px; 
            text-align: center; min-width: 120px;
        }
        .stat .value { font-size: 28px; font-weight: bold; color: #00d4ff; }
        .stat .label { font-size: 12px; color: #888; margin-top: 5px; }
        .rules-text { 
            background: #0a0a1a; padding: 15px; border-radius: 5px; 
            font-size: 13px; line-height: 1.6; white-space: pre-wrap;
            max-height: 300px; overflow-y: auto; color: #aaa;
        }
        .constraint-viz {
            display: flex; align-items: center; gap: 10px; 
            padding: 15px; background: #0f3460; border-radius: 8px; margin: 10px 0;
        }
        .constraint-viz .market {
            background: #16213e; padding: 10px 15px; border-radius: 5px;
            border: 1px solid #333; flex: 1;
        }
        .constraint-viz .relation {
            font-size: 20px; color: #00d4ff; padding: 0 10px;
        }
        .search { margin-bottom: 20px; }
        .search input {
            width: 100%; padding: 12px 15px; border-radius: 8px;
            border: 1px solid #333; background: #0f3460; color: #fff;
            font-size: 14px;
        }
        .search input:focus { outline: none; border-color: #00d4ff; }
        .empty { text-align: center; padding: 40px; color: #666; }
        .tabs { display: flex; gap: 5px; margin-bottom: 20px; }
        .tab { 
            padding: 10px 20px; background: #0f3460; color: #888;
            border-radius: 5px 5px 0 0; cursor: pointer; border: none;
        }
        .tab.active { background: #16213e; color: #00d4ff; }
    </style>
    """


def render_home(explorer: RulesExplorer) -> str:
    props = explorer.load_propositions()
    constraints = explorer.load_constraints()
    violations = explorer.load_violations()
    rules = explorer.load_rules()
    ladders = explorer.get_ladders()
    
    props_count = len(props) if not props.is_empty() else 0
    high_conf = len(props.filter(pl.col("confidence") >= 0.6)) if not props.is_empty() else 0
    
    stats_html = f"""
    <div class="stats">
        <div class="stat"><div class="value">{len(rules)}</div><div class="label">Rules Fetched</div></div>
        <div class="stat"><div class="value">{props_count}</div><div class="label">Propositions</div></div>
        <div class="stat"><div class="value">{high_conf}</div><div class="label">High Confidence</div></div>
        <div class="stat"><div class="value">{len(constraints) if not constraints.is_empty() else 0}</div><div class="label">Constraints</div></div>
        <div class="stat"><div class="value">{len(violations) if not violations.is_empty() else 0}</div><div class="label">Violations</div></div>
        <div class="stat"><div class="value">{len(ladders)}</div><div class="label">Price Ladders</div></div>
    </div>
    """
    
    # Underliers summary
    underliers_html = ""
    for underlier in explorer.get_underliers()[:10]:
        count = len(props.filter(pl.col("underlier") == underlier))
        ladder_count = len(ladders.get(underlier, []))
        underliers_html += f"""
        <tr>
            <td><a href="?page=underlier&u={underlier}" class="link">{underlier}</a></td>
            <td>{count}</td>
            <td>{ladder_count} markets</td>
        </tr>
        """
    
    # Recent violations
    violations_html = ""
    if not violations.is_empty():
        for row in violations.head(5).iter_rows(named=True):
            violations_html += f"""
            <tr>
                <td><span class="badge badge-red">Violation</span></td>
                <td><a href="?page=market&id={row.get('a_market_id','')}" class="link">{row.get('a_market_id','')[:20]}...</a></td>
                <td>{row.get('constraint_type', '')}</td>
                <td>{row.get('violation_magnitude', 0):.4f}</td>
            </tr>
            """
    else:
        violations_html = '<tr><td colspan="4" class="empty">No violations detected</td></tr>'
    
    return f"""
    <div class="container">
        {stats_html}
        
        <div class="card">
            <h2>📊 Price Ladders by Underlier</h2>
            <p style="color:#888;margin-bottom:15px;">Markets grouped by asset with extracted strike prices - useful for arbitrage detection</p>
            <table>
                <thead><tr><th>Underlier</th><th>Propositions</th><th>Ladder Size</th></tr></thead>
                <tbody>{underliers_html if underliers_html else '<tr><td colspan="3" class="empty">No underliers extracted</td></tr>'}</tbody>
            </table>
        </div>
        
        <div class="card">
            <h2>⚠️ Recent Violations</h2>
            <table>
                <thead><tr><th>Type</th><th>Market</th><th>Constraint</th><th>Magnitude</th></tr></thead>
                <tbody>{violations_html}</tbody>
            </table>
        </div>
    </div>
    """


def render_underlier(explorer: RulesExplorer, underlier: str) -> str:
    props = explorer.load_propositions()
    rules = explorer.load_rules()
    constraints = explorer.load_constraints()
    
    markets = props.filter(pl.col("underlier") == underlier).sort("strike")
    
    # Build ladder visualization
    ladder_html = ""
    for row in markets.iter_rows(named=True):
        rule = rules.get(row["market_id"], {})
        conf = row.get("confidence", 0)
        conf_class = "conf-high" if conf >= 0.6 else "conf-med" if conf >= 0.4 else "conf-low"
        strike = row.get("strike")
        strike_str = f"${strike:,.0f}" if strike else "N/A"
        comp = row.get("comparator", "")
        comp_symbol = "≥" if comp and "gte" in comp.lower() else "≤" if comp and "lte" in comp.lower() else ""
        
        ladder_html += f"""
        <div class="ladder-item">
            <div class="strike">{comp_symbol} {strike_str}</div>
            <div class="title">
                <a href="?page=market&id={row['market_id']}" class="link">{html.escape(row.get('title', '')[:60])}</a>
            </div>
            <div class="confidence {conf_class}">{conf:.0%}</div>
            <a href="{rule.get('url', '#')}" target="_blank" class="link">↗</a>
        </div>
        """
    
    # Find related constraints
    constraint_html = ""
    if not constraints.is_empty():
        related = constraints.filter(
            pl.col("notes").str.contains(underlier)
        )
        for row in related.iter_rows(named=True):
            constraint_html += f"""
            <div class="constraint-viz">
                <div class="market">
                    <a href="?page=market&id={row['a_market_id']}" class="link">{row['a_market_id'][:20]}...</a>
                </div>
                <div class="relation">≤</div>
                <div class="market">
                    <a href="?page=market&id={row['b_market_id']}" class="link">{row['b_market_id'][:20]}...</a>
                </div>
            </div>
            """
    
    return f"""
    <div class="container">
        <div class="card">
            <h2>📈 {underlier} Price Ladder</h2>
            <p style="color:#888;margin-bottom:15px;">
                {len(markets)} markets • Sorted by strike price • 
                Higher strikes should have lower probability
            </p>
            <div class="ladder">
                {ladder_html if ladder_html else '<div class="empty">No markets with strike prices</div>'}
            </div>
        </div>
        
        <div class="card">
            <h2>🔗 Monotonic Constraints</h2>
            <p style="color:#888;margin-bottom:15px;">P(lower strike) ≥ P(higher strike)</p>
            {constraint_html if constraint_html else '<div class="empty">No constraints generated for this underlier</div>'}
        </div>
    </div>
    """


def render_market(explorer: RulesExplorer, market_id: str) -> str:
    props = explorer.load_propositions()
    rules = explorer.load_rules()
    constraints = explorer.load_constraints()
    
    rule = rules.get(market_id, {})
    prop_rows = props.filter(pl.col("market_id") == market_id)
    prop = prop_rows.row(0, named=True) if len(prop_rows) > 0 else {}
    
    # Basic info
    title = rule.get("title") or prop.get("title", "Unknown")
    url = rule.get("url", "")
    rules_text = rule.get("raw_rules_text", "No rules text available")
    
    # Proposition details
    conf = prop.get("confidence", 0)
    conf_class = "badge-green" if conf >= 0.6 else "badge-yellow" if conf >= 0.4 else "badge-red"
    
    prop_html = f"""
    <table>
        <tr><th>Underlier</th><td>{prop.get('underlier') or 'Not extracted'}</td></tr>
        <tr><th>Strike</th><td>{f"${prop.get('strike'):,.0f}" if prop.get('strike') else 'Not extracted'}</td></tr>
        <tr><th>Comparator</th><td>{prop.get('comparator') or 'Not extracted'}</td></tr>
        <tr><th>Proposition Type</th><td>{prop.get('proposition_type') or 'Unknown'}</td></tr>
        <tr><th>Window End</th><td>{datetime.fromtimestamp(prop.get('window_end_ts', 0)/1000).strftime('%Y-%m-%d') if prop.get('window_end_ts') else 'Not extracted'}</td></tr>
        <tr><th>Confidence</th><td><span class="badge {conf_class}">{conf:.0%}</span></td></tr>
        <tr><th>Parse Notes</th><td>{prop.get('parse_notes', '')}</td></tr>
    </table>
    """
    
    # Related markets (same underlier)
    related_html = ""
    if prop.get("underlier"):
        related = props.filter(
            (pl.col("underlier") == prop["underlier"]) &
            (pl.col("market_id") != market_id)
        ).head(10)
        for row in related.iter_rows(named=True):
            related_html += f"""
            <tr>
                <td><a href="?page=market&id={row['market_id']}" class="link">{html.escape(row.get('title', '')[:50])}</a></td>
                <td>{f"${row.get('strike'):,.0f}" if row.get('strike') else '-'}</td>
                <td>{row.get('confidence', 0):.0%}</td>
            </tr>
            """
    
    # Constraints involving this market
    constraint_html = ""
    if not constraints.is_empty():
        involved = constraints.filter(
            (pl.col("a_market_id") == market_id) |
            (pl.col("b_market_id") == market_id)
        )
        for row in involved.iter_rows(named=True):
            other_id = row["b_market_id"] if row["a_market_id"] == market_id else row["a_market_id"]
            constraint_html += f"""
            <tr>
                <td>{row.get('constraint_type', '')}</td>
                <td><a href="?page=market&id={other_id}" class="link">{other_id[:20]}...</a></td>
                <td>{row.get('relation', '')}</td>
            </tr>
            """
    
    return f"""
    <div class="container">
        <div class="card">
            <h2>{html.escape(title)}</h2>
            <p>
                <span class="badge badge-blue">{market_id[:30]}...</span>
                {f'<a href="{url}" target="_blank" class="link" style="margin-left:10px;">View on Polymarket ↗</a>' if url else ''}
            </p>
        </div>
        
        <div class="card">
            <h2>📋 Extracted Proposition</h2>
            {prop_html}
        </div>
        
        <div class="card">
            <h2>📜 Raw Rules Text</h2>
            <div class="rules-text">{html.escape(rules_text)}</div>
        </div>
        
        <div class="card">
            <h2>🔗 Constraints</h2>
            <table>
                <thead><tr><th>Type</th><th>Other Market</th><th>Relation</th></tr></thead>
                <tbody>{constraint_html if constraint_html else '<tr><td colspan="3" class="empty">No constraints</td></tr>'}</tbody>
            </table>
        </div>
        
        <div class="card">
            <h2>📊 Related Markets (Same Underlier)</h2>
            <table>
                <thead><tr><th>Title</th><th>Strike</th><th>Confidence</th></tr></thead>
                <tbody>{related_html if related_html else '<tr><td colspan="3" class="empty">No related markets</td></tr>'}</tbody>
            </table>
        </div>
    </div>
    """


def render_search(explorer: RulesExplorer, query: str) -> str:
    props = explorer.load_propositions()
    rules = explorer.load_rules()
    
    query_lower = query.lower()
    results = []
    
    # Search in rules
    for market_id, rule in rules.items():
        title = rule.get("title", "")
        rules_text = rule.get("raw_rules_text", "")
        if query_lower in title.lower() or query_lower in rules_text.lower():
            results.append({
                "market_id": market_id,
                "title": title,
                "match": "title" if query_lower in title.lower() else "rules",
                "url": rule.get("url", ""),
            })
            if len(results) >= 50:
                break
    
    results_html = ""
    for r in results:
        results_html += f"""
        <tr>
            <td><a href="?page=market&id={r['market_id']}" class="link">{html.escape(r['title'][:60])}</a></td>
            <td><span class="badge badge-blue">{r['match']}</span></td>
            <td><a href="{r['url']}" target="_blank" class="link">↗</a></td>
        </tr>
        """
    
    return f"""
    <div class="container">
        <div class="card">
            <h2>🔍 Search Results for "{html.escape(query)}"</h2>
            <p style="color:#888;">{len(results)} results found</p>
            <table>
                <thead><tr><th>Title</th><th>Match</th><th>Link</th></tr></thead>
                <tbody>{results_html if results_html else '<tr><td colspan="3" class="empty">No results</td></tr>'}</tbody>
            </table>
        </div>
    </div>
    """


def generate_html(explorer: RulesExplorer, page: str, params: dict) -> str:
    nav_items = [
        ("home", "🏠 Overview", "?page=home"),
        ("ladders", "📊 Ladders", "?page=home"),
        ("constraints", "🔗 Constraints", "?page=constraints"),
        ("violations", "⚠️ Violations", "?page=violations"),
    ]
    
    nav_html = ""
    for key, label, href in nav_items:
        active = "active" if page == key or (page == "home" and key == "home") else ""
        nav_html += f'<a href="{href}" class="{active}">{label}</a>'
    
    # Add search form
    nav_html += """
    <form action="" method="get" style="margin-left:auto;display:flex;gap:5px;">
        <input type="hidden" name="page" value="search">
        <input type="text" name="q" placeholder="Search markets..." 
               style="padding:8px 12px;border-radius:5px;border:1px solid #333;background:#0f3460;color:#fff;">
        <button type="submit" style="padding:8px 15px;border-radius:5px;border:none;background:#00d4ff;color:#000;cursor:pointer;">Search</button>
    </form>
    """
    
    # Render page content
    if page == "market":
        content = render_market(explorer, params.get("id", [""])[0])
    elif page == "underlier":
        content = render_underlier(explorer, params.get("u", [""])[0])
    elif page == "search":
        content = render_search(explorer, params.get("q", [""])[0])
    else:
        content = render_home(explorer)
    
    return f"""<!DOCTYPE html>
<html>
<head>
    <title>Rules Explorer - {explorer.venue} - {explorer.date}</title>
    <meta charset="utf-8">
    {css_styles()}
</head>
<body>
    <div class="header">
        <h1>🔍 Rules Explorer</h1>
        <div class="meta">Venue: {explorer.venue} | Date: {explorer.date} | Interactive browser for arbitrage detection</div>
    </div>
    <div class="nav">{nav_html}</div>
    {content}
</body>
</html>"""


class RequestHandler(BaseHTTPRequestHandler):
    explorer: RulesExplorer

    def do_GET(self):
        parsed = urlparse(self.path)
        params = parse_qs(parsed.query)
        page = params.get("page", ["home"])[0]
        
        self.send_response(200)
        self.send_header("Content-type", "text/html")
        self.end_headers()
        self.wfile.write(generate_html(self.explorer, page, params).encode())

    def log_message(self, format, *args):
        pass


def main():
    parser = argparse.ArgumentParser(description="Interactive Rules Explorer")
    parser.add_argument("--venue", default="polymarket", help="Venue name")
    parser.add_argument("--date", default=datetime.now(timezone.utc).strftime("%Y-%m-%d"), help="Date")
    parser.add_argument("--data-dir", default="data", help="Data directory")
    parser.add_argument("--port", type=int, default=8082, help="HTTP port")
    args = parser.parse_args()

    explorer = RulesExplorer(args.venue, args.date, args.data_dir)
    RequestHandler.explorer = explorer

    server = HTTPServer(("0.0.0.0", args.port), RequestHandler)
    print(f"🔍 Rules Explorer running at http://localhost:{args.port}")
    print(f"   Venue: {args.venue}, Date: {args.date}")
    print("   Press Ctrl+C to stop")

    try:
        server.serve_forever()
    except KeyboardInterrupt:
        print("\nStopping...")
        server.shutdown()


if __name__ == "__main__":
    main()
