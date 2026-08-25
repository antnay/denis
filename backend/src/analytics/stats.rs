use clickhouse::{Client, Row};
use rdkafka::{
    config::ClientConfig,
    consumer::{BaseConsumer, Consumer},
};
use serde::{Deserialize, Serialize};
use std::time::Duration;

use super::producer::TOPIC;

// ── ClickHouse row types ──────────────────────────────────────────────────────

#[derive(Row, Deserialize)]
struct U64Row {
    value: u64,
}

#[derive(Row, Deserialize)]
struct F64Row {
    value: f64,
}

#[derive(Row, Deserialize)]
struct DomainRow {
    domain: String,
    count: u64,
}

#[derive(Row, Deserialize)]
struct RespCodeRow {
    response_code: String,
    count: u64,
}

// ── Public response types ─────────────────────────────────────────────────────

#[derive(Serialize)]
pub struct KafkaStats {
    pub topic: String,
    pub partitions: i32,
    /// High-watermark sum across all partitions (total messages ever produced
    /// minus any that fell off the retention window).
    pub total_messages: i64,
    /// How many messages the analytics consumer has not yet flushed to ClickHouse.
    pub consumer_lag: i64,
}

#[derive(Serialize)]
pub struct DomainCount {
    pub domain: String,
    pub count: u64,
}

#[derive(Serialize)]
pub struct ResponseCodeCount {
    pub response_code: String,
    pub count: u64,
}

#[derive(Serialize)]
pub struct ClickHouseStats {
    pub total_queries: u64,
    pub cache_hit_rate_pct: f64,
    pub blocked_count: u64,
    pub avg_latency_us: f64,
    pub queries_last_hour: u64,
    pub queries_last_24h: u64,
    pub top_domains: Vec<DomainCount>,
    pub response_codes: Vec<ResponseCodeCount>,
}

#[derive(Serialize)]
pub struct AllStats {
    pub kafka: KafkaStats,
    pub clickhouse: ClickHouseStats,
}

// ── Client ────────────────────────────────────────────────────────────────────

pub struct StatsClient {
    kafka_brokers: String,
    ch: Client,
}

impl StatsClient {
    pub fn new(brokers: &str, ch_url: &str, ch_user: &str, ch_password: &str) -> Self {
        let ch = Client::default()
            .with_url(ch_url)
            .with_user(ch_user)
            .with_password(ch_password);
        Self {
            kafka_brokers: brokers.to_string(),
            ch,
        }
    }

    pub async fn get_stats(&self) -> AllStats {
        let (kafka, clickhouse) =
            tokio::join!(self.kafka_stats(), self.clickhouse_stats());
        AllStats { kafka, clickhouse }
    }

    async fn kafka_stats(&self) -> KafkaStats {
        let brokers = self.kafka_brokers.clone();
        tokio::task::spawn_blocking(move || fetch_kafka_stats_sync(&brokers))
            .await
            .unwrap_or_else(|_| KafkaStats {
                topic: TOPIC.to_string(),
                partitions: 0,
                total_messages: -1,
                consumer_lag: -1,
            })
    }

    async fn clickhouse_stats(&self) -> ClickHouseStats {
        let (total, hit_rate, blocked, avg_lat, last_hour, last_24h, domains, codes) =
            tokio::join!(
                u64_query(&self.ch, "SELECT count() AS value FROM dns_queries"),
                f64_query(
                    &self.ch,
                    "SELECT ifNull(toFloat64(round(
                        100.0 * countIf(cache_hit = 1) / nullIf(toFloat64(count()), 0)
                    , 2)), 0) AS value FROM dns_queries",
                ),
                u64_query(
                    &self.ch,
                    "SELECT countIf(blocked = 1) AS value FROM dns_queries",
                ),
                f64_query(
                    &self.ch,
                    "SELECT ifNull(toFloat64(round(avg(latency_us), 2)), 0) AS value
                     FROM dns_queries",
                ),
                u64_query(
                    &self.ch,
                    "SELECT count() AS value FROM dns_queries
                     WHERE timestamp_ms > (toUnixTimestamp(now()) - 3600) * 1000",
                ),
                u64_query(
                    &self.ch,
                    "SELECT count() AS value FROM dns_queries
                     WHERE timestamp_ms > (toUnixTimestamp(now()) - 86400) * 1000",
                ),
                multi_query::<DomainRow>(
                    &self.ch,
                    "SELECT domain, count() AS count FROM dns_queries
                     GROUP BY domain ORDER BY count DESC LIMIT 10",
                ),
                multi_query::<RespCodeRow>(
                    &self.ch,
                    "SELECT response_code, count() AS count FROM dns_queries
                     GROUP BY response_code ORDER BY count DESC",
                ),
            );

        ClickHouseStats {
            total_queries: total,
            cache_hit_rate_pct: hit_rate,
            blocked_count: blocked,
            avg_latency_us: avg_lat,
            queries_last_hour: last_hour,
            queries_last_24h: last_24h,
            top_domains: domains
                .into_iter()
                .map(|r| DomainCount { domain: r.domain, count: r.count })
                .collect(),
            response_codes: codes
                .into_iter()
                .map(|r| ResponseCodeCount {
                    response_code: r.response_code,
                    count: r.count,
                })
                .collect(),
        }
    }
}

