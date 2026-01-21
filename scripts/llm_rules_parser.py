#!/usr/bin/env python3
"""
LLM-based Rules Parser - Convert natural language rules to symbolic logic.

Uses an LLM to:
1. Parse each market's rules into structured propositions
2. Identify logical relationships between markets
3. Generate constraints for arbitrage detection
"""

import argparse
import json
import os
import sys
from dataclasses import dataclass, asdict
from datetime import datetime, timezone
from pathlib import Path
from typing import Optional, List, Dict, Any
import hashlib

try:
    import polars as pl
except ImportError:
    print("ERROR: polars not installed. pip install polars")
    sys.exit(1)

# Try to import LLM libraries
ANTHROPIC_AVAILABLE = False
OPENAI_AVAILABLE = False

try:
    import anthropic
    ANTHROPIC_AVAILABLE = True
except ImportError:
    pass

try:
    import openai
    OPENAI_AVAILABLE = True
except ImportError:
    pass


@dataclass
class SymbolicProposition:
    """A proposition in symbolic logic form."""
    market_id: str
    title: str
    
    # Core proposition
    proposition_type: str  # price_target, earnings_beat, election, binary_event, other
    
    # For price targets
    underlier: Optional[str] = None  # BTC, ETH, NASDAQ, DJI, AAPL, etc.
    strike: Optional[float] = None
    comparator: Optional[str] = None  # gte, lte, gt, lt, eq
    
    # For earnings
    company_ticker: Optional[str] = None
    metric: Optional[str] = None  # EPS, revenue, etc.
    
    # Time window
    window_start: Optional[str] = None
    window_end: Optional[str] = None
    
    # Symbolic representation
    symbolic_form: str = ""  # e.g., "P(BTC >= 100000 by 2026-12-31)"
    
    # LLM extraction metadata
    confidence: float = 0.0
    reasoning: str = ""
    raw_rules: str = ""


@dataclass  
class LogicalConstraint:
    """A logical constraint between propositions."""
    constraint_type: str  # monotonic_ladder, complement, implication, mutual_exclusion
    market_ids: List[str]
    relation: str  # Human-readable relation
    symbolic_form: str  # Formal logic form
    confidence: float
    reasoning: str


SYSTEM_PROMPT = """You are an expert at converting prediction market rules into symbolic logic.

Given a market's title and rules text, extract:
1. The type of proposition (price_target, earnings_beat, election, sports, binary_event, other)
2. For price targets: the underlier asset, strike price, and comparator (above/below/at)
3. For earnings: company ticker and metric being compared
4. Time window if specified
5. A symbolic logic representation

Be precise about distinguishing:
- "BTC above $100k" = price_target with underlier=BTC, strike=100000, comparator=gte
- "Will AAPL beat earnings?" = earnings_beat with company_ticker=AAPL (NOT a price target)
- "DOW hits 50k" = price_target with underlier=DJI, strike=50000, comparator=gte
- "Trump wins election" = election (NOT a price target)

Return JSON only, no markdown."""

EXTRACTION_PROMPT = """Analyze this prediction market:

Title: {title}

Rules:
{rules}

Extract the proposition and return ONLY valid JSON:
{{
    "proposition_type": "price_target|earnings_beat|election|sports|binary_event|other",
    "underlier": "asset symbol or null",
    "strike": number or null,
    "comparator": "gte|lte|gt|lt|eq or null",
    "company_ticker": "ticker or null",
    "metric": "EPS|revenue|etc or null", 
    "window_start": "YYYY-MM-DD or null",
    "window_end": "YYYY-MM-DD or null",
    "symbolic_form": "P(condition) format",
    "confidence": 0.0-1.0,
    "reasoning": "brief explanation"
}}"""


BATCH_CONSTRAINT_PROMPT = """Given these parsed propositions, identify logical constraints between them.

Propositions:
{propositions}

Find:
1. **Monotonic ladders**: Same underlier, different strikes (P(X>=100) >= P(X>=110))
2. **Complements**: YES/NO pairs that should sum to 1
3. **Implications**: If A happens, B must happen (P(A) <= P(B))
4. **Mutual exclusions**: Only one can be true

Return JSON array of constraints:
[{{
    "constraint_type": "monotonic_ladder|complement|implication|mutual_exclusion",
    "market_ids": ["id1", "id2"],
    "relation": "human readable",
    "symbolic_form": "P(A) <= P(B)",
    "confidence": 0.0-1.0,
    "reasoning": "why this constraint exists"
}}]"""


class LLMParser:
    """Base class for LLM-based parsing."""
    
    def parse_single(self, title: str, rules: str) -> dict:
        raise NotImplementedError
    
    def find_constraints(self, propositions: List[dict]) -> List[dict]:
        raise NotImplementedError


