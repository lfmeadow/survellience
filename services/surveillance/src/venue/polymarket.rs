use super::traits::{MarketInfo, OrderBookLevel, OrderBookUpdate, Venue};
use anyhow::{Context, Result};
use async_trait::async_trait;
use chrono::{DateTime, Timelike, Utc};
use polars::prelude::*;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, VecDeque};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::path::Path;
use tokio::sync::Mutex;
use tokio::time::{Duration, Instant};
use tokio_tungstenite::{tungstenite::{Message, client::IntoClientRequest}, MaybeTlsStream, WebSocketStream};
use futures::{SinkExt, StreamExt};

#[derive(Debug, Serialize, Deserialize)]
struct PolymarketMarket {
    #[serde(rename = "conditionId")]
    condition_id: String,
    #[serde(rename = "questionId")]
    question_id: Option<String>,
    question: String,
    slug: String,
    #[serde(rename = "endDate")]
    end_date: Option<String>,
    #[serde(rename = "outcomePrices")]
    outcome_prices: Option<serde_json::Value>, // API returns JSON string or array
    active: Option<bool>,
    closed: Option<bool>,
    #[serde(rename = "marketMakerAddress")]
    market_maker_address: Option<String>,
    #[serde(rename = "imageUrl")]
    image_url: Option<String>,
    #[serde(rename = "iconUrl")]
    icon_url: Option<String>,
    #[serde(rename = "groupItemTitle")]
    group_item_title: Option<String>,
    #[serde(rename = "groupItemIconUrl")]
    group_item_icon_url: Option<String>,
    #[serde(rename = "groupItemSlug")]
    group_item_slug: Option<String>,
    #[serde(rename = "liquidityNum")]
    liquidity: Option<f64>,
    #[serde(rename = "volumeNum")]
    volume: Option<f64>,
    #[serde(rename = "newQuestion")]
    new_question: Option<bool>,
    #[serde(rename = "clobTokenIds")]
    clob_token_ids: Option<serde_json::Value>, // JSON string array like "[\"token1\", \"token2\"]"
}

#[derive(Debug, Serialize, Deserialize)]
struct PolymarketEvent {
    id: String,
    title: String,
    slug: String,
    active: Option<bool>,
    closed: Option<bool>,
    #[serde(rename = "endDate")]
    end_date: Option<String>,
    markets: Option<Vec<PolymarketMarket>>,
}

#[derive(Debug, Serialize, Deserialize)]
#[allow(dead_code)]
struct PolymarketMarketResponse {
    data: Vec<PolymarketMarket>,
}

#[derive(Debug, Serialize, Deserialize)]
#[allow(dead_code)]
struct PolymarketOutcome {
    id: String,
    price: f64,
}

// Legacy format (not used by CLOB WebSocket, kept for compatibility)
#[derive(Debug, Serialize, Deserialize)]
#[allow(dead_code)]
struct PolymarketOrderBookMessage {
    #[serde(rename = "type")]
    message_type: String,
    #[serde(rename = "channel")]
    channel: Option<String>,
    #[serde(rename = "data")]
    data: Option<serde_json::Value>,
    #[serde(rename = "event")]
    event: Option<String>,
    #[serde(rename = "market")]
    market: Option<String>,
    #[serde(rename = "outcome")]
    outcome: Option<String>,
    #[serde(rename = "bids")]
    bids: Option<Vec<[f64; 2]>>, // [price, size]
    #[serde(rename = "asks")]
    asks: Option<Vec<[f64; 2]>>, // [price, size]
    #[serde(rename = "timestamp")]
    timestamp: Option<i64>,
    #[serde(rename = "sequence")]
    sequence: Option<i64>,
}

// Actual CLOB WebSocket message formats
#[derive(Debug, Serialize, Deserialize)]
struct PolymarketClobBidAsk {
    price: String,  // String in actual messages
    size: String,  // String in actual messages
}

#[derive(Debug, Serialize, Deserialize)]
struct PolymarketClobOrderBookSnapshot {
    market: String,
    #[serde(rename = "asset_id")]
    asset_id: String,  // Token ID
    timestamp: Option<String>,  // String timestamp
    hash: Option<String>,
    bids: Option<Vec<PolymarketClobBidAsk>>,
    asks: Option<Vec<PolymarketClobBidAsk>>,
}

