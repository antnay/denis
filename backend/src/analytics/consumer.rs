use clickhouse::{Client, Row};
use hickory_proto::{op::ResponseCode, rr::RecordType};
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
            query_type: RecordType::from(e.query_type).to_string(),
            // EDNS. `u16::from(ResponseCode)` packs them as (high << 4) | low.
            response_code: ResponseCode::from(
                (e.response_code >> 4) as u8,
                (e.response_code & 0x0F) as u8,
            )
            .to_string(),
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
    pub fn new(
        brokers: &str,
        group_id: &str,
        ch_url: &str,
        ch_user: &str,
        ch_password: &str,
    ) -> Self {
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

    async fn ensure_table(&self) -> Result<(), clickhouse::error::Error> {
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
    }

    pub async fn run(self) {
        let mut backoff = Duration::from_secs(1);
        while let Err(e) = self.ensure_table().await {
            ftlog::error!("ClickHouse ensure_table failed (retry in {backoff:?}): {e}");
            time::sleep(backoff).await;
            backoff = (backoff * 2).min(Duration::from_secs(30));
        }

        let consumer = self.consumer;
        let ch = self.ch;
        let mut batch: Vec<DnsQueryRow> = Vec::with_capacity(BATCH_SIZE);
        let mut flush_interval = time::interval(Duration::from_millis(FLUSH_INTERVAL_MS));
        flush_interval.tick().await;

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