class AnthropicParser(LLMParser):
    def __init__(self, model: str = "claude-sonnet-4-20250514"):
        self.client = anthropic.Anthropic()
        self.model = model
    
    def parse_single(self, title: str, rules: str) -> dict:
        prompt = EXTRACTION_PROMPT.format(title=title, rules=rules[:3000])
        
        try:
            response = self.client.messages.create(
                model=self.model,
                max_tokens=1024,
                system=SYSTEM_PROMPT,
                messages=[{"role": "user", "content": prompt}]
            )
            text = response.content[0].text.strip()
            # Extract JSON from response
            if text.startswith("```"):
                text = text.split("```")[1]
                if text.startswith("json"):
                    text = text[4:]
            return json.loads(text)
        except Exception as e:
            return {"error": str(e), "confidence": 0.0}
    
    def find_constraints(self, propositions: List[dict]) -> List[dict]:
        # Summarize propositions for the prompt
        summary = []
        for p in propositions[:100]:  # Limit for context window
            summary.append({
                "market_id": p.get("market_id", "")[:20],
                "type": p.get("proposition_type"),
                "underlier": p.get("underlier"),
                "strike": p.get("strike"),
                "symbolic_form": p.get("symbolic_form"),
            })
        
        prompt = BATCH_CONSTRAINT_PROMPT.format(propositions=json.dumps(summary, indent=2))
        
        try:
            response = self.client.messages.create(
                model=self.model,
                max_tokens=4096,
                system=SYSTEM_PROMPT,
                messages=[{"role": "user", "content": prompt}]
            )
            text = response.content[0].text.strip()
            if text.startswith("```"):
                text = text.split("```")[1]
                if text.startswith("json"):
                    text = text[4:]
            return json.loads(text)
        except Exception as e:
            print(f"Constraint detection error: {e}")
            return []


class OpenAIParser(LLMParser):
    def __init__(self, model: str = "gpt-4o"):
        self.client = openai.OpenAI()
        self.model = model
    
    def parse_single(self, title: str, rules: str) -> dict:
        prompt = EXTRACTION_PROMPT.format(title=title, rules=rules[:3000])
        
        try:
            response = self.client.chat.completions.create(
                model=self.model,
                max_tokens=1024,
                messages=[
                    {"role": "system", "content": SYSTEM_PROMPT},
                    {"role": "user", "content": prompt}
                ]
            )
            text = response.choices[0].message.content.strip()
            if text.startswith("```"):
                text = text.split("```")[1]
                if text.startswith("json"):
                    text = text[4:]
            return json.loads(text)
        except Exception as e:
            return {"error": str(e), "confidence": 0.0}
    
    def find_constraints(self, propositions: List[dict]) -> List[dict]:
        summary = []
        for p in propositions[:100]:
            summary.append({
                "market_id": p.get("market_id", "")[:20],
                "type": p.get("proposition_type"),
                "underlier": p.get("underlier"),
                "strike": p.get("strike"),
                "symbolic_form": p.get("symbolic_form"),
            })
        
        prompt = BATCH_CONSTRAINT_PROMPT.format(propositions=json.dumps(summary, indent=2))
        
        try:
            response = self.client.chat.completions.create(
                model=self.model,
                max_tokens=4096,
                messages=[
                    {"role": "system", "content": SYSTEM_PROMPT},
                    {"role": "user", "content": prompt}
                ]
            )
            text = response.choices[0].message.content.strip()
            if text.startswith("```"):
                text = text.split("```")[1]
                if text.startswith("json"):
                    text = text[4:]
            return json.loads(text)
        except Exception as e:
            print(f"Constraint detection error: {e}")
            return []


def load_rules(data_dir: Path, venue: str, date: str) -> List[dict]:
    """Load raw rules from JSONL."""
    path = data_dir / "rules" / f"venue={venue}" / f"date={date}" / "rules.jsonl"
    rules = []
    if path.exists():
        for line in path.read_text().splitlines():
            if line.strip():
                try:
                    rules.append(json.loads(line))
                except:
                    pass
    return rules


def cache_key(market_id: str, rules_text: str) -> str:
    """Generate cache key for parsed result."""
    content = f"{market_id}:{rules_text}"
    return hashlib.sha256(content.encode()).hexdigest()[:16]