#[derive(Debug, Serialize, Deserialize)]
struct PolymarketClobPriceChange {
    #[serde(rename = "asset_id")]
    asset_id: String,  // Token ID
    price: String,
    size: String,
    side: String,  // "BUY" or "SELL"
    hash: Option<String>,
    #[serde(rename = "best_bid")]
    best_bid: Option<String>,
    #[serde(rename = "best_ask")]
    best_ask: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
struct PolymarketClobPriceChanges {
    market: String,
    #[serde(rename = "price_changes")]
    price_changes: Vec<PolymarketClobPriceChange>,
}

#[derive(Debug, Serialize, Deserialize)]
struct PolymarketClobTradeEvent {
    market: String,
    #[serde(rename = "asset_id")]
    asset_id: String,
    #[serde(rename = "event_type")]
    event_type: String,
    price: Option<String>,
    size: Option<String>,
    side: Option<String>,
    timestamp: Option<String>,
    #[serde(rename = "transaction_hash")]
    transaction_hash: Option<String>,
}

#[derive(Debug, Serialize)]
struct PolymarketTradeRecord {
    venue: String,
    market_id: Option<String>,
    outcome_id: Option<String>,
    asset_id: String,
    event_type: String,
    price: Option<String>,
    size: Option<String>,
    side: Option<String>,
    timestamp: Option<String>,
    timestamp_ms: Option<i64>,
    transaction_hash: Option<String>,
    received_ts: i64,
}

fn parse_trade_timestamp_ms(ts: &Option<String>) -> Option<i64> {
    let ts_str = ts.as_ref()?;
    if let Ok(ms) = ts_str.parse::<i64>() {
        return Some(ms);
    }
    DateTime::parse_from_rfc3339(ts_str)
        .map(|dt| dt.timestamp_millis())
        .ok()
}

fn build_trade_record_from_value(
    value: &serde_json::Value,
    venue: &str,
    mapping: &HashMap<String, (String, String)>,
) -> Option<PolymarketTradeRecord> {
    let msg_type = value.get("type").and_then(|v| v.as_str());
    let event_type = value.get("event_type").and_then(|v| v.as_str());
    let trade_type = msg_type.or(event_type);
    if trade_type != Some("last_trade_price")
        && trade_type != Some("trade")
        && trade_type != Some("trade_execution")
    {
        return None;
    }

    let payload = value.get("payload").unwrap_or(value);
    let asset_id = payload
        .get("asset_id")
        .and_then(|v| v.as_str())
        .or_else(|| value.get("asset_id").and_then(|v| v.as_str()))?
        .to_string();

    let (market_id, outcome_id) = mapping
        .get(&asset_id)
        .cloned()
        .unwrap_or((String::new(), String::new()));

    let event_type = payload
        .get("event_type")
        .and_then(|v| v.as_str())
        .or(trade_type)
        .unwrap_or("trade")
        .to_string();

    let price = payload
        .get("price")
        .and_then(|v| v.as_str())
        .or_else(|| value.get("price").and_then(|v| v.as_str()))
        .map(|s| s.to_string());
    let size = payload
        .get("size")
        .and_then(|v| v.as_str())
        .or_else(|| value.get("size").and_then(|v| v.as_str()))
        .map(|s| s.to_string());
    let side = payload
        .get("side")
        .and_then(|v| v.as_str())
        .or_else(|| value.get("side").and_then(|v| v.as_str()))
        .map(|s| s.to_string());
    let timestamp = payload
        .get("timestamp")
        .and_then(|v| v.as_str())
        .or_else(|| value.get("timestamp").and_then(|v| v.as_str()))
        .map(|s| s.to_string());
    let timestamp_ms = parse_trade_timestamp_ms(&timestamp);
    let transaction_hash = payload
        .get("transaction_hash")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());

    Some(PolymarketTradeRecord {
        venue: venue.to_string(),
        market_id: if market_id.is_empty() { None } else { Some(market_id) },
        outcome_id: if outcome_id.is_empty() { None } else { Some(outcome_id) },
        asset_id,
        event_type,
        price,
        size,
        side,
        timestamp,
        timestamp_ms,
        transaction_hash,
        received_ts: Utc::now().timestamp_millis(),
    })
}

fn trade_bucket(ts_ms: i64, bucket_minutes: u32) -> (String, String, String) {
    let dt = DateTime::<Utc>::from_timestamp_millis(ts_ms)
        .unwrap_or_else(|| Utc::now());
    let date_str = dt.format("%Y-%m-%d").to_string();
    let hour_str = dt.format("%H").to_string();
    let minute = dt.minute();
    let bucket_minute = (minute / bucket_minutes) * bucket_minutes;
    let minute_str = format!("{:02}", bucket_minute);
    (date_str, hour_str, minute_str)
}

fn write_trades_parquet(venue: &str, records: &[PolymarketTradeRecord]) -> Result<()> {
    if records.is_empty() {
        return Ok(());
    }
    let bucket_minutes = 5;
    let ts_ms = records[0].received_ts;
    let (date_str, hour_str, minute_str) = trade_bucket(ts_ms, bucket_minutes);

    let dir = Path::new("data")
        .join("trades")
        .join(format!("venue={}", venue))
        .join(format!("date={}", date_str))
        .join(format!("hour={}", hour_str));
    std::fs::create_dir_all(&dir)
        .with_context(|| format!("Failed to create trade directory: {:?}", dir))?;

    let file_prefix = format!("trades_{}T{}-{}", date_str, hour_str, minute_str);
    let temp_file = dir.join(format!("{}.parquet.tmp", file_prefix));
    let final_file = dir.join(format!("{}.parquet", file_prefix));

    let ts_recv: Vec<i64> = records.iter().map(|r| r.received_ts).collect();
    let venue_col: Vec<String> = records.iter().map(|r| r.venue.clone()).collect();
    let market_id: Vec<Option<String>> = records.iter().map(|r| r.market_id.clone()).collect();
    let outcome_id: Vec<Option<String>> = records.iter().map(|r| r.outcome_id.clone()).collect();
    let asset_id: Vec<String> = records.iter().map(|r| r.asset_id.clone()).collect();
    let event_type: Vec<String> = records.iter().map(|r| r.event_type.clone()).collect();
    let price: Vec<Option<String>> = records.iter().map(|r| r.price.clone()).collect();
    let size: Vec<Option<String>> = records.iter().map(|r| r.size.clone()).collect();
    let side: Vec<Option<String>> = records.iter().map(|r| r.side.clone()).collect();
    let timestamp: Vec<Option<String>> = records.iter().map(|r| r.timestamp.clone()).collect();
    let timestamp_ms: Vec<Option<i64>> = records.iter().map(|r| r.timestamp_ms).collect();
    let transaction_hash: Vec<Option<String>> =
        records.iter().map(|r| r.transaction_hash.clone()).collect();

    let df = DataFrame::new(vec![
        Series::new("ts_recv", ts_recv),
        Series::new("venue", venue_col),
        Series::new("market_id", market_id),
        Series::new("outcome_id", outcome_id),
        Series::new("asset_id", asset_id),
        Series::new("event_type", event_type),
        Series::new("price", price),
        Series::new("size", size),
        Series::new("side", side),
        Series::new("timestamp", timestamp),
        Series::new("timestamp_ms", timestamp_ms),
        Series::new("transaction_hash", transaction_hash),
    ])
    .context("Failed to create trade DataFrame")?;

    df.lazy()
        .sink_parquet(temp_file.clone(), ParquetWriteOptions::default())
        .context("Failed to write trade Parquet file")?;

    std::fs::rename(&temp_file, &final_file)
        .with_context(|| format!("Failed to rename {:?} to {:?}", temp_file, final_file))?;

    tracing::info!(
        "Wrote {} trade rows to {:?}",
        records.len(),
        final_file
    );

    Ok(())
}

