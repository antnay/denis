use ahash::RandomState;
use arc_swap::ArcSwap;
use core::fmt::Write;
use ftlog::debug;
use redis::RedisError;
use serde::Serialize;
use sqlx::{PgPool, Row, postgres::PgRow};
use std::{
    collections::HashSet,
    sync::{Arc, Mutex},
    time::Duration,
};
use tokio_stream::StreamExt;

use crate::handler::Query;

type DomainSet = HashSet<String, RandomState>;

#[derive(thiserror::Error, Debug)]
pub enum CacheError {
    #[error("cache redis: {0}")]
    Redis(RedisError),
    #[error("cache sql: {0}")]
    Sql(sqlx::Error),
}

impl From<sqlx::Error> for CacheError {
    fn from(err: sqlx::Error) -> Self {
        CacheError::Sql(err)
    }
}

impl From<redis::RedisError> for CacheError {
    fn from(err: redis::RedisError) -> Self {
        CacheError::Redis(err)
    }
}

#[derive(Serialize)]
pub struct CacheStats {
    pub block_list_size: usize,
    pub allow_list_size: usize,
    pub l1_entry_count: u64,
}

#[derive(Debug, Clone, Copy)]
pub struct AddOutcome {
    pub added: usize,
    pub considered: usize,
}

// SQL max insertion per transaction
const INSERT_CHUNK: usize = 5_000;

pub struct Cache {
    l1: moka::future::Cache<String, Vec<u8>>,
    allow_list: ArcSwap<DomainSet>,
    block_list: ArcSwap<DomainSet>,
    write_lock: Mutex<()>,
    rds_conn: redis::aio::MultiplexedConnection,
    pg_pool: PgPool,
}

impl Cache {
    pub fn new(
        rds_conn: redis::aio::MultiplexedConnection,
        pg_pool: PgPool,
        l1_capacity: u64,
    ) -> Self {
        Self {
            l1: moka::future::Cache::builder()
                .max_capacity(l1_capacity)
                .time_to_live(Duration::from_mins(1))
                .build(),
            allow_list: ArcSwap::from_pointee(DomainSet::default()),
            block_list: ArcSwap::from_pointee(DomainSet::default()),
            write_lock: Mutex::new(()),
            rds_conn,
            pg_pool,
        }
    }

    pub fn pool(&self) -> &PgPool {
        &self.pg_pool
    }

    pub async fn add_dns_query_redis(&self, query: &Query, response: &[u8], ttl: u32) {
        let mut buf: heapless::String<128> = heapless::String::new();
        self.query_key(&mut buf, query);
        let key = buf.to_string();

        self.l1.insert(key.clone(), response.to_vec()).await;

        let mut conn = self.rds_conn.clone();
        let _ = redis::cmd("SETEX")
            .arg(&key)
            .arg(ttl)
            .arg(response)
            .query_async::<()>(&mut conn)
            .await;
    }

    #[inline]
    pub async fn add_dns_query_moka(&self, query: &Query, response: &[u8]) {
        let mut buf: heapless::String<128> = heapless::String::new();
        self.query_key(&mut buf, query);
        let key = buf.to_string();

        self.l1.insert(key, response.to_vec()).await;
    }

    #[inline]
    pub fn is_blocked(&self, name: &str) -> bool {
        let block = self.block_list.load();
        let allow = self.allow_list.load();
        decide(&block, &allow, name)
    }

    pub async fn check_and_get(
        &self,
        query: &Query,
    ) -> Result<(bool, Option<Vec<u8>>), CacheError> {
        if self.is_blocked(&query.name) {
            if cfg!(debug_assertions) {
                debug!("in memory block");
            }
            return Ok((true, None));
        }

        let mut buf: heapless::String<128> = heapless::String::new();
        self.query_key(&mut buf, query);
        let key = buf.to_string();

        if let Some(res) = self.l1.get(&key).await {
            return Ok((false, Some(res)));
        }

        let mut conn = self.rds_conn.clone();
        let res = redis::cmd("GET")
            .arg(&key)
            .query_async::<Option<Vec<u8>>>(&mut conn)
            .await?;
        Ok((false, res))
    }

    #[inline]
    fn query_key(&self, buf: &mut heapless::String<128>, query: &Query) {
        let _ = write!(buf, "dns:{}:{}", query.name, query.query_type);
    }

