use core::fmt::Write;
use ftlog::{debug, error, info};
use redis::RedisError;
use sqlx::PgPool;
use std::{collections::HashSet, sync::Arc};
use tokio::{sync::RwLock, time::Instant};

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
    // moka?
    allow_list: Arc<RwLock<HashSet<String>>>,
    block_list: Arc<RwLock<HashSet<String>>>,
    rds_conn: redis::aio::MultiplexedConnection,
    pg_pool: PgPool,
}

impl Cache {
    pub fn new(rds_conn: redis::aio::MultiplexedConnection, pg_pool: PgPool) -> Self {
        Self {
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

    pub async fn add_dns_query(&self, query: &Query, response: &[u8], ttl: u32) {
        let begin = Instant::now();
        let mut conn = self.rds_conn.clone();
        let mut buf: heapless::String<128> = heapless::String::new();
        self.query_key(&mut buf, &query);
        let key = buf.to_string();
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

    pub async fn check_and_get(
        &self,
        query: &Query,
    ) -> Result<(bool, Option<Vec<u8>>), CacheError> {
        let begin = Instant::now();
        let lower = query.name.to_lowercase();

        let allow = self.allow_list.read().await;
        if allow.contains(&lower) {
            let delta = begin.elapsed();
            if cfg!(debug_assertions) {
                info!("allow time: {:?}", delta);
            }
            return Ok((false, None));
        }

        let block = self.block_list.read().await;
        if block.contains(&lower) {
            let delta = begin.elapsed();
            if cfg!(debug_assertions) {
                info!("block time: {:?}", delta);
                debug!("in memory block");
            }
            return Ok((true, None));
        }

        let pt = Instant::now();
        let mut conn = self.rds_conn.clone();
        let delta = pt.elapsed();
        if cfg!(debug_assertions) {
            info!("redis connection time: {:?}", delta);
        }

        let begin = Instant::now();
        let mut buf: heapless::String<128> = heapless::String::new();
        self.query_key(&mut buf, &query);
        let key = buf.to_string();

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
        write!(buf, "dns:{}:{}", query.name, query.query_type);
    }

    pub async fn add_allow_domain(&self, list: &str, domain: &str) -> Result<(), CacheError> {
        let lower = domain.to_lowercase();
        let mut local = self.block_list.write().await;
        local.insert(lower.clone());

        sqlx::query("INSERT INTO blocked ('list', 'domain') VALUES ?, ?")
            .bind(list)
            .bind(domain)
            .execute(&self.pg_pool)
            .await?;

        Ok(())
    }

    pub async fn add_block_domain(&self, list: &str, domain: &str) -> Result<(), CacheError> {
        let lower = domain.to_lowercase();
        let mut local = self.allow_list.write().await;
        local.insert(lower.clone());

        sqlx::query("INSERT INTO allowed ('list', 'domain') VALUES ?, ?")
            .bind(list)
            .bind(domain)
            .execute(&self.pg_pool)
            .await?;

        Ok(())
    }
}