#[derive(Debug, Serialize)]
struct PolymarketSubscribeMessage {
    #[serde(rename = "type")]
    message_type: String,  // "market" (lowercase)
    #[serde(rename = "assets_ids")]
    assets_ids: Vec<String>,  // List of token IDs (clobTokenIds), NOT condition IDs
    #[serde(rename = "custom_feature_enabled")]
    custom_feature_enabled: bool,
}

pub struct PolymarketVenue {
    name: String,
    #[allow(dead_code)]
    api_key: String,
    #[allow(dead_code)]
    api_secret: String,
    ws_url: String,
    rest_url: String,
    connected: Arc<AtomicBool>,
    #[allow(dead_code)]
    ws_stream: Arc<Mutex<Option<WebSocketStream<MaybeTlsStream<tokio::net::TcpStream>>>>>,
    ws_sender: Arc<Mutex<Option<futures::stream::SplitSink<WebSocketStream<MaybeTlsStream<tokio::net::TcpStream>>, Message>>>>,
    message_queue: Arc<Mutex<VecDeque<OrderBookUpdate>>>,
    #[allow(dead_code)]
    sequence: Arc<AtomicU64>,
    // Per-market/outcome sequence counters for gap detection
    market_sequences: Arc<Mutex<HashMap<(String, String), AtomicU64>>>,
    subscribed_markets: Arc<Mutex<HashMap<String, Vec<String>>>>, // market_id -> outcome_ids
    // Token ID (asset_id) -> (market_id, outcome_id) mapping
    token_to_market: Arc<Mutex<HashMap<String, (String, String)>>>,
    trade_buffer: Arc<Mutex<Vec<PolymarketTradeRecord>>>,
    trade_last_flush: Arc<Mutex<Instant>>,
}

impl PolymarketVenue {
    pub fn new(name: String, api_key: String, api_secret: String, ws_url: String, rest_url: String) -> Self {
        Self {
            name,
            api_key,
            api_secret,
            ws_url: if ws_url.is_empty() { "wss://gamma-api.polymarket.com/ws".to_string() } else { ws_url },
            rest_url: if rest_url.is_empty() { "https://gamma-api.polymarket.com".to_string() } else { rest_url },
            connected: Arc::new(AtomicBool::new(false)),
            ws_stream: Arc::new(Mutex::new(None)),
            ws_sender: Arc::new(Mutex::new(None)),
            message_queue: Arc::new(Mutex::new(VecDeque::new())),
            sequence: Arc::new(AtomicU64::new(1)),
            market_sequences: Arc::new(Mutex::new(HashMap::new())),
            subscribed_markets: Arc::new(Mutex::new(HashMap::new())),
            token_to_market: Arc::new(Mutex::new(HashMap::new())),
            trade_buffer: Arc::new(Mutex::new(Vec::new())),
            trade_last_flush: Arc::new(Mutex::new(Instant::now())),
        }
    }

    /// Load token_id -> (market_id, outcome_id) mapping from universe file
    #[allow(dead_code)]
    async fn load_token_mapping(&self, config: &crate::config::Config) -> Result<()> {
        use chrono::Utc;
        let today = Utc::now().date_naive();
        let date_str = today.format("%Y-%m-%d").to_string();
        
        let universe_path = std::path::Path::new(&config.data_dir)
            .join("metadata")
            .join(format!("venue={}", self.name))
            .join(format!("date={}", date_str))
            .join("universe.jsonl");
        
        if !universe_path.exists() {
            tracing::warn!("Universe file not found: {:?}, token mapping will be empty", universe_path);
            return Ok(());
        }
        
        let content = std::fs::read_to_string(&universe_path)
            .context("Failed to read universe file")?;
        
        let mut mapping = self.token_to_market.lock().await;
        for line in content.lines() {
            if let Ok(market_info) = serde_json::from_str::<crate::venue::MarketInfo>(line) {
                // Map each token_id to (market_id, outcome_id)
                // For binary markets, token_ids[0] -> outcome "0", token_ids[1] -> outcome "1"
                for (idx, token_id) in market_info.token_ids.iter().enumerate() {
                    let outcome_id = if idx < market_info.outcome_ids.len() {
                        market_info.outcome_ids[idx].clone()
                    } else {
                        format!("{}", idx) // Fallback to index
                    };
                    mapping.insert(token_id.clone(), (market_info.market_id.clone(), outcome_id));
                }
            }
        }
        
        tracing::info!("Loaded {} token_id -> (market_id, outcome_id) mappings", mapping.len());
        Ok(())
    }