    pub async fn add_allow_domain(&self, domain: &str) -> Result<AddOutcome, CacheError> {
        self.add_allow_domains(std::slice::from_ref(&domain.to_string()))
            .await
    }

    pub async fn add_block_domain(
        &self,
        list: &str,
        domain: &str,
    ) -> Result<AddOutcome, CacheError> {
        self.add_block_domains(list, std::slice::from_ref(&domain.to_string()))
            .await
    }

    pub async fn add_block_domains(
        &self,
        list: &str,
        domains: &[String],
    ) -> Result<AddOutcome, CacheError> {
        let valid = normalize_domains(domains);
        if valid.is_empty() {
            return Ok(AddOutcome {
                added: 0,
                considered: 0,
            });
        }

        self.cow_insert(&self.block_list, &valid);

        let mut added = 0usize;
        for chunk in valid.chunks(INSERT_CHUNK) {
            let res = sqlx::query(
                "INSERT INTO blocked (list, domain)
                 SELECT $1, * FROM UNNEST($2::text[])
                 ON CONFLICT (domain) DO NOTHING",
            )
            .bind(list)
            .bind(chunk)
            .execute(&self.pg_pool)
            .await
            .map_err(CacheError::Sql)?;
            added += res.rows_affected() as usize;
        }

        Ok(AddOutcome {
            added,
            considered: valid.len(),
        })
    }

    /// Bulk-add allowed domains (allowlist overrides the blocklist in the DNS
    /// path). Same semantics as [`Cache::add_block_domains`].
    pub async fn add_allow_domains(&self, domains: &[String]) -> Result<AddOutcome, CacheError> {
        let valid = normalize_domains(domains);
        if valid.is_empty() {
            return Ok(AddOutcome {
                added: 0,
                considered: 0,
            });
        }

        self.cow_insert(&self.allow_list, &valid);

        let mut added = 0usize;
        for chunk in valid.chunks(INSERT_CHUNK) {
            let res = sqlx::query(
                "INSERT INTO allowed (domain)
                 SELECT * FROM UNNEST($1::text[])
                 ON CONFLICT (domain) DO NOTHING",
            )
            .bind(chunk)
            .execute(&self.pg_pool)
            .await
            .map_err(CacheError::Sql)?;
            added += res.rows_affected() as usize;
        }

        Ok(AddOutcome {
            added,
            considered: valid.len(),
        })
    }

    pub async fn remove_block_domain(&self, domain: &str) -> Result<bool, CacheError> {
        let lower = domain.to_lowercase();
        let removed = {
            let _w = self.write_lock.lock().unwrap();
            let mut new: DomainSet = (**self.block_list.load()).clone();
            let removed = new.remove(&lower);
            self.block_list.store(Arc::new(new));
            removed
        };

        sqlx::query("DELETE FROM blocked WHERE domain = $1")
            .bind(&lower)
            .execute(&self.pg_pool)
            .await
            .map_err(CacheError::Sql)?;

        Ok(removed)
    }

    #[inline]
    fn cow_insert(&self, target: &ArcSwap<DomainSet>, domains: &[String]) {
        let _w = self.write_lock.lock().unwrap();
        let mut new: DomainSet = (**target.load()).clone();
        for d in domains {
            new.insert(d.clone());
        }
        target.store(Arc::new(new));
    }

    pub async fn list_block_domains(&self) -> Result<Vec<String>, CacheError> {
        let domains = sqlx::query_scalar::<_, String>("SELECT domain FROM blocked ORDER BY domain")
            .fetch_all(&self.pg_pool)
            .await
            .map_err(CacheError::Sql)?;
        Ok(domains)
    }

    pub async fn list_allow_domains(&self) -> Result<Vec<String>, CacheError> {
        let domains = sqlx::query_scalar::<_, String>("SELECT domain FROM allowed ORDER BY domain")
            .fetch_all(&self.pg_pool)
            .await
            .map_err(CacheError::Sql)?;
        Ok(domains)
    }

    pub async fn stats(&self) -> CacheStats {
        CacheStats {
            block_list_size: self.block_list.load().len(),
            allow_list_size: self.allow_list.load().len(),
            l1_entry_count: self.l1.entry_count(),
        }
    }

    pub async fn read_blocklist_db_memory(&self) {
        let set = load_set(&self.pg_pool, "blocked").await;
        self.block_list.store(Arc::new(set));
    }

    pub async fn read_allowlist_db_memory(&self) {
        let set = load_set(&self.pg_pool, "allowed").await;
        self.allow_list.store(Arc::new(set));
    }
}

