use std::net::SocketAddr;

use clap::{Args, Parser, ValueEnum};

#[derive(ValueEnum, Clone, Copy, Debug, PartialEq, Eq)]
pub enum Runtime {
    Monoio,
    Tokio,
}

/// Denis DNS proxy.
///
/// Flags below are *startup* configuration: things that require a restart to
/// change (bind addresses, backend connections, socket tuning). Runtime-tunable
/// settings (upstreams, blocking mode, cache TTL clamps) live in [`RuntimeConfig`]
/// and are mutated through the management API — see `crate::config`.
#[derive(Parser, Debug)]
#[command(author, version, about)]
pub struct Cli {
    #[command(flatten)]
    pub dns: DnsArgs,
    #[command(flatten)]
    pub api: ApiArgs,
    #[command(flatten)]
    pub upstream: UpstreamArgs,
    #[command(flatten)]
    pub redis: RedisArgs,
    #[command(flatten)]
    pub postgres: PostgresArgs,
    #[command(flatten)]
    pub kafka: KafkaArgs,
    #[command(flatten)]
    pub clickhouse: ClickhouseArgs,
    #[command(flatten)]
    pub analytics: AnalyticsArgs,
    #[command(flatten)]
    pub auth: AuthArgs,

    /// Increase log verbosity: -v = debug, -vv = trace.
    #[arg(short, long, action = clap::ArgAction::Count, global = true)]
    pub verbose: u8,
}

#[derive(Args, Debug)]
#[command(next_help_heading = "Management API auth")]
pub struct AuthArgs {
    /// Admin username for POST /auth/login. Auth is enforced once this and
    /// --admin-password (or --api-token) are set; otherwise the API is open.
    #[arg(long, env = "DENIS_ADMIN_USER")]
    pub admin_user: Option<String>,

    /// Plaintext admin password; argon2-hashed at startup.
    #[arg(long, env = "DENIS_ADMIN_PASSWORD")]
    pub admin_password: Option<String>,

    /// Pre-computed argon2 PHC hash (preferred over --admin-password). Generate
    /// one with `denis --hash-password <PW>`.
    #[arg(long, env = "DENIS_ADMIN_PASSWORD_HASH")]
    pub admin_password_hash: Option<String>,

    /// Static long-lived bearer token (alternative to login).
    #[arg(long, env = "DENIS_API_TOKEN")]
    pub api_token: Option<String>,

    /// Lifetime of a login-issued session token, in seconds.
    #[arg(long, env = "DENIS_AUTH_TTL_SECS", default_value_t = 86_400)]
    pub auth_ttl_secs: u64,

    /// Print an argon2 hash of the given password and exit.
    #[arg(long)]
    pub hash_password: Option<String>,
}

#[derive(Args, Debug)]
#[command(next_help_heading = "Analytics")]
pub struct AnalyticsArgs {
    /// ClickHouse analytics sink (Kafka producer + consumer + /stats).
    /// Only effective if built with the `analytics` feature.
    #[arg(
        long = "clickhouse",
        env = "DENIS_CLICKHOUSE",
        default_value_t = true,
        action = clap::ArgAction::Set
    )]
    pub clickhouse: bool,

    /// Prometheus metrics sink (/metrics endpoint for Grafana).
    #[arg(
        long = "prometheus",
        env = "DENIS_PROMETHEUS",
        default_value_t = true,
        action = clap::ArgAction::Set
    )]
    pub prometheus: bool,
}

#[derive(Args, Debug)]
#[command(next_help_heading = "DNS server")]
pub struct DnsArgs {
    /// Address the UDP + TCP DNS listener binds to.
    #[arg(long, env = "DENIS_DNS_BIND", default_value = "0.0.0.0:53")]
    pub dns_bind: SocketAddr,

    /// Datapath runtime: monoio (thread-per-core, io_uring) or tokio.
    #[arg(long, env = "DENIS_RUNTIME", value_enum, default_value_t = Runtime::Monoio)]
    pub runtime: Runtime,

    /// Number of pinned monoio datapath workers (default: cores - tokio-workers).
    #[arg(long, env = "DENIS_WORKERS")]
    pub workers: Option<usize>,

    /// Tokio control-plane worker threads (API, cold path, analytics).
    #[arg(long, env = "DENIS_TOKIO_WORKERS", default_value_t = 2)]
    pub tokio_workers: usize,
}

#[derive(Args, Debug)]
#[command(next_help_heading = "Management API")]
pub struct ApiArgs {
    /// Address the HTTP management/stats API binds to.
    #[arg(long, env = "DENIS_API_BIND", default_value = "0.0.0.0:8080")]
    pub api_bind: SocketAddr,
}

#[derive(Args, Debug)]
#[command(next_help_heading = "Upstream resolvers (initial defaults; tunable via API)")]
pub struct UpstreamArgs {
    /// Upstream resolvers, comma-separated or repeated. These seed the runtime
    /// config the first time Denis starts; afterwards the persisted value wins.
    #[arg(
        long = "upstream",
        env = "DENIS_UPSTREAMS",
        value_delimiter = ',',
        default_value = "9.9.9.9:53,1.1.1.1:53"
    )]
    pub servers: Vec<SocketAddr>,

    /// Per-query upstream timeout in milliseconds.
    #[arg(long, env = "DENIS_UPSTREAM_TIMEOUT_MS", default_value_t = 5000)]
    pub timeout_ms: u64,
}

#[derive(Args, Debug)]
#[command(next_help_heading = "Redis / cache")]
pub struct RedisArgs {
    #[arg(long, env = "REDIS_URL", default_value = "redis://localhost:6379")]
    pub redis_url: String,

    /// Maximum number of entries in the in-process (L1) cache.
    #[arg(long, env = "DENIS_CACHE_CAPACITY", default_value_t = 10_000)]
    pub cache_capacity: u64,
}

#[derive(Args, Debug)]
#[command(next_help_heading = "PostgreSQL")]
pub struct PostgresArgs {
    #[arg(
        long,
        env = "DATABASE_URL",
        default_value = "postgres://postgres:postgres@localhost:5433/denis"
    )]
    pub database_url: String,

    #[arg(long, env = "DENIS_PG_MAX_CONNECTIONS", default_value_t = 16)]
    pub pg_max_connections: u32,
}

#[derive(Args, Debug)]
#[command(next_help_heading = "Kafka")]
pub struct KafkaArgs {
    #[arg(long, env = "DENIS_KAFKA_BROKERS", default_value = "localhost:9092")]
    pub kafka_brokers: String,
}

#[derive(Args, Debug)]
#[command(next_help_heading = "ClickHouse")]
pub struct ClickhouseArgs {
    #[arg(
        long,
        env = "DENIS_CLICKHOUSE_URL",
        default_value = "http://localhost:8123"
    )]
    pub clickhouse_url: String,
    #[arg(long, env = "DENIS_CLICKHOUSE_USER", default_value = "default")]
    pub clickhouse_user: String,
    #[arg(long, env = "DENIS_CLICKHOUSE_PASSWORD", default_value = "clickhouse")]
    pub clickhouse_password: String,
}