    #[allow(dead_code)]
    fn parse_order_book_message(&self, msg: &str) -> Result<Option<OrderBookUpdate>> {
        let parsed: PolymarketOrderBookMessage = serde_json::from_str(msg)
            .context("Failed to parse Polymarket message")?;

        match parsed.message_type.as_str() {
            "orderbook" | "update" => {
                let market_id = parsed.market.as_ref()
                    .ok_or_else(|| anyhow::anyhow!("Missing market in message"))?
                    .clone();
                let outcome_id = parsed.outcome.as_ref()
                    .ok_or_else(|| anyhow::anyhow!("Missing outcome in message"))?
                    .clone();

                let bids = parsed.bids
                    .unwrap_or_default()
                    .into_iter()
                    .map(|b| OrderBookLevel { price: b[0], size: b[1] })
                    .collect();

                let asks = parsed.asks
                    .unwrap_or_default()
                    .into_iter()
                    .map(|a| OrderBookLevel { price: a[0], size: a[1] })
                    .collect();

                let seq = parsed.sequence.unwrap_or_else(|| {
                    self.sequence.fetch_add(1, Ordering::Relaxed) as i64
                });

                Ok(Some(OrderBookUpdate {
                    market_id,
                    outcome_id,
                    bids,
                    asks,
                    timestamp_ms: parsed.timestamp,
                    sequence: seq,
                }))
            }
            "subscribed" | "pong" => {
                // Ignore subscription confirmations and pongs
                Ok(None)
            }
            _ => {
                tracing::debug!("Unhandled Polymarket message type: {}", parsed.message_type);
                Ok(None)
            }
        }
    }
}

#[async_trait]
impl Venue for PolymarketVenue {
    fn name(&self) -> &str {
        &self.name
    }

    async fn discover_markets(&self) -> Result<Vec<MarketInfo>> {
        let client = reqwest::Client::new();
        
        // Use /events endpoint with closed=false to get active markets
        // Events contain markets in their markets array
        let mut all_markets = Vec::new();
        let mut offset = 0;
        let limit = 100; // Events endpoint limit
        let mut has_more = true;
        
        while has_more {
            let url = format!("{}/events?closed=false&limit={}&offset={}", self.rest_url, limit, offset);
            tracing::debug!("Fetching events from Polymarket (offset={}): {}", offset, url);
            
            let response = client
                .get(&url)
                .header("Accept", "application/json")
                .send()
                .await
                .context("Failed to fetch events from Polymarket")?;

            if !response.status().is_success() {
                anyhow::bail!("Polymarket API returned error: {}", response.status());
            }

            // Parse events response
            let events: Vec<PolymarketEvent> = match response.json::<serde_json::Value>().await {
                Ok(json) => {
                    if json.is_array() {
                        match serde_json::from_value::<Vec<PolymarketEvent>>(json) {
                            Ok(events) => events,
                            Err(e) => {
                                tracing::error!("Failed to deserialize Polymarket events array: {}", e);
                                anyhow::bail!("Failed to parse Polymarket events array: {}", e);
                            }
                        }
                    } else if let Some(data) = json.get("data") {
                        match serde_json::from_value::<Vec<PolymarketEvent>>(data.clone()) {
                            Ok(events) => events,
                            Err(e) => {
                                tracing::error!("Failed to deserialize Polymarket events from data field: {}", e);
                                anyhow::bail!("Failed to parse Polymarket events from data field: {}", e);
                            }
                        }
                    } else {
                        tracing::error!("Unexpected Polymarket API response format: {}", serde_json::to_string(&json).unwrap_or_default());
                        anyhow::bail!("Unexpected Polymarket API response format");
                    }
                }
                Err(e) => {
                    tracing::error!("Failed to parse Polymarket response as JSON: {}", e);
                    anyhow::bail!("Failed to parse Polymarket response: {}", e);
                }
            };
            
            if events.is_empty() {
                has_more = false;
            } else {
                let batch_size = events.len();
                
                // Extract markets from events
                for event in events {
                    if let Some(markets) = event.markets {
                        // Only include markets from active, non-closed events
                        if event.closed.unwrap_or(false) {
                            continue;
                        }
                        if let Some(false) = event.active {
                            continue;
                        }
                        
                        // Extract markets from this event
                        for market in markets {
                            // Filter: Only include markets that are open (not closed and active)
                            if market.closed.unwrap_or(false) {
                                continue; // Skip closed markets
                            }
                            if let Some(false) = market.active {
                                continue; // Skip inactive markets
                            }
                            all_markets.push(market);
                        }
                    }
                }
                
                // If we got fewer than the limit, we've reached the end
                if batch_size < limit {
                    has_more = false;
                } else {
                    offset += limit;
                }
            }
        }
        
        tracing::info!("Fetched {} total open markets from Polymarket events API", all_markets.len());

        let mut result = Vec::new();
        for market in all_markets {
            // Filter: Only include markets that are open (not closed and active)
            if market.closed.unwrap_or(false) {
                continue; // Skip closed markets
            }
            if let Some(false) = market.active {
                continue; // Skip inactive markets
            }
            
            // Polymarket uses condition_id as the market identifier
            // Outcomes are typically "0" (no) and "1" (yes) for binary markets
            let outcome_ids = if market.outcome_prices.is_some() {
                // Binary market
                vec!["0".to_string(), "1".to_string()]
            } else {
                // Multi-outcome market - we'll use indices
                vec!["0".to_string(), "1".to_string()] // Default to binary
            };

            let close_ts = market.end_date.as_ref()
                .and_then(|d| {
                    // Parse ISO 8601 date
                    chrono::DateTime::parse_from_rfc3339(d)
                        .ok()
                        .map(|dt| dt.timestamp_millis())
                });

            // Extract token IDs (clobTokenIds) - required for WebSocket subscriptions
            let token_ids = if let Some(clob_token_ids_raw) = &market.clob_token_ids {
                if let serde_json::Value::String(s) = clob_token_ids_raw {
                    // Parse JSON string like "[\"token1\", \"token2\"]"
                    serde_json::from_str::<Vec<String>>(s)
                        .unwrap_or_default()
                } else if let serde_json::Value::Array(arr) = clob_token_ids_raw {
                    // Already an array
                    arr.iter()
                        .filter_map(|v| v.as_str().map(|s| s.to_string()))
                        .collect()
                } else {
                    Vec::new()
                }
            } else {
                Vec::new()
            };

            result.push(MarketInfo {
                market_id: market.condition_id.clone(),
                title: market.question,
                outcome_ids,
                close_ts,
                status: if market.closed.unwrap_or(false) {
                    "closed".to_string()
                } else if market.active.unwrap_or(true) {
                    "active".to_string()
                } else {
                    "inactive".to_string()
                },
                tags: vec![market.slug],
                token_ids,
            });
        }

        tracing::info!("Discovered {} markets from Polymarket", result.len());
        Ok(result)
    }

