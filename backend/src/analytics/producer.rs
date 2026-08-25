use std::sync::Arc;

use serde::{Deserialize, Serialize};
use tokio::sync::mpsc;

use super::metrics::Metrics;

#[cfg(feature = "analytics")]
pub const TOPIC: &str = "dns-queries";

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct DnsQueryEvent {
    pub timestamp_ms: u64,
    pub domain: String,
    pub query_type: u16,
    pub response_code: u16,
    pub cache_hit: bool,
    pub blocked: bool,
    pub latency_us: u64,
}

pub struct AnalyticsProducer {
    tx: Option<mpsc::Sender<DnsQueryEvent>>,
}

impl AnalyticsProducer {
    pub fn disabled() -> Self {
        Self { tx: None }
    }

    pub fn spawn(metrics: Option<Arc<Metrics>>, kafka_brokers: Option<String>) -> Self {
        if metrics.is_none() && kafka_brokers.is_none() {
            return Self::disabled();
        }

        let (tx, mut rx) = mpsc::channel::<DnsQueryEvent>(10_000);

        #[cfg(feature = "analytics")]
        let kafka = kafka_brokers.as_deref().map(build_kafka);
        #[cfg(not(feature = "analytics"))]
        let _ = kafka_brokers;

        tokio::spawn(async move {
            while let Some(event) = rx.recv().await {
                if let Some(m) = &metrics {
                    m.record(&event);
                }
                #[cfg(feature = "analytics")]
                if let Some(p) = &kafka {
                    send_kafka(p, &event).await;
                }
            }
        });

        Self { tx: Some(tx) }
    }

    pub fn send(&self, event: DnsQueryEvent) {
        if let Some(tx) = &self.tx {
            let _ = tx.try_send(event);
        }
    }
}

#[cfg(feature = "analytics")]
fn build_kafka(brokers: &str) -> rdkafka::producer::FutureProducer {
    rdkafka::config::ClientConfig::new()
        .set("bootstrap.servers", brokers)
        .set("message.timeout.ms", "5000")
        .set("queue.buffering.max.ms", "100")
        .create()
        .expect("Failed to create Kafka producer")
}

#[cfg(feature = "analytics")]
async fn send_kafka(producer: &rdkafka::producer::FutureProducer, event: &DnsQueryEvent) {
    use rdkafka::producer::FutureRecord;
    let Ok(payload) = serde_json::to_string(event) else {
        return;
    };
    let record = FutureRecord::to(TOPIC)
        .key(event.domain.as_str())
        .payload(payload.as_str());
    let _ = producer
        .send(record, std::time::Duration::from_secs(0))
        .await;
}
