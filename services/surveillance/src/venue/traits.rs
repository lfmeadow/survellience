use anyhow::Result;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MarketInfo {
    pub market_id: String,  // Condition ID (for Polymarket)
    pub title: String,
    pub outcome_ids: Vec<String>,
    pub close_ts: Option<i64>,
    pub status: String,
    pub tags: Vec<String>,
    #[serde(default)]
    pub token_ids: Vec<String>,  // Token IDs (clobTokenIds) for WebSocket subscriptions
}

#[derive(Debug, Clone)]
pub struct OrderBookLevel {
    pub price: f64,
    pub size: f64,
}

#[derive(Debug, Clone)]
pub struct OrderBookUpdate {
    pub market_id: String,
    pub outcome_id: String,
    pub bids: Vec<OrderBookLevel>,
    pub asks: Vec<OrderBookLevel>,
    pub timestamp_ms: Option<i64>,
    pub sequence: i64,
}

#[async_trait]
pub trait Venue: Send + Sync {
    fn name(&self) -> &str;

    async fn discover_markets(&self) -> Result<Vec<MarketInfo>>;

    async fn connect_websocket(&self) -> Result<()>;

    async fn subscribe(&self, market_ids: &[String], outcome_ids: &[String]) -> Result<()>;

    async fn unsubscribe(&self, market_ids: &[String], outcome_ids: &[String]) -> Result<()>;

    async fn receive_update(&mut self) -> Result<Option<OrderBookUpdate>>;

    fn is_connected(&self) -> bool;
}