    async fn connect_websocket(&self) -> Result<()> {
        if self.connected.load(Ordering::Relaxed) {
            tracing::warn!("WebSocket already connected");
            return Ok(());
        }

        tracing::info!("Connecting to Polymarket WebSocket: {}", self.ws_url);
        
        let url = url::Url::parse(&self.ws_url)
            .context("Invalid WebSocket URL")?;

        tracing::debug!("Parsed URL: scheme={}, host={:?}", url.scheme(), url.host_str());

        // Extract values before moving url
        let scheme = url.scheme().to_string();
        let host = url.host_str()
            .ok_or_else(|| anyhow::anyhow!("No host in URL"))?
            .to_string();
        
        tracing::debug!("Resolving address for host: {}", host);
        let addr = url.socket_addrs(|| None)
            .context("Failed to resolve WebSocket address")?[0];
        tracing::debug!("Resolved address: {}", addr);
        
        // Create WebSocket connection using tokio_tungstenite
        let request = url.into_client_request()
            .context("Failed to create WebSocket request")?;
        
        tracing::debug!("Connecting TCP stream to {}", addr);
        // Connect TCP stream with timeout
        let tcp_stream = tokio::time::timeout(
            std::time::Duration::from_secs(5),
            tokio::net::TcpStream::connect(addr)
        )
        .await
        .context("TCP connection timeout")?
        .context("Failed to connect TCP stream")?;
        tracing::debug!("TCP stream connected");
        
        // For wss://, wrap in TLS
        let stream: MaybeTlsStream<tokio::net::TcpStream> = if scheme == "wss" {
            tracing::debug!("Establishing TLS connection to {}", host);
            let tls_connector = native_tls::TlsConnector::builder()
                .build()
                .context("Failed to create TLS connector")?;
            let tls_stream = tokio::time::timeout(
                std::time::Duration::from_secs(5),
                tokio_native_tls::TlsConnector::from(tls_connector)
                    .connect(&host, tcp_stream)
            )
            .await
            .context("TLS connection timeout")?
            .context("Failed to establish TLS connection")?;
            tracing::debug!("TLS connection established");
            MaybeTlsStream::NativeTls(tls_stream)
        } else {
            MaybeTlsStream::Plain(tcp_stream)
        };
        
        tracing::debug!("Upgrading to WebSocket protocol (this may take a moment)");
        // Use client_async with timeout
        let (ws_stream, response) = tokio::time::timeout(
            std::time::Duration::from_secs(10),
            tokio_tungstenite::client_async(request, stream)
        )
        .await
        .context("WebSocket handshake timeout - server may not be responding")?
        .context("Failed to upgrade to WebSocket - check if endpoint is correct")?;
        tracing::debug!("WebSocket upgrade complete, response status: {:?}", response.status());

        let (sender, mut receiver) = ws_stream.split();

        // Store sender
        *self.ws_sender.lock().await = Some(sender);
        self.connected.store(true, Ordering::Relaxed);

        tracing::info!("Connected to Polymarket WebSocket");

        // Start message processing loop
        let message_queue = self.message_queue.clone();
        let market_sequences = self.market_sequences.clone();
        let token_to_market = self.token_to_market.clone();
        let venue_name = self.name.clone();
        let trade_buffer = self.trade_buffer.clone();
        let trade_last_flush = self.trade_last_flush.clone();
        
        tokio::spawn(async move {
            // Load token mapping on first message
            let mut mapping_loaded = false;
            let mut trade_count: u64 = 0;
            let mut last_trade_log = Instant::now();
            
            while let Some(msg) = receiver.next().await {
                match msg {
                    Ok(Message::Text(text)) => {
                        tracing::debug!("Received WebSocket message ({} bytes): {}", text.len(), 
                            if text.len() > 500 { format!("{}...", &text[..500]) } else { text.clone() });
                        
                        // Load token mapping if not already loaded
                        if !mapping_loaded {
                            // Try to load from today's universe file
                            use chrono::Utc;
                            let today = Utc::now().date_naive();
                            let date_str = today.format("%Y-%m-%d").to_string();
                            
                            // Try to find universe file in default location
                            // TODO: Pass config to venue or make data_dir configurable
                            let data_dir = "data"; // Default - should match config.data_dir
                            let universe_path = std::path::Path::new(data_dir)
                                .join("metadata")
                                .join(format!("venue={}", venue_name))
                                .join(format!("date={}", date_str))
                                .join("universe.jsonl");
                            
                            if universe_path.exists() {
                                if let Ok(content) = std::fs::read_to_string(&universe_path) {
                                    let mut mapping = token_to_market.lock().await;
                                    for line in content.lines() {
                                        if let Ok(market_info) = serde_json::from_str::<crate::venue::MarketInfo>(line) {
                                            for (idx, token_id) in market_info.token_ids.iter().enumerate() {
                                                let outcome_id = if idx < market_info.outcome_ids.len() {
                                                    market_info.outcome_ids[idx].clone()
                                                } else {
                                                    format!("{}", idx)
                                                };
                                                mapping.insert(token_id.clone(), (market_info.market_id.clone(), outcome_id));
                                            }
                                        }
                                    }
                                    tracing::info!("Loaded {} token_id mappings", mapping.len());
                                    mapping_loaded = true;
                                }
                            } else {
                                tracing::warn!("Universe file not found: {:?}, token mapping will be empty", universe_path);
                            }
                        }
                        
                        // Try parsing as CLOB message formats
                        let mut parsed_any = false;
                        
                        // Try as order book snapshot
                        if let Ok(snapshot) = serde_json::from_str::<PolymarketClobOrderBookSnapshot>(&text) {
                            let mapping = token_to_market.lock().await;
                            if let Some((market_id, outcome_id)) = mapping.get(&snapshot.asset_id) {
                                let bids: Vec<OrderBookLevel> = snapshot.bids.as_ref()
                                    .map(|bids| bids.iter()
                                        .map(|b| OrderBookLevel {
                                            price: b.price.parse().unwrap_or(0.0),
                                            size: b.size.parse().unwrap_or(0.0),
                                        })
                                        .collect())
                                    .unwrap_or_default();
                                
                                let asks: Vec<OrderBookLevel> = snapshot.asks.as_ref()
                                    .map(|asks| asks.iter()
                                        .map(|a| OrderBookLevel {
                                            price: a.price.parse().unwrap_or(0.0),
                                            size: a.size.parse().unwrap_or(0.0),
                                        })
                                        .collect())
                                    .unwrap_or_default();
                                
                                let timestamp_ms = snapshot.timestamp.as_ref()
                                    .and_then(|ts| ts.parse::<i64>().ok());
                                
                                // Use per-market/outcome sequence counter for gap detection
                                // (Polymarket CLOB doesn't provide sequence numbers)
                                let seq_key = (market_id.clone(), outcome_id.clone());
                                let seq = {
                                    let mut market_seqs = market_sequences.lock().await;
                                    let counter = market_seqs.entry(seq_key.clone())
                                        .or_insert_with(|| AtomicU64::new(1));
                                    counter.fetch_add(1, Ordering::Relaxed) as i64
                                };
                                
                                let update = OrderBookUpdate {
                                    market_id: market_id.clone(),
                                    outcome_id: outcome_id.clone(),
                                    bids,
                                    asks,
                                    timestamp_ms,
                                    sequence: seq,
                                };
                                
                                let bids_len = update.bids.len();
                                let asks_len = update.asks.len();
                                {
                                    let mut queue = message_queue.lock().await;
                                    queue.push_back(update);
                                    // Log queue depth periodically
                                    if queue.len() % 100 == 0 {
                                        tracing::debug!("Message queue depth: {}", queue.len());
                                    }
                                }
                                parsed_any = true;
                                tracing::debug!("Parsed CLOB order book snapshot: market={}, asset_id={}, bids={}, asks={}", 
                                    market_id, snapshot.asset_id, bids_len, asks_len);
                            } else {
                                tracing::debug!("No mapping found for asset_id={}", snapshot.asset_id);
                            }
                        }
                        
                        // Try as price changes
                        if !parsed_any {
                            if let Ok(price_changes) = serde_json::from_str::<PolymarketClobPriceChanges>(&text) {
                                let mapping = token_to_market.lock().await;
                                
                                for change in &price_changes.price_changes {
                                    if let Some((market_id, outcome_id)) = mapping.get(&change.asset_id) {
                                        // For price changes, we'll create a minimal update
                                        // In production, you'd maintain incremental order book state
                                        let price = change.price.parse().unwrap_or(0.0);
                                        let size = change.size.parse().unwrap_or(0.0);
                                        
                                        // Use best_bid/best_ask if available, otherwise use the price
                                        let best_bid = change.best_bid.as_ref()
                                            .and_then(|bb| bb.parse::<f64>().ok())
                                            .unwrap_or(if change.side == "BUY" { price } else { 0.0 });
                                        let best_ask = change.best_ask.as_ref()
                                            .and_then(|ba| ba.parse::<f64>().ok())
                                            .unwrap_or(if change.side == "SELL" { price } else { 0.0 });
                                        
                                        // Create update with best bid/ask
                                        let bids = if best_bid > 0.0 {
                                            vec![OrderBookLevel { price: best_bid, size }]
                                        } else {
                                            vec![]
                                        };
                                        let asks = if best_ask > 0.0 {
                                            vec![OrderBookLevel { price: best_ask, size }]
                                        } else {
                                            vec![]
                                        };
                                        
                                        // Use per-market/outcome sequence counter
                                        let seq_key = (market_id.clone(), outcome_id.clone());
                                        let seq = {
                                            let mut market_seqs = market_sequences.lock().await;
                                            let counter = market_seqs.entry(seq_key)
                                                .or_insert_with(|| AtomicU64::new(1));
                                            counter.fetch_add(1, Ordering::Relaxed) as i64
                                        };
                                        
                                        let update = OrderBookUpdate {
                                            market_id: market_id.clone(),
                                            outcome_id: outcome_id.clone(),
                                            bids,
                                            asks,
                                            timestamp_ms: None,
                                            sequence: seq,
                                        };
                                        
                                        message_queue.lock().await.push_back(update);
                                        parsed_any = true;
                                        tracing::debug!("Parsed CLOB price change: market={}, asset_id={}, side={}", 
                                            market_id, change.asset_id, change.side);
                                    }
                                }
                            }
                        }
                        
                        // Try as trade event (fills/executions)
                        if !parsed_any {
                            if let Ok(trade) = serde_json::from_str::<PolymarketClobTradeEvent>(&text) {
                                let (market_id, outcome_id) = {
                                    let mapping = token_to_market.lock().await;
                                    mapping.get(&trade.asset_id).cloned().unwrap_or((String::new(), String::new()))
                                };
                                let record = PolymarketTradeRecord {
                                    venue: venue_name.clone(),
                                    market_id: if market_id.is_empty() { None } else { Some(market_id) },
                                    outcome_id: if outcome_id.is_empty() { None } else { Some(outcome_id) },
                                    asset_id: trade.asset_id.clone(),
                                    event_type: trade.event_type.clone(),
                                    price: trade.price.clone(),
                                    size: trade.size.clone(),
                                    side: trade.side.clone(),
                                    timestamp: trade.timestamp.clone(),
                                    timestamp_ms: parse_trade_timestamp_ms(&trade.timestamp),
                                    transaction_hash: trade.transaction_hash.clone(),
                                    received_ts: Utc::now().timestamp_millis(),
                                };
                                {
                                    let mut buffer = trade_buffer.lock().await;
                                    buffer.push(record);
                                    trade_count += 1;
                                }

                                let should_flush = {
                                    let buffer = trade_buffer.lock().await;
                                    let last_flush = trade_last_flush.lock().await;
                                    buffer.len() >= 500 || last_flush.elapsed() >= Duration::from_secs(5)
                                };

                                if should_flush {
                                    let records = {
                                        let mut buffer = trade_buffer.lock().await;
                                        let drained = buffer.drain(..).collect::<Vec<_>>();
                                        drained
                                    };
                                    if !records.is_empty() {
                                        if let Err(e) = write_trades_parquet(&venue_name, &records) {
                                            tracing::warn!("Failed to write trades parquet: {}", e);
                                        } else {
                                            let mut last_flush = trade_last_flush.lock().await;
                                            *last_flush = Instant::now();
                                        }
                                    }
                                }
                                tracing::debug!("Recorded trade event: asset_id={}", trade.asset_id);
                                parsed_any = true;
                            }
                        }

                        if !parsed_any {
                            if let Ok(value) = serde_json::from_str::<serde_json::Value>(&text) {
                                let mapping = token_to_market.lock().await;
                                if let Some(record) = build_trade_record_from_value(&value, &venue_name, &mapping) {
                                    {
                                        let mut buffer = trade_buffer.lock().await;
                                        buffer.push(record);
                                        trade_count += 1;
                                    }

                                    let should_flush = {
                                        let buffer = trade_buffer.lock().await;
                                        let last_flush = trade_last_flush.lock().await;
                                        buffer.len() >= 500 || last_flush.elapsed() >= Duration::from_secs(5)
                                    };

                                    if should_flush {
                                        let records = {
                                            let mut buffer = trade_buffer.lock().await;
                                            buffer.drain(..).collect::<Vec<_>>()
                                        };
                                        if !records.is_empty() {
                                            if let Err(e) = write_trades_parquet(&venue_name, &records) {
                                                tracing::warn!("Failed to write trades parquet: {}", e);
                                            } else {
                                                let mut last_flush = trade_last_flush.lock().await;
                                                *last_flush = Instant::now();
                                            }
                                        }
                                    }

                                    tracing::debug!("Recorded trade event from message type");
                                    parsed_any = true;
                                }
                            }
                        }

                        if last_trade_log.elapsed() >= Duration::from_secs(60) {
                            tracing::info!("Trade events seen in last 60s: {}", trade_count);
                            trade_count = 0;
                            last_trade_log = Instant::now();
                        }
                        
                        // Try as array of messages
                        if !parsed_any {
                            if let Ok(messages) = serde_json::from_str::<Vec<serde_json::Value>>(&text) {
                                // Process each message in the array
                                for msg_json in messages {
                                    if let Ok(msg_text) = serde_json::to_string(&msg_json) {
                                        // Recursively parse (simplified - in production, extract to function)
                                        if let Ok(snapshot) = serde_json::from_str::<PolymarketClobOrderBookSnapshot>(&msg_text) {
                                            let mapping = token_to_market.lock().await;
                                            if let Some((market_id, outcome_id)) = mapping.get(&snapshot.asset_id) {
                                                let bids: Vec<OrderBookLevel> = snapshot.bids.as_ref()
                                                    .map(|bids| bids.iter()
                                                        .map(|b| OrderBookLevel {
                                                            price: b.price.parse().unwrap_or(0.0),
                                                            size: b.size.parse().unwrap_or(0.0),
                                                        })
                                                        .collect())
                                                    .unwrap_or_default();
                                                
                                                let asks: Vec<OrderBookLevel> = snapshot.asks.as_ref()
                                                    .map(|asks| asks.iter()
                                                        .map(|a| OrderBookLevel {
                                                            price: a.price.parse().unwrap_or(0.0),
                                                            size: a.size.parse().unwrap_or(0.0),
                                                        })
                                                        .collect())
                                                    .unwrap_or_default();
                                                
                                                let timestamp_ms = snapshot.timestamp.as_ref()
                                                    .and_then(|ts| ts.parse::<i64>().ok());
                                                
                                                // Use per-market/outcome sequence counter
                                                let seq_key = (market_id.clone(), outcome_id.clone());
                                                let seq = {
                                                    let mut market_seqs = market_sequences.lock().await;
                                                    let counter = market_seqs.entry(seq_key)
                                                        .or_insert_with(|| AtomicU64::new(1));
                                                    counter.fetch_add(1, Ordering::Relaxed) as i64
                                                };
                                                
                                                let update = OrderBookUpdate {
                                                    market_id: market_id.clone(),
                                                    outcome_id: outcome_id.clone(),
                                                    bids,
                                                    asks,
                                                    timestamp_ms,
                                                    sequence: seq,
                                                };
                                                
                                                message_queue.lock().await.push_back(update);
                                                tracing::debug!("Parsed CLOB snapshot from array: market={}", market_id);
                                            }
                                        }
                                    }
                                }
                                parsed_any = true;
                            }
                        }
                        
                        if !parsed_any {
                            tracing::debug!("Message did not match any known CLOB format");
                        }
                    }
                    Ok(Message::Ping(_data)) => {
                        // Handle ping - will be auto-responded by tungstenite
                        tracing::debug!("Received ping from Polymarket");
                    }
                    Ok(Message::Close(_)) => {
                        tracing::warn!("Polymarket WebSocket closed");
                        break;
                    }
                    Err(e) => {
                        tracing::error!("WebSocket error: {}", e);
                        break;
                    }
                    _ => {}
                }
            }
            tracing::warn!("Polymarket WebSocket receiver loop ended");
        });

        Ok(())
    }

