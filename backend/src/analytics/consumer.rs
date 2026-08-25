use clickhouse::{Client, Row};
use rdkafka::{
    config::ClientConfig,
    consumer::{CommitMode, Consumer, StreamConsumer},
    message::Message,
};
use serde::{Deserialize, Serialize};
use std::time::Duration;
use tokio::time;

use super::producer::DnsQueryEvent;

const BATCH_SIZE: usize = 1_000;
const FLUSH_INTERVAL_MS: u64 = 5_000;

#[derive(Row, Serialize, Deserialize, Debug, Clone)]
pub struct DnsQueryRow {
    pub timestamp_ms: u64,
    pub domain: String,
    pub query_type: String,
    pub response_code: String,
    pub cache_hit: u8,
    pub blocked: u8,
    pub latency_us: u64,
}

impl From<DnsQueryEvent> for DnsQueryRow {
    fn from(e: DnsQueryEvent) -> Self {
        Self {
            timestamp_ms: e.timestamp_ms,
            domain: e.domain,
            query_type: e.query_type,
            response_code: e.response_code,
            cache_hit: e.cache_hit as u8,
            blocked: e.blocked as u8,
            latency_us: e.latency_us,
        }
    }
}

pub struct AnalyticsConsumer {
    consumer: StreamConsumer,
    ch: Client,
}

impl AnalyticsConsumer {
    pub fn new(brokers: &str, group_id: &str, ch_url: &str, ch_user: &str, ch_password: &str) -> Self {
        let consumer: StreamConsumer = ClientConfig::new()
            .set("bootstrap.servers", brokers)
            .set("group.id", group_id)
            .set("enable.auto.commit", "false")
            .set("auto.offset.reset", "earliest")
            .create()
            .expect("Failed to create Kafka consumer");

        consumer
            .subscribe(&[super::producer::TOPIC])
            .expect("Failed to subscribe to dns-queries topic");

        let ch = Client::default()
            .with_url(ch_url)
            .with_user(ch_user)
            .with_password(ch_password);

        Self { consumer, ch }
    }

    async fn ensure_table(&self) {
        self.ch
            .query(
                "CREATE TABLE IF NOT EXISTS dns_queries (
                    timestamp_ms  UInt64,
                    domain        String,
                    query_type    String,
                    response_code String,
                    cache_hit     UInt8,
                    blocked       UInt8,
                    latency_us    UInt64
                ) ENGINE = MergeTree()
                ORDER BY (timestamp_ms, domain)",
            )
            .execute()
            .await
            .expect("Failed to create dns_queries table in ClickHouse");
    }

    pub async fn run(self) {
        self.ensure_table().await;

        let consumer = self.consumer;
        let ch = self.ch;
        let mut batch: Vec<DnsQueryRow> = Vec::with_capacity(BATCH_SIZE);
        let mut flush_interval = time::interval(Duration::from_millis(FLUSH_INTERVAL_MS));
        flush_interval.tick().await; // skip the immediate first tick

        loop {
            tokio::select! {
                msg = consumer.recv() => {
                    match msg {
                        Ok(m) => {
                            if let Some(payload) = m.payload() {
                                if let Ok(event) = serde_json::from_slice::<DnsQueryEvent>(payload) {
                                    batch.push(DnsQueryRow::from(event));
                                    if batch.len() >= BATCH_SIZE {
                                        flush_batch(&ch, &mut batch).await;
                                        consumer.commit_message(&m, CommitMode::Async).ok();
                                    }
                                }
                            }
                        }
                        Err(e) => ftlog::error!("Kafka consumer error: {}", e),
                    }
                }
                _ = flush_interval.tick() => {
                    if !batch.is_empty() {
                        flush_batch(&ch, &mut batch).await;
                    }
                }
            }
        }
    }
}

async fn flush_batch(ch: &Client, batch: &mut Vec<DnsQueryRow>) {
    match ch.insert("dns_queries") {
        Ok(mut insert) => {
            for row in batch.iter() {
                if let Err(e) = insert.write(row).await {
                    ftlog::error!("ClickHouse write error: {}", e);
                }
            }
            match insert.end().await {
                Ok(_) => ftlog::info!("Flushed {} DNS events to ClickHouse", batch.len()),
                Err(e) => ftlog::error!("ClickHouse flush error: {}", e),
            }
        }
        Err(e) => ftlog::error!("ClickHouse insert error: {}", e),
    }
    batch.clear();
}
