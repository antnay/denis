pub mod metrics;
mod producer;

#[cfg(feature = "analytics")]
mod consumer;
#[cfg(feature = "analytics")]
mod stats;

pub use metrics::Metrics;
pub use producer::{AnalyticsProducer, DnsQueryEvent};

#[cfg(feature = "analytics")]
pub use consumer::AnalyticsConsumer;
#[cfg(feature = "analytics")]
pub use stats::StatsClient;