    async fn subscribe(&self, market_ids: &[String], outcome_ids: &[String]) -> Result<()> {
        if !self.connected.load(Ordering::Relaxed) {
            anyhow::bail!("WebSocket not connected");
        }

        let mut sender = self.ws_sender.lock().await;
        let sender = sender.as_mut()
            .ok_or_else(|| anyhow::anyhow!("WebSocket sender not available"))?;

        let max_batch = 500usize;
        let mut assets_ids = market_ids.to_vec();
        if assets_ids.len() > max_batch {
            tracing::warn!(
                "Subscribe batch exceeds cap ({}). Truncating from {}.",
                max_batch,
                assets_ids.len()
            );
            assets_ids.truncate(max_batch);
        }
        // Polymarket CLOB WebSocket subscription format
        // IMPORTANT: market_ids should be token IDs (clobTokenIds), NOT condition IDs
        // Format: {"type": "market", "assets_ids": ["token_id_1", "token_id_2"], "custom_feature_enabled": false}
        let subscribe_msg = PolymarketSubscribeMessage {
            message_type: "market".to_string(),  // Lowercase per documentation
            assets_ids: assets_ids.clone(),  // Token IDs (clobTokenIds from Gamma API)
            custom_feature_enabled: false,
        };

        let msg_text = serde_json::to_string(&subscribe_msg)
            .context("Failed to serialize subscribe message")?;

        tracing::info!(
            "Subscribing to {} token IDs: {:?}",
            assets_ids.len(),
            &assets_ids[..assets_ids.len().min(3)]
        );
        sender.send(Message::Text(msg_text))
            .await
            .context("Failed to send subscribe message")?;

        tracing::info!("Subscription message sent for {} token IDs", assets_ids.len());

        // Track subscriptions (note: using market_ids as token IDs)
        let mut subs = self.subscribed_markets.lock().await;
        for market_id in &assets_ids {
            subs.insert(market_id.clone(), outcome_ids.to_vec());
        }

        Ok(())
    }

