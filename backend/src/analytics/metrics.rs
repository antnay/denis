use std::sync::Arc;

use prometheus::{
    Encoder, Histogram, HistogramOpts, IntCounterVec, IntGauge, Opts, Registry, TextEncoder,
    exponential_buckets,
};

use super::DnsQueryEvent;

pub struct Metrics {
    registry: Registry,
    queries: IntCounterVec,
    latency: Histogram,
    block_size: IntGauge,
    allow_size: IntGauge,
    l1_entries: IntGauge,
}

impl Metrics {
    pub fn new() -> Arc<Self> {
        let registry = Registry::new();
        let queries = IntCounterVec::new(
            Opts::new("denis_queries_total", "DNS queries by outcome"),
            &["outcome"],
        )
        .unwrap();
        let latency = Histogram::with_opts(
            HistogramOpts::new("denis_query_latency_seconds", "Query latency in seconds")
                .buckets(exponential_buckets(0.000_05, 2.0, 16).unwrap()),
        )
        .unwrap();
        let block_size = IntGauge::new("denis_blocklist_size", "Blocklist domain count").unwrap();
        let allow_size = IntGauge::new("denis_allowlist_size", "Allowlist domain count").unwrap();
        let l1_entries = IntGauge::new("denis_l1_cache_entries", "L1 cache entry count").unwrap();

        registry.register(Box::new(queries.clone())).unwrap();
        registry.register(Box::new(latency.clone())).unwrap();
        registry.register(Box::new(block_size.clone())).unwrap();
        registry.register(Box::new(allow_size.clone())).unwrap();
        registry.register(Box::new(l1_entries.clone())).unwrap();

        Arc::new(Self {
            registry,
            queries,
            latency,
            block_size,
            allow_size,
            l1_entries,
        })
    }

    pub fn record(&self, event: &DnsQueryEvent) {
        let outcome = if event.blocked {
            "blocked"
        } else if event.cache_hit {
            "hit"
        } else {
            "miss"
        };
        self.queries.with_label_values(&[outcome]).inc();
        self.latency.observe(event.latency_us as f64 / 1_000_000.0);
    }

    pub fn render(&self, block_size: usize, allow_size: usize, l1_entries: u64) -> String {
        self.block_size.set(block_size as i64);
        self.allow_size.set(allow_size as i64);
        self.l1_entries.set(l1_entries as i64);

        let mut buf = Vec::new();
        let encoder = TextEncoder::new();
        encoder.encode(&self.registry.gather(), &mut buf).ok();
        String::from_utf8(buf).unwrap_or_default()
    }
}
