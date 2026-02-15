use core::fmt::Write;
use ftlog::{debug, error, info};
use redis::RedisError;
use sqlx::{PgPool, Row, postgres::PgRow};
use std::{collections::HashSet, sync::Arc, time::Duration};
use tokio::{sync::RwLock, time::Instant};
use tokio_stream::StreamExt;

use crate::handler::Query;

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

pub struct Cache {
    l1: moka::future::Cache<String, Vec<u8>>,
    allow_list: Arc<RwLock<HashSet<String>>>,
    block_list: Arc<RwLock<HashSet<String>>>,
    rds_conn: redis::aio::MultiplexedConnection,
    pg_pool: PgPool,
}

impl Cache {
    pub fn new(rds_conn: redis::aio::MultiplexedConnection, pg_pool: PgPool) -> Self {
        Self {
            l1: moka::future::Cache::builder()
                .max_capacity(10000)
                .time_to_live(Duration::from_mins(1))
                .build(),
            allow_list: Arc::new(RwLock::new(HashSet::new())),
            block_list: Arc::new(RwLock::new(HashSet::new())),
            rds_conn,
            pg_pool,
        }
    }

    // pub async fn get_query(&self, query: &Query) -> Result<Option<Vec<u8>>, CacheError> {
    //     let mut conn = self.pool.get().await?;
    //     let key = self.query_key(&query);
    //     let res = cmd("GET")
    //         .arg(&key)
    //         .query_async::<Option<Vec<u8>>>(&mut conn)
    //         .await
    //         .map_err(|e| CacheError::Get(e, key))?;
    //     Ok(res)
    // }
    //

    pub async fn add_dns_query_redis(&self, query: &Query, response: &[u8], ttl: u32) {
        let begin = Instant::now();
        let mut buf: heapless::String<128> = heapless::String::new();
        self.query_key(&mut buf, query);
        let key = buf.to_string();

        self.l1.insert(key.clone(), response.to_vec()).await;
        debug!("current cache: {:?}", self.l1);

        let mut conn = self.rds_conn.clone();
        let _ = redis::cmd("SETEX")
            .arg(&key)
            .arg(ttl)
            .arg(response)
            .query_async::<()>(&mut conn)
            .await;
        let delta = begin.elapsed();
        if cfg!(debug_assertions) {
            info!("add query time: {:?}", delta);
        }
    }

    #[inline]
    pub async fn add_dns_query_moka(&self, query: &Query, response: &[u8]) {
        let begin = Instant::now();
        let mut buf: heapless::String<128> = heapless::String::new();
        self.query_key(&mut buf, query);
        let key = buf.to_string();

        self.l1.insert(key.clone(), response.to_vec()).await;
        let delta = begin.elapsed();
        if cfg!(debug_assertions) {
            info!("add query moka time: {:?}", delta);
        }
    }

    pub async fn check_and_get(
        &self,
        query: &Query,
    ) -> Result<(bool, Option<Vec<u8>>), CacheError> {
        let begin = Instant::now();
        let lower = query.name.to_lowercase();
        let allowed = !self.block_list.read().await.contains(&lower)
            || self.allow_list.read().await.contains(&lower);

        if !allowed {
            let delta = begin.elapsed();
            if cfg!(debug_assertions) {
                info!("block time: {:?}", delta);
                debug!("in memory block");
            }
            return Ok((true, None));
        }

        let pt = Instant::now();
        let mut buf: heapless::String<128> = heapless::String::new();
        self.query_key(&mut buf, query);
        let key = buf.to_string();

        let res = self.l1.get(&key).await;
        if res.is_some() {
            let delta = begin.elapsed();
            if cfg!(debug_assertions) {
                info!("moka get time: {:?}", delta);
            }
            return Ok((false, res));
        }
        let mut conn = self.rds_conn.clone();
        let delta = pt.elapsed();
        if cfg!(debug_assertions) {
            info!("redis connection time: {:?}", delta);
        }

        let begin = Instant::now();
        let res = redis::cmd("GET")
            .arg(&key)
            .query_async::<Option<Vec<u8>>>(&mut conn)
            .await?;
        let delta = begin.elapsed();
        if cfg!(debug_assertions) {
            info!("redis time: {:?}", delta);
        }
        Ok((false, res))
    }

    #[inline]
    fn query_key(&self, buf: &mut heapless::String<128>, query: &Query) {
        let _ = write!(buf, "dns:{}:{}", query.name, query.query_type);
    }

    pub async fn add_allow_domain(&self, list: &str, domain: &str) -> Result<(), CacheError> {
        let lower = domain.to_lowercase();
        let mut local = self.block_list.write().await;
        local.insert(lower.clone());

        sqlx::query("INSERT INTO allowed (domain) VALUES ($1, $2)")
            .bind(list)
            .bind(domain)
            .execute(&self.pg_pool)
            .await
            .map_err(CacheError::Sql)?;

        Ok(())
    }

    pub async fn add_block_domain(&self, list: &str, domain: &str) -> Result<(), CacheError> {
        let lower = domain.to_lowercase();
        let mut local = self.block_list.write().await;
        local.insert(lower.clone());

        sqlx::query("INSERT INTO blocked (list, domain) VALUES ($1, $2)")
            .bind(list)
            .bind(domain)
            .execute(&self.pg_pool)
            .await
            .map_err(CacheError::Sql)?;

        Ok(())
    }

    pub async fn read_blocklist_db_memory(&self) {
        let mut local = self.block_list.write().await;
        let mut stream = sqlx::query("SELECT domain FROM blocked")
            .map(|row: PgRow| {
                let domain: String = row.try_get("domain").unwrap();
                domain
            })
            .fetch(&self.pg_pool);

        while let Some(Ok(domain)) = stream.next().await {
            local.insert(domain);
        }
    }
}