    async fn unsubscribe(&self, market_ids: &[String], outcome_ids: &[String]) -> Result<()> {
        if !self.connected.load(Ordering::Relaxed) {
            return Ok(()); // Already disconnected
        }

        let mut sender = self.ws_sender.lock().await;
        let sender = sender.as_mut()
            .ok_or_else(|| anyhow::anyhow!("WebSocket sender not available"))?;

        // Polymarket unsubscribe format
        for market_id in market_ids {
            for outcome_id in outcome_ids {
                let unsubscribe_msg = serde_json::json!({
                    "type": "unsubscribe",
                    "channel": "orderbook",
                    "market": market_id,
                    "outcome": outcome_id,
                });

                let msg_text = serde_json::to_string(&unsubscribe_msg)
                    .context("Failed to serialize unsubscribe message")?;

                sender.send(Message::Text(msg_text))
                    .await
                    .context("Failed to send unsubscribe message")?;

                tracing::debug!("Unsubscribed from Polymarket {}/{}", market_id, outcome_id);
            }
        }

        // Remove from tracked subscriptions
        let mut subs = self.subscribed_markets.lock().await;
        for market_id in market_ids {
            if let Some(outcomes) = subs.get_mut(market_id) {
                outcomes.retain(|o| !outcome_ids.contains(o));
                if outcomes.is_empty() {
                    subs.remove(market_id);
                }
            }
        }

        Ok(())
    }

