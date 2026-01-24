# Denis DNS Proxy

Denis is a high-performance asynchronous DNS proxy written in Rust designed for analytics, caching, and blocklist enforcement.

## Features

- **Asynchronous DNS handling** using Tokio for high concurrency.
- **Caching** of DNS responses to reduce upstream queries, with NXDOMAIN synthesis for blocked domains.
- **Blocklist enforcement** for ads, trackers, and malicious domains.
- **Real-time analytics** using Kafka for query streaming and ClickHouse for storage and analysis.
- **Observability** via Prometheus metrics including query throughput, latency percentiles, cache effectiveness, and upstream health.

## Technologies

- Rust, Tokio
- DragonflyDB (for in-memory caching)
- Kafka & ClickHouse (for analytics)
- Prometheus & Grafana (monitoring)
- Docker for containerized deployment

