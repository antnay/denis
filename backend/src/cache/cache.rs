use std::{collections::HashSet, sync::Arc};

use deadpool_redis::PoolError;
use ftlog::{debug, error, info};
use redis::RedisError;
use tokio::{sync::RwLock, time::Instant};

use crate::handler::Query;

#[derive(thiserror::Error, Debug)]
pub enum CacheError {
    #[error("could not get pool: {0}")]
    GetConn(PoolError),
    #[error("could not get key \"{1}\": {0}")]
    Get(RedisError, String),
    // #[error("could not set key \"{1}\": {0}")]
    // Set(RedisError, String),
}

impl From<PoolError> for CacheError {
    fn from(err: PoolError) -> Self {
        CacheError::GetConn(err)
    }
}

#[derive(thiserror::Error, Debug)]
pub enum BlocklistError {
    #[error("could not get pool: {0}")]
    GetConn(PoolError),
    // #[error("could check if \"{1}\" is blocked: {0}")]
    // IsBlocked(RedisError, String),
    #[error("could not add \"{1}\" to block set: {0}")]
    AddDomain(RedisError, String),
}

impl From<PoolError> for BlocklistError {
    fn from(err: PoolError) -> Self {
        BlocklistError::GetConn(err)
    }
}

pub struct Cache {
    conn: redis::aio::MultiplexedConnection,
    list: Arc<RwLock<HashSet<String>>>,
    // moka?
}

impl Cache {
    pub fn new(conn: redis::aio::MultiplexedConnection) -> Self {
        Self {
            conn,
            list: Arc::new(RwLock::new(HashSet::new())),
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

    pub async fn add_query(&self, query: &Query, response: &[u8], ttl: u32) {
        let begin = Instant::now();
        let mut conn = self.conn.clone();

        let key = self.query_key(&query);
        let _ = redis::cmd("SETEX")
            .arg(&key)
            .arg(ttl)
            .arg(response)
            .query_async::<()>(&mut conn)
            .await
            .map_err(|e| error!("could not add query {}", e));
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
        let local = self.list.read().await;
        if local.contains(&lower) {
            let delta = begin.elapsed();
            if cfg!(debug_assertions) {
                info!("cache time: {:?}", delta);
                debug!("in memory block");
            }
            return Ok((true, None));
        }

        let pt = Instant::now();
        let mut conn = self.conn.clone();
        let delta = pt.elapsed();
        if cfg!(debug_assertions) {
            info!("redis connection time: {:?}", delta);
        }

        let begin = Instant::now();
        let key = self.query_key(&query);
        let res = redis::cmd("GET")
            .arg(&key)
            .query_async::<Option<Vec<u8>>>(&mut conn)
            .await
            .map_err(|e| CacheError::Get(e, key))?;

        // let key = self.query_key(&query);
        // let (is_blocked, res) = redis::pipe()
        //     .sismember("block:domains", &query.name)
        //     .get(&key)
        //     .query_async::<(bool, Option<Vec<u8>>)>(&mut conn)
        //     .await
        //     .map_err(|e| CacheError::Get(e, key))?;
        let delta = begin.elapsed();
        if cfg!(debug_assertions) {
            info!("redis time: {:?}", delta);
        }

        // Ok((is_blocked, res))
        Ok((false, res))
    }

    fn query_key(&self, query: &Query) -> String {
        format!("dns:{}:{}", query.name, query.query_type)
    }

    // pub fn fetch_lists(&self, urls: &[String]) {}

    // pub async fn is_blocked(&self, query: &Query) -> Result<bool, BlocklistError> {
    //     let lower = query.name.to_lowercase();
    //     let local = self.list.read().await;
    //     if local.contains(&lower) {
    //         if cfg!(debug_assertions) {
    //             debug!("in memory block");
    //         }
    //         return Ok(true);
    //     }
    //
    //     let parts: Vec<&str> = lower.split('.').collect();
    //     for i in 1..parts.len() {
    //         let parent = parts[i..].join(".");
    //         if local.contains(&parent) {
    //             if cfg!(debug_assertions) {
    //                 debug!("in memory block");
    //             }
    //             return Ok(true);
    //         }
    //     }
    //
    //     let mut conn = self.pool.get().await?;
    //     cmd("SISMEMBER")
    //         .arg("domains")
    //         .arg(&query.name)
    //         .query_async::<bool>(&mut conn)
    //         .await
    //         .map_err(|e| BlocklistError::IsBlocked(e, query.name.clone()))
    // }

    pub async fn add_block_domain(&self, domain: &str) -> Result<(), BlocklistError> {
        let lower = domain.to_lowercase();
        let mut local = self.list.write().await;
        local.insert(lower.clone());

        let mut conn = self.conn.clone();
        redis::cmd("SADD")
            .arg("block:domains")
            .arg(domain.trim())
            .query_async::<()>(&mut conn)
            .await
            .map_err(|e| BlocklistError::AddDomain(e, domain.to_string()))
    }

    // pub async fn add_block_domain_batch(&self, domains: &[String]) -> Result<(), BlocklistError> {
    //     todo!()
    // }
}