// ── Kafka sync helper (runs in spawn_blocking) ────────────────────────────────

fn fetch_kafka_stats_sync(brokers: &str) -> KafkaStats {
    let timeout = Duration::from_secs(5);

    let consumer: BaseConsumer = match ClientConfig::new()
        .set("bootstrap.servers", brokers)
        .set("group.id", "denis-analytics")
        .create()
    {
        Ok(c) => c,
        Err(_) => {
            return KafkaStats {
                topic: TOPIC.to_string(),
                partitions: 0,
                total_messages: -1,
                consumer_lag: -1,
            }
        }
    };

    let metadata = match consumer.fetch_metadata(Some(TOPIC), timeout) {
        Ok(m) => m,
        Err(_) => {
            return KafkaStats {
                topic: TOPIC.to_string(),
                partitions: 0,
                total_messages: -1,
                consumer_lag: -1,
            }
        }
    };

    let topic_meta = match metadata.topics().iter().find(|t| t.name() == TOPIC) {
        Some(t) => t,
        None => {
            return KafkaStats {
                topic: TOPIC.to_string(),
                partitions: 0,
                total_messages: 0,
                consumer_lag: 0,
            }
        }
    };

    let num_partitions = topic_meta.partitions().len() as i32;

    // Assign all partitions so committed() returns offsets for this group.
    let mut tpl = rdkafka::topic_partition_list::TopicPartitionList::new();
    for p in topic_meta.partitions() {
        tpl.add_partition(TOPIC, p.id());
    }
    let _ = consumer.assign(&tpl);

    let committed = consumer
        .committed(timeout)
        .unwrap_or_else(|_| rdkafka::topic_partition_list::TopicPartitionList::new());

    let mut total_messages = 0i64;
    let mut consumer_lag = 0i64;

    for p in topic_meta.partitions() {
        let pid = p.id();
        let Ok((low, high)) = consumer.fetch_watermarks(TOPIC, pid, timeout) else {
            continue;
        };

        total_messages += (high - low).max(0);

        let committed_offset = committed
            .elements()
            .iter()
            .find(|e| e.topic() == TOPIC && e.partition() == pid)
            .and_then(|e| e.offset().to_raw())
            .unwrap_or(low.max(0));

        consumer_lag += (high - committed_offset).max(0);
    }

    KafkaStats {
        topic: TOPIC.to_string(),
        partitions: num_partitions,
        total_messages,
        consumer_lag,
    }
}

// ── ClickHouse query helpers ──────────────────────────────────────────────────

async fn u64_query(ch: &Client, query: &str) -> u64 {
    ch.query(query)
        .fetch_one::<U64Row>()
        .await
        .map(|r| r.value)
        .unwrap_or(0)
}

async fn f64_query(ch: &Client, query: &str) -> f64 {
    ch.query(query)
        .fetch_one::<F64Row>()
        .await
        .map(|r| if r.value.is_finite() { r.value } else { 0.0 })
        .unwrap_or(0.0)
}

async fn multi_query<T>(ch: &Client, query: &str) -> Vec<T>
where
    T: Row + serde::de::DeserializeOwned,
{
    let Ok(mut cursor) = ch.query(query).fetch::<T>() else {
        return Vec::new();
    };
    let mut rows = Vec::new();
    while let Ok(Some(row)) = cursor.next().await {
        rows.push(row);
    }
    rows
}
