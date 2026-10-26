mod acl;
mod analytics;
mod api;
mod cli;
mod config;
mod db;
mod dns;
mod handler;
mod repo;

use clap::Parser;
use ftlog::{LevelFilter, error, info};
use std::sync::Arc;

use crate::{
    analytics::{AnalyticsProducer, Metrics},
    api::ApiState,
    cli::{Cli, Runtime},
    config::RuntimeConfig,
    dns::Server,
    handler::{QueryHandler, UpstreamPool},
    repo::Cache,
};

#[cfg(feature = "analytics")]
use crate::analytics::{AnalyticsConsumer, StatsClient};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cli = Cli::parse();

    let level = match cli.verbose {
        0 => LevelFilter::Info,
        1 => LevelFilter::Debug,
        _ => LevelFilter::Trace,
    };
    let _guard = ftlog::builder()
        .max_log_level(level)
        .try_init()
        .expect("failed to init logger");

    let cores = core_affinity::get_core_ids();
    let total = cores.as_ref().map(|c| c.len()).unwrap_or_else(|| {
        std::thread::available_parallelism()
            .map(|n| n.get())
            .unwrap_or(1)
    });
    let tokio_workers = cli.dns.other_workers.max(1);
    let datapath_workers = cli
        .dns
        .dns_workers
        .unwrap_or_else(|| total.saturating_sub(tokio_workers).max(1))
        .max(1);

    let pin_at = |i: usize| cores.as_ref().map(|c| c[i % c.len()]);
    let dp_pins: Vec<Option<core_affinity::CoreId>> = (0..datapath_workers).map(pin_at).collect();

    let rt = match cli.dns.runtime {
        Runtime::Monoio => {
            let tk_pins: Vec<Option<core_affinity::CoreId>> = (0..tokio_workers)
                .map(|i| pin_at(datapath_workers + i))
                .collect();
            let next = Arc::new(std::sync::atomic::AtomicUsize::new(0));
            tokio::runtime::Builder::new_multi_thread()
                .worker_threads(tokio_workers)
                .enable_all()
                .on_thread_start(move || {
                    let i = next.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                    if let Some(core) = tk_pins.get(i % tk_pins.len()).copied().flatten() {
                        core_affinity::set_for_current(core);
                    }
                })
                .build()?
        }
        Runtime::Tokio => tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()?,
    };

    rt.block_on(run(cli, dp_pins))
}

async fn run(
    cli: Cli,
    dp_pins: Vec<Option<core_affinity::CoreId>>,
) -> Result<(), Box<dyn std::error::Error>> {
    let pool = db::connect(&cli.database.database_url, cli.database.max_connections)
        .await
        .expect("cannot connect to database");

    repo::ensure_tables(&pool)
        .await
        .expect("failed to create blocklist/allowlist tables");
    let acl = acl::Acl::load(&pool)
        .await
        .expect("failed to load access control list");

    #[cfg(feature = "redis")]
    let rds_conn = redis::Client::open(cli.redis.redis_url.clone())
        .expect("cannot open redis client")
        .get_multiplexed_async_connection()
        .await?;

    let runtime_config = config::load_or_init(&pool, RuntimeConfig::from_cli(&cli))
        .await
        .expect("failed to load runtime config");

    let metrics = cli.analytics.prometheus.then(Metrics::new);
    let clickhouse_on = cli.analytics.clickhouse && cfg!(feature = "analytics");
    if cli.analytics.clickhouse && !cfg!(feature = "analytics") {
        info!("--clickhouse requested but built without the `analytics` feature; ignoring");
    }
    info!(
        "sinks: prometheus={} clickhouse={}",
        cli.analytics.prometheus, clickhouse_on
    );

    let kafka_brokers = clickhouse_on.then(|| cli.kafka.kafka_brokers.clone());
    let analytics_producer = Arc::new(AnalyticsProducer::spawn(metrics.clone(), kafka_brokers));

    #[cfg(feature = "analytics")]
    if clickhouse_on {
        let analytics_consumer = AnalyticsConsumer::new(
            &cli.kafka.kafka_brokers,
            "denis-analytics",
            &cli.clickhouse.clickhouse_url,
            &cli.clickhouse.clickhouse_user,
            &cli.clickhouse.clickhouse_password,
        );
        tokio::spawn(analytics_consumer.run());
    }

    let cache = Arc::new(Cache::new(
        #[cfg(feature = "redis")]
        rds_conn,
        pool,
        cli.redis.cache_capacity,
    ));
    cache.read_blocklist_db_memory().await;
    cache.read_allowlist_db_memory().await;

    let upstream = UpstreamPool::new(runtime_config.clone()).await;
    let handler = Arc::new(QueryHandler::new(
        cache.clone(),
        upstream,
        analytics_producer.clone(),
        runtime_config.clone(),
    ));

    #[cfg(feature = "analytics")]
    let stats_client = clickhouse_on.then(|| {
        Arc::new(StatsClient::new(
            &cli.kafka.kafka_brokers,
            &cli.clickhouse.clickhouse_url,
            &cli.clickhouse.clickhouse_user,
            &cli.clickhouse.clickhouse_password,
        ))
    });
    let auth = api::Auth::new(cache.pool().clone())
        .await
        .expect("failed to init auth");
    let api_router = api::router(ApiState {
        cache: cache.clone(),
        #[cfg(feature = "analytics")]
        stats: stats_client,
        metrics: metrics.clone(),
        config: runtime_config.clone(),
        auth,
    });
    let api_bind = cli.api.api_bind;
    tokio::spawn(async move {
        let listener = tokio::net::TcpListener::bind(api_bind)
            .await
            .expect("Cannot bind API listener");
        info!("Management API listening on {}", api_bind);
        axum::serve(listener, api_router)
            .await
            .expect("API server error");
    });

    match cli.dns.runtime {
        Runtime::Tokio => {
            let server_config = dns::ServerConfig {
                bind_addr: cli.dns.dns_bind,
                udp_buffer_size: dns::UDP_BUFFER_SIZE,
                udp_buffer_count: dns::UDP_BUFFER_COUNT,
            };
            let dns_server = Server::new(server_config, handler);
            info!("Starting DNS server (tokio) on {}", cli.dns.dns_bind);
            if let Err(e) = dns_server.run().await {
                error!("Server error: {}", e);
                std::process::exit(1);
            }
        }
        Runtime::Monoio => {
            let workers = dp_pins.len();

            let (cold_tx, cold_rx) = flume::unbounded::<dns::mono::ColdRequest>();
            for _ in 0..workers {
                tokio::spawn(dns::mono::cold_path(cold_rx.clone(), handler.clone()));
            }

            dns::mono::spawn_workers(
                cli.dns.dns_bind,
                dp_pins,
                cache.clone(),
                runtime_config.clone(),
                analytics_producer.clone(),
                acl.clone(),
                cold_tx,
            );

            let tcp_handler = handler.clone();
            let tcp_bind = cli.dns.dns_bind;
            tokio::spawn(async move {
                if let Err(e) = dns::serve_tcp(tcp_bind, tcp_handler).await {
                    error!("TCP server error: {}", e);
                }
            });

            info!(
                "Starting DNS server on {} with {} pinned workers + {} tokio",
                cli.dns.dns_bind, workers, cli.dns.other_workers
            );
            std::future::pending::<()>().await;
        }
    }

    Ok(())
}