def run_llm_parsing(
    data_dir: str,
    venue: str,
    date: str,
    provider: str = "anthropic",
    model: Optional[str] = None,
    limit: Optional[int] = None,
    use_cache: bool = True,
):
    """Run LLM-based parsing on rules."""
    
    data_path = Path(data_dir)
    
    # Select LLM provider
    if provider == "anthropic":
        if not ANTHROPIC_AVAILABLE:
            print("ERROR: anthropic library not installed. pip install anthropic")
            sys.exit(1)
        parser = AnthropicParser(model or "claude-sonnet-4-20250514")
    elif provider == "openai":
        if not OPENAI_AVAILABLE:
            print("ERROR: openai library not installed. pip install openai")
            sys.exit(1)
        parser = OpenAIParser(model or "gpt-4o")
    else:
        print(f"ERROR: Unknown provider {provider}")
        sys.exit(1)
    
    # Load rules
    rules = load_rules(data_path, venue, date)
    print(f"Loaded {len(rules)} rules")
    
    if limit:
        rules = rules[:limit]
        print(f"Limited to {limit} rules")
    
    # Setup cache
    cache_dir = data_path / "llm_cache" / f"venue={venue}" / f"date={date}"
    cache_dir.mkdir(parents=True, exist_ok=True)
    
    # Parse each rule
    propositions = []
    for i, rule in enumerate(rules):
        market_id = rule.get("market_id", "")
        title = rule.get("title", "")
        rules_text = rule.get("raw_rules_text", "")
        
        # Check cache
        key = cache_key(market_id, rules_text)
        cache_file = cache_dir / f"{key}.json"
        
        if use_cache and cache_file.exists():
            try:
                result = json.loads(cache_file.read_text())
                result["market_id"] = market_id
                result["title"] = title
                result["raw_rules"] = rules_text
                propositions.append(result)
                continue
            except:
                pass
        
        # Parse with LLM
        print(f"\r  Parsing {i+1}/{len(rules)}: {title[:50]}...", end="", flush=True)
        result = parser.parse_single(title, rules_text)
        result["market_id"] = market_id
        result["title"] = title
        result["raw_rules"] = rules_text
        
        # Cache result
        cache_file.write_text(json.dumps(result, indent=2))
        
        propositions.append(result)
    
    print(f"\n\nParsed {len(propositions)} propositions")
    
    # Summarize by type
    by_type = {}
    for p in propositions:
        t = p.get("proposition_type", "unknown")
        by_type[t] = by_type.get(t, 0) + 1
    
    print("\nProposition types:")
    for t, count in sorted(by_type.items(), key=lambda x: -x[1]):
        print(f"  {t}: {count}")
    
    # Find high-confidence price targets
    price_targets = [p for p in propositions 
                     if p.get("proposition_type") == "price_target" 
                     and p.get("confidence", 0) >= 0.7]
    
    print(f"\nHigh-confidence price targets: {len(price_targets)}")
    
    # Group by underlier
    by_underlier = {}
    for p in price_targets:
        u = p.get("underlier", "unknown")
        if u not in by_underlier:
            by_underlier[u] = []
        by_underlier[u].append(p)
    
    print("\nPrice ladders detected:")
    for underlier, markets in sorted(by_underlier.items(), key=lambda x: -len(x[1])):
        if len(markets) >= 2:
            strikes = sorted([m.get("strike", 0) for m in markets if m.get("strike")])
            print(f"  {underlier}: {len(markets)} markets, strikes: {strikes[:5]}{'...' if len(strikes) > 5 else ''}")
    
    # Find constraints using LLM
    print("\nFinding logical constraints...")
    constraints = parser.find_constraints(price_targets)
    print(f"Found {len(constraints)} constraints")
    
    # Save outputs
    output_dir = data_path / "llm_logic" / f"venue={venue}" / f"date={date}"
    output_dir.mkdir(parents=True, exist_ok=True)
    
    # Save propositions
    props_file = output_dir / "propositions.jsonl"
    with open(props_file, "w") as f:
        for p in propositions:
            f.write(json.dumps(p) + "\n")
    print(f"\nWrote propositions to {props_file}")
    
    # Save constraints
    constraints_file = output_dir / "constraints.jsonl"
    with open(constraints_file, "w") as f:
        for c in constraints:
            f.write(json.dumps(c) + "\n")
    print(f"Wrote constraints to {constraints_file}")
    
    # Summary report
    report = {
        "venue": venue,
        "date": date,
        "total_rules": len(rules),
        "parsed": len(propositions),
        "by_type": by_type,
        "price_targets": len(price_targets),
        "ladders": {u: len(m) for u, m in by_underlier.items() if len(m) >= 2},
        "constraints": len(constraints),
    }
    
    report_file = output_dir / "summary.json"
    report_file.write_text(json.dumps(report, indent=2))
    print(f"Wrote summary to {report_file}")
    
    return propositions, constraints


def main():
    parser = argparse.ArgumentParser(description="LLM-based Rules Parser")
    parser.add_argument("--venue", default="polymarket", help="Venue name")
    parser.add_argument("--date", default=datetime.now(timezone.utc).strftime("%Y-%m-%d"), help="Date")
    parser.add_argument("--data-dir", default="data", help="Data directory")
    parser.add_argument("--provider", choices=["anthropic", "openai"], default="anthropic", help="LLM provider")
    parser.add_argument("--model", help="Model name (provider-specific)")
    parser.add_argument("--limit", type=int, help="Limit number of rules to parse")
    parser.add_argument("--no-cache", action="store_true", help="Disable caching")
    
    args = parser.parse_args()
    
    run_llm_parsing(
        data_dir=args.data_dir,
        venue=args.venue,
        date=args.date,
        provider=args.provider,
        model=args.model,
        limit=args.limit,
        use_cache=not args.no_cache,
    )


if __name__ == "__main__":
    main()
