use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr};
use std::sync::Arc;
use std::time::Duration;

use arc_swap::ArcSwap;
use ftlog::info;
use serde::{Deserialize, Serialize};

use crate::cli::Cli;
use crate::db::{DbError, DbPool, QueryBuilder};

pub type SharedConfig = Arc<ArcSwap<RuntimeConfig>>;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BlockingMode {
    NxDomain,
    Refused,
    ZeroIp,
}

impl Default for BlockingMode {
    fn default() -> Self {
        Self::NxDomain
    }
}

fn default_neg_ttl() -> u32 {
    60
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RuntimeConfig {
    pub upstreams: Vec<SocketAddr>,
    pub upstream_timeout_ms: u64,
    pub blocking_mode: BlockingMode,
    pub cache_min_ttl: u32,
    pub cache_max_ttl: u32,
    #[serde(default = "default_neg_ttl")]
    pub neg_ttl: u32,
}

impl RuntimeConfig {
    pub fn from_cli(cli: &Cli) -> Self {
        Self {
            upstreams: cli.upstream.servers.clone(),
            upstream_timeout_ms: cli.upstream.timeout_ms,
            blocking_mode: BlockingMode::default(),
            cache_min_ttl: 0,
            cache_max_ttl: 86_400,
            neg_ttl: default_neg_ttl(),
        }
    }

    pub fn upstream_timeout(&self) -> Duration {
        Duration::from_millis(self.upstream_timeout_ms)
    }

    pub fn clamp_ttl(&self, ttl: u32) -> u32 {
        ttl.clamp(
            self.cache_min_ttl,
            self.cache_max_ttl.max(self.cache_min_ttl),
        )
    }

    pub fn zero_ip(qtype: hickory_proto::rr::RecordType) -> Option<IpAddr> {
        use hickory_proto::rr::RecordType;
        match qtype {
            RecordType::A => Some(IpAddr::V4(Ipv4Addr::UNSPECIFIED)),
            RecordType::AAAA => Some(IpAddr::V6(Ipv6Addr::UNSPECIFIED)),
            _ => None,
        }
    }
}

#[derive(Debug, Default, Deserialize)]
pub struct RuntimeConfigPatch {
    pub upstreams: Option<Vec<SocketAddr>>,
    pub upstream_timeout_ms: Option<u64>,
    pub blocking_mode: Option<BlockingMode>,
    pub cache_min_ttl: Option<u32>,
    pub cache_max_ttl: Option<u32>,
    pub neg_ttl: Option<u32>,
}

impl RuntimeConfigPatch {
    pub fn apply_to(self, base: &RuntimeConfig) -> RuntimeConfig {
        RuntimeConfig {
            upstreams: self.upstreams.unwrap_or_else(|| base.upstreams.clone()),
            upstream_timeout_ms: self.upstream_timeout_ms.unwrap_or(base.upstream_timeout_ms),
            blocking_mode: self.blocking_mode.unwrap_or(base.blocking_mode),
            cache_min_ttl: self.cache_min_ttl.unwrap_or(base.cache_min_ttl),
            cache_max_ttl: self.cache_max_ttl.unwrap_or(base.cache_max_ttl),
            neg_ttl: self.neg_ttl.unwrap_or(base.neg_ttl),
        }
    }
}

pub async fn ensure_table(pool: &DbPool) -> Result<(), DbError> {
    sqlx::query(
        "CREATE TABLE IF NOT EXISTS settings (
            id     INTEGER PRIMARY KEY,
            config TEXT NOT NULL
        )",
    )
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn load_or_init(
    pool: &DbPool,
    defaults: RuntimeConfig,
) -> Result<SharedConfig, DbError> {
    ensure_table(pool).await?;

    let existing: Option<String> =
        sqlx::query_scalar("SELECT config FROM settings WHERE id = 1")
            .fetch_optional(pool)
            .await?;

    let config = match existing.and_then(|s| serde_json::from_str::<RuntimeConfig>(&s).ok()) {
        Some(cfg) => {
            info!("loaded runtime config from database");
            cfg
        }
        None => {
            info!("no persisted runtime config; seeding from CLI defaults");
            persist(pool, &defaults).await?;
            defaults
        }
    };

    Ok(Arc::new(ArcSwap::from_pointee(config)))
}

pub async fn persist(pool: &DbPool, config: &RuntimeConfig) -> Result<(), DbError> {
    let json = serde_json::to_string(config).expect("RuntimeConfig serializes");
    QueryBuilder::new("INSERT INTO settings (id, config) VALUES (1, ")
        .push_bind(json)
        .push(") ON CONFLICT (id) DO UPDATE SET config = excluded.config")
        .build()
        .execute(pool)
        .await?;
    Ok(())
}
