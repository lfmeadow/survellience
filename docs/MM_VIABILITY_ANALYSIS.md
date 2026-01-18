# Market Making Viability Analysis

## Overview

This document analyzes the viability of passive market making strategies on Polymarket, with a focus on short-term "Up or Down" crypto prediction markets.

## Executive Summary

**Passive market making on short-term (5-15 minute) crypto prediction markets is likely NOT viable** due to extreme adverse selection, time pressure, and structural information disadvantage.

## Short-Term "Up or Down" Markets

### Market Structure

Polymarket offers many short-term prediction markets for crypto price movements with varying time windows:

- **5-minute windows** (e.g., "Bitcoin Up or Down - January 17, 11:35AM-11:40AM ET")
- **15-minute windows** (e.g., "XRP Up or Down - January 17, 7:00PM-7:15PM ET")
- **4-hour windows** (e.g., "Ethereum Up or Down - January 16, 4:00PM-8:00PM ET")

Each time window is a completely independent market with its own `market_id` and resolves at the end of its time period.

### Data Analysis (2026-01-17)

| Metric | Value | Interpretation |
|--------|-------|----------------|
| Total "Up or Down" market rows | 3,624 | Significant portion of activity |
| Unique "Up or Down" markets | 157 | Many independent short-term markets |
| Two-sided quote rate | 59.3% | 40% of time, can't capture spread |
| Median spread | 1 cent (0.01) | Only 0.5 cents per side to capture |
| Mean spread | 2.4 cents (0.024) | Wider spreads exist but less common |
| Mean toxicity (30s horizon) | -1.03 | Adverse selection ≈ 100% of half-spread |
| Markets meeting MM criteria | 1 of 157 | Almost none have sufficient activity |

### Key Problems

#### 1. Extreme Adverse Selection

The fundamental problem with these markets is **information asymmetry**:

- The underlying asset (BTC, ETH, SOL, XRP) price is **publicly observable in real-time** on major exchanges (Binance, Coinbase, etc.)
- Traders can see price movements on spot/futures markets BEFORE placing bets on Polymarket
- Market makers are essentially betting against people who already know the direction
- Toxicity ≈ -1.0 means adverse selection consumes approximately 100% of spread capture

This creates a structural disadvantage for passive market makers - you're providing liquidity to informed traders who can observe the underlying price in real-time.

#### 2. Time Pressure

Short time windows (5-15 minutes) create severe operational challenges:

- **Limited fill opportunity**: Very little time to get filled on both bid and ask
- **No inventory management**: Can't wait for favorable conditions to exit
- **Binary resolution risk**: If holding inventory at expiration, outcome is 0 or 1
- **No mean reversion**: Unlike traditional MM, there's no time for prices to revert

#### 3. Two-Sided Quote Availability

Only ~60% of snapshots show two-sided markets:

- 40% of the time, only one side has liquidity
- Can't capture spread if there's no opposite side
- Indicates thin, sporadic participation

#### 4. Economics

For a typical 5-minute market:

```
Total trading window: 300 seconds
Typical spread: 1 cent (0.01)
Half-spread capture: 0.5 cents per side
Polymarket fee: ~2% of winnings (~1 cent on a $1 contract)

Gross capture per round-trip: ~1 cent
Adverse selection cost: ~1 cent (based on toxicity)
Net expected value: ~0 or negative
```

### Why Informed Trading Dominates

These markets are essentially **latency arbitrage** opportunities:

1. Trader observes BTC price spike on Binance
2. Trader immediately buys "Up" on Polymarket
3. Market maker's resting "Up" ask gets lifted
4. Market maker is now short "Up" right before resolution
5. Market resolves "Up" → MM loses

The information flow is:
```
Spot Exchange Price Move → Informed Trader → Polymarket → Market Maker (victim)
```

## When MM Might Be Viable on Polymarket

Better candidates for market making strategies:

### 1. Longer-Dated Markets (Days/Weeks)
- Information arrives gradually over time
- More time to manage inventory and capture spread
- Mean reversion opportunities exist
- Examples: Weekly crypto price targets, monthly events

### 2. Event-Based Markets with Genuine Uncertainty
- Elections where polling is uncertain
- Sports outcomes where odds are debatable
- No real-time observable underlying that leaks information
- Information advantages are harder to obtain

### 3. Markets Without Real-Time Observable Underlying
- Political events (who will be appointed, policy decisions)
- Scientific outcomes (will a study succeed)
- Regulatory decisions
- These have no "Binance equivalent" to front-run from

### 4. High-Liquidity Markets with Tight Spreads
- More participants = harder for any single trader to move price
- Tighter spreads indicate competitive MM already
- May still have opportunity for sophisticated strategies

## Recommendations

### For This Surveillance System

1. **Filter short-term crypto markets from MM analysis** - they are structurally unsuitable
2. **Focus data collection on longer-dated markets** - better MM candidates
3. **Add market duration/close_ts to analysis** - identify market timeframes
4. **Consider toxicity thresholds** - skip markets with toxicity < -0.5

### For Trading Strategy

1. **Avoid passive MM on sub-hour crypto markets** - negative expected value
2. **Consider informed directional trading instead** - if you have low-latency crypto price feeds
3. **Focus MM efforts on event markets** - elections, sports, policy
4. **Evaluate longer-dated crypto markets separately** - may have different dynamics

## Data Collection Notes

The analysis above is based on:
- Date: 2026-01-17
- Venue: Polymarket
- Data points: 6,924 total snapshots
- "Up or Down" markets: 3,624 snapshots across 157 unique markets

Toxicity is calculated as:
```
toxicity = E[adverse_selection] / (spread / 2)
```

Where adverse selection is measured as the expected mid-price movement against your position over a 30-second horizon.

## Conclusion

Short-term crypto "Up or Down" markets on Polymarket are **not suitable for passive market making** due to:

1. Extreme adverse selection from real-time observable underlying prices
2. Insufficient time to manage inventory or capture round-trip spreads  
3. Binary resolution risk
4. Structural information disadvantage vs. informed traders

Market making efforts should focus on longer-dated markets and event-based predictions where information asymmetry is less severe.