    async fn receive_update(&mut self) -> Result<Option<OrderBookUpdate>> {
        let mut queue = self.message_queue.lock().await;
        let update = queue.pop_front();
        if update.is_some() {
            tracing::debug!("Popped update from queue: market={}, outcome={}, queue_size={}", 
                update.as_ref().unwrap().market_id, 
                update.as_ref().unwrap().outcome_id,
                queue.len());
        }
        Ok(update)
    }

    fn is_connected(&self) -> bool {
        self.connected.load(Ordering::Relaxed)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_polymarket_venue_creation() {
        let venue = PolymarketVenue::new(
            "polymarket".to_string(),
            "test_key".to_string(),
            "test_secret".to_string(),
            "wss://test".to_string(),
            "https://test".to_string(),
        );
        assert_eq!(venue.name(), "polymarket");
        assert!(!venue.is_connected());
    }

    #[tokio::test]
    async fn test_polymarket_parse_message() {
        let venue = PolymarketVenue::new(
            "polymarket".to_string(),
            "".to_string(),
            "".to_string(),
            "".to_string(),
            "".to_string(),
        );

        let msg = r#"{
            "type": "orderbook",
            "market": "0x123",
            "outcome": "0",
            "bids": [[0.5, 100.0], [0.49, 200.0]],
            "asks": [[0.51, 150.0], [0.52, 100.0]],
            "timestamp": 1234567890,
            "sequence": 1
        }"#;

        let update = venue.parse_order_book_message(msg).unwrap();
        assert!(update.is_some());
        let update = update.unwrap();
        assert_eq!(update.market_id, "0x123");
        assert_eq!(update.outcome_id, "0");
        assert_eq!(update.bids.len(), 2);
        assert_eq!(update.asks.len(), 2);
    }
}
