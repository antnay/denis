use rdkafka::{
    config::ClientConfig,
    producer::{FutureProducer, FutureRecord},
};
use serde::{Deserialize, Serialize};
use std::time::Duration;
use tokio::sync::mpsc;

pub const TOPIC: &str = "dns-queries";

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct DnsQueryEvent {
    pub timestamp_ms: u64,
    pub domain: String,
    pub query_type: String,
    pub response_code: String,
    pub cache_hit: bool,
    pub blocked: bool,
    pub latency_us: u64,
}

pub struct AnalyticsProducer {
    tx: mpsc::Sender<DnsQueryEvent>,
}

impl AnalyticsProducer {
    pub fn new(brokers: &str) -> Self {
        let producer: FutureProducer = ClientConfig::new()
            .set("bootstrap.servers", brokers)
            .set("message.timeout.ms", "5000")
            .set("queue.buffering.max.ms", "100")
            .create()
            .expect("Failed to create Kafka producer");

        let (tx, mut rx) = mpsc::channel::<DnsQueryEvent>(10_000);

        tokio::spawn(async move {
            while let Some(event) = rx.recv().await {
                let payload = match serde_json::to_string(&event) {
                    Ok(p) => p,
                    Err(_) => continue,
                };
                let record = FutureRecord::to(TOPIC)
                    .key(event.domain.as_str())
                    .payload(payload.as_str());
                let _ = producer.send(record, Duration::from_secs(0)).await;
            }
        });

        Self { tx }
    }

    // Non-blocking: drops the event if the channel is full to never stall the DNS hot path.
    pub fn send(&self, event: DnsQueryEvent) {
        let _ = self.tx.try_send(event);
    }
}