async fn load_set(pool: &PgPool, table: &str) -> DomainSet {
    let mut local = DomainSet::default();
    let query = format!("SELECT domain FROM {table}");
    let mut stream = sqlx::query(&query)
        .map(|row: PgRow| row.try_get::<String, _>("domain").ok())
        .fetch(pool);

    while let Some(Ok(Some(domain))) = stream.next().await {
        local.insert(domain);
    }
    local
}

/// Decide whether domain is blocked
#[inline]
fn decide(block: &DomainSet, allow: &DomainSet, name_lc: &str) -> bool {
    if block.is_empty() {
        return false;
    }
    if allow.is_empty() {
        return any_suffix_match(block, name_lc);
    }

    if allow.contains(name_lc) {
        return false;
    }
    let mut blocked = block.contains(name_lc);
    for p in memchr::memchr_iter(b'.', name_lc.as_bytes()) {
        let suffix = &name_lc[p + 1..];
        if allow.contains(suffix) {
            return false;
        }
        if !blocked && block.contains(suffix) {
            blocked = true;
        }
    }
    blocked
}

#[inline]
fn any_suffix_match(set: &DomainSet, name: &str) -> bool {
    if set.is_empty() {
        return false;
    }
    if set.contains(name) {
        return true;
    }
    for p in memchr::memchr_iter(b'.', name.as_bytes()) {
        if set.contains(&name[p + 1..]) {
            return true;
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    fn set(items: &[&str]) -> DomainSet {
        items.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn suffix_matches_subdomains_but_not_partial_labels() {
        let s = set(&["doubleclick.net", "example.com"]);
        assert!(any_suffix_match(&s, "doubleclick.net")); // exact
        assert!(any_suffix_match(&s, "ad.doubleclick.net")); // subdomain
        assert!(any_suffix_match(&s, "a.b.c.example.com")); // deep subdomain
        assert!(!any_suffix_match(&s, "notdoubleclick.net")); // partial label
        assert!(!any_suffix_match(&s, "example.com.evil.org")); // suffix elsewhere
        assert!(!any_suffix_match(&s, "net")); // parent TLD not blocked
        assert!(!any_suffix_match(&set(&[]), "anything.com")); // empty set
    }

    #[test]
    fn allowlist_overrides_blocklist() {
        let block = set(&["example.com"]);
        let allow = set(&["good.example.com"]);
        assert!(decide(&block, &allow, "ads.example.com"));
        assert!(!decide(&block, &allow, "good.example.com")); // allowed exact
        assert!(!decide(&block, &allow, "x.good.example.com")); // allowed subtree
    }

    #[test]
    fn validation_and_parsing() {
        assert!(is_valid_domain("example.com"));
        assert!(!is_valid_domain("nodot"));
        assert!(!is_valid_domain("bad_.com.")); // trailing dot -> empty label
        assert!(!is_valid_domain(""));

        let parsed = parse_domain_list(
            "# comment\n0.0.0.0 ads.com\n127.0.0.1 tracker.net evil.org\nplain.io\n\nbad\n",
        );
        assert_eq!(
            parsed,
            vec!["ads.com", "tracker.net", "evil.org", "plain.io"]
        );
    }
}

fn normalize_domains(domains: &[String]) -> Vec<String> {
    domains
        .iter()
        .map(|d| d.trim().trim_end_matches('.').to_lowercase())
        .filter(|d| is_valid_domain(d))
        .collect()
}

pub fn is_valid_domain(d: &str) -> bool {
    if d.is_empty() || d.len() > 253 || !d.contains('.') {
        return false;
    }
    d.split('.').all(|label| {
        !label.is_empty()
            && label.len() <= 63
            && label
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
    })
}

pub fn parse_domain_list(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    for line in text.lines() {
        let line = line.split('#').next().unwrap_or("").trim();
        if line.is_empty() {
            continue;
        }
        let mut tokens = line.split_whitespace();
        let Some(first) = tokens.next() else { continue };

        if first.parse::<std::net::IpAddr>().is_ok() {
            for d in tokens {
                let d = d.trim_end_matches('.').to_lowercase();
                if is_valid_domain(&d) {
                    out.push(d);
                }
            }
        } else {
            let d = first.trim_end_matches('.').to_lowercase();
            if is_valid_domain(&d) {
                out.push(d);
            }
        }
    }
    out
}
