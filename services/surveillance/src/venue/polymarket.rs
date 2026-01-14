use super::traits::{MarketInfo, OrderBookLevel, OrderBookUpdate, Venue};
use anyhow::{Context, Result};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use tokio::sync::Mutex;
use tokio_tungstenite::{tungstenite::{Message, client::IntoClientRequest}, MaybeTlsStream, WebSocketStream};
use futures::{SinkExt, StreamExt};

#[derive(Debug, Serialize, Deserialize)]
struct PolymarketMarket {
    #[serde(rename = "conditionId")]
    condition_id: String,
    #[serde(rename = "questionId")]
    question_id: String,
    question: String,
    slug: String,
    #[serde(rename = "endDate")]
    end_date: Option<String>,
    #[serde(rename = "outcomePrices")]
    outcome_prices: Option<Vec<f64>>,
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
    liquidity: Option<f64>,
    volume: Option<f64>,
    #[serde(rename = "newQuestion")]
    new_question: Option<bool>,
}

#[derive(Debug, Serialize, Deserialize)]
struct PolymarketMarketResponse {
    data: Vec<PolymarketMarket>,
}

#[derive(Debug, Serialize, Deserialize)]
struct PolymarketOutcome {
    id: String,
    price: f64,
}

#[derive(Debug, Serialize, Deserialize)]
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

#[derive(Debug, Serialize)]
struct PolymarketSubscribeMessage {
    #[serde(rename = "type")]
    message_type: String,
    #[serde(rename = "channel")]
    channel: String,
    #[serde(rename = "market")]
    market: String,
    #[serde(rename = "outcome")]
    outcome: Option<String>,
}

pub struct PolymarketVenue {
    name: String,
    api_key: String,
    api_secret: String,
    ws_url: String,
    rest_url: String,
    connected: Arc<AtomicBool>,
    ws_stream: Arc<Mutex<Option<WebSocketStream<MaybeTlsStream<tokio::net::TcpStream>>>>>,
    ws_sender: Arc<Mutex<Option<futures::stream::SplitSink<WebSocketStream<MaybeTlsStream<tokio::net::TcpStream>>, Message>>>>,
    message_queue: Arc<Mutex<Vec<OrderBookUpdate>>>,
    sequence: Arc<AtomicU64>,
    subscribed_markets: Arc<Mutex<HashMap<String, Vec<String>>>>, // market_id -> outcome_ids
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
            message_queue: Arc::new(Mutex::new(Vec::new())),
            sequence: Arc::new(AtomicU64::new(1)),
            subscribed_markets: Arc::new(Mutex::new(HashMap::new())),
        }
    }

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
        let url = format!("{}/markets", self.rest_url);
        
        tracing::info!("Fetching markets from Polymarket: {}", url);
        
        let response = client
            .get(&url)
            .header("Accept", "application/json")
            .send()
            .await
            .context("Failed to fetch markets from Polymarket")?;

        if !response.status().is_success() {
            anyhow::bail!("Polymarket API returned error: {}", response.status());
        }

        let markets: Vec<PolymarketMarket> = response
            .json::<PolymarketMarketResponse>()
            .await
            .context("Failed to parse Polymarket markets response")?
            .data;

        let mut result = Vec::new();
        for market in markets {
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

        // Extract values before moving url
        let scheme = url.scheme().to_string();
        let addr = url.socket_addrs(|| None)
            .context("Failed to resolve WebSocket address")?[0];
        let host = url.host_str()
            .ok_or_else(|| anyhow::anyhow!("No host in URL"))?
            .to_string();
        
        // Create WebSocket connection using tokio_tungstenite
        let request = url.into_client_request()
            .context("Failed to create WebSocket request")?;
        
        // Connect TCP stream
        let tcp_stream = tokio::net::TcpStream::connect(addr).await
            .context("Failed to connect TCP stream")?;
        
        // For wss://, wrap in TLS
        let stream: MaybeTlsStream<tokio::net::TcpStream> = if scheme == "wss" {
            let tls_connector = native_tls::TlsConnector::builder()
                .build()
                .context("Failed to create TLS connector")?;
            let tls_stream = tokio_native_tls::TlsConnector::from(tls_connector)
                .connect(&host, tcp_stream)
                .await
                .context("Failed to establish TLS connection")?;
            MaybeTlsStream::NativeTls(tls_stream)
        } else {
            MaybeTlsStream::Plain(tcp_stream)
        };
        
        let (ws_stream, _) = tokio_tungstenite::client_async(request, stream)
            .await
            .context("Failed to upgrade to WebSocket")?;

        let (sender, mut receiver) = ws_stream.split();

        // Store sender
        *self.ws_sender.lock().await = Some(sender);
        self.connected.store(true, Ordering::Relaxed);

        tracing::info!("Connected to Polymarket WebSocket");

        // Start message processing loop
        let message_queue = self.message_queue.clone();
        let sequence = self.sequence.clone();
        
        tokio::spawn(async move {
            while let Some(msg) = receiver.next().await {
                match msg {
                    Ok(Message::Text(text)) => {
                        // Parse and queue order book updates
                        if let Ok(parsed) = serde_json::from_str::<PolymarketOrderBookMessage>(&text) {
                            if let (Some(market), Some(outcome)) = (parsed.market, parsed.outcome) {
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

                                let update = OrderBookUpdate {
                                    market_id: market,
                                    outcome_id: outcome,
                                    bids,
                                    asks,
                                    timestamp_ms: parsed.timestamp,
                                    sequence: parsed.sequence.unwrap_or_else(|| {
                                        sequence.fetch_add(1, Ordering::Relaxed) as i64
                                    }),
                                };

                                message_queue.lock().await.push(update);
                            }
                        }
                    }
                    Ok(Message::Ping(data)) => {
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

        // Polymarket subscription format
        for market_id in market_ids {
            for outcome_id in outcome_ids {
                let subscribe_msg = PolymarketSubscribeMessage {
                    message_type: "subscribe".to_string(),
                    channel: "orderbook".to_string(),
                    market: market_id.clone(),
                    outcome: Some(outcome_id.clone()),
                };

                let msg_text = serde_json::to_string(&subscribe_msg)
                    .context("Failed to serialize subscribe message")?;

                sender.send(Message::Text(msg_text))
                    .await
                    .context("Failed to send subscribe message")?;

                tracing::debug!("Subscribed to Polymarket {}/{}", market_id, outcome_id);
            }
        }

        // Track subscriptions
        let mut subs = self.subscribed_markets.lock().await;
        for market_id in market_ids {
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
        Ok(queue.pop())
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
