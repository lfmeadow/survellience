use crate::venue::MarketInfo;
use std::collections::HashMap;

#[derive(Debug, Clone)]
pub struct MarketScore {
    pub market_id: String,
    pub score: f64,
}

pub fn score_markets(
    markets: &[MarketInfo],
    stats_cache: Option<&HashMap<String, MarketStats>>,
) -> Vec<MarketScore> {
    let mut scores = Vec::new();

    for market in markets {
        let mut score = 0.0;

        // Base score from recency (if close_ts is in future, market is active)
        if let Some(close_ts) = market.close_ts {
            let now = chrono::Utc::now().timestamp_millis();
            if close_ts > now {
                let days_until_close = (close_ts - now) as f64 / (86400.0 * 1000.0);
                score += 1.0 / (1.0 + days_until_close / 30.0); // Decay over 30 days
            }
        }

        // Boost from status
        if market.status == "active" {
            score += 0.5;
        }

        // Boost from stats cache if available
        if let Some(stats) = stats_cache {
            if let Some(market_stats) = stats.get(&market.market_id) {
                // Higher depth = higher score
                score += market_stats.avg_depth / 1000.0;
                // Tighter spread = higher score
                if market_stats.avg_spread > 0.0 {
                    score += 1.0 / (1.0 + market_stats.avg_spread * 100.0);
                }
                // More active = higher score
                score += (market_stats.update_count as f64) / 10000.0;
            }
        }

        scores.push(MarketScore {
            market_id: market.market_id.clone(),
            score,
        });
    }

    // Sort by score descending
    scores.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap_or(std::cmp::Ordering::Equal));
    scores
}

#[derive(Debug, Clone)]
pub struct MarketStats {
    pub market_id: String,
    pub avg_depth: f64,
    pub avg_spread: f64,
    pub update_count: usize,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_score_markets() {
        let markets = vec![
            MarketInfo {
                market_id: "market1".to_string(),
                title: "Market 1".to_string(),
                outcome_ids: vec!["yes".to_string(), "no".to_string()],
                close_ts: Some(chrono::Utc::now().timestamp_millis() + 86400_000),
                status: "active".to_string(),
                tags: vec![],
                token_ids: vec![],
            },
            MarketInfo {
                market_id: "market2".to_string(),
                title: "Market 2".to_string(),
                outcome_ids: vec!["yes".to_string(), "no".to_string()],
                close_ts: Some(chrono::Utc::now().timestamp_millis() - 86400_000),
                status: "closed".to_string(),
                tags: vec![],
                token_ids: vec![],
            },
        ];

        let scores = score_markets(&markets, None);
        assert_eq!(scores.len(), 2);
        assert!(scores[0].score > scores[1].score); // Active market should score higher
    }
}
