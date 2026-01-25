use std::{collections::HashSet, sync::Arc};

use deadpool_redis::{
    Pool, PoolError,
    redis::{RedisError, cmd},
};
use ftlog::debug;
use tokio::sync::RwLock;

use crate::handler::Query;

#[derive(thiserror::Error, Debug)]
pub enum BlocklistError {
    #[error("could not get pool: {0}")]
    GetConn(PoolError),
    #[error("could check if \"{1}\" is blocked: {0}")]
    IsBlocked(RedisError, String),
    #[error("could not add \"{1}\" to block set: {0}")]
    AddDomain(RedisError, String),
}

impl From<PoolError> for BlocklistError {
    fn from(err: PoolError) -> Self {
        BlocklistError::GetConn(err)
    }
}

pub struct Blocklist {
    list: Arc<RwLock<HashSet<String>>>,
    pool: Pool,
}

impl Blocklist {
    pub fn new(pool: deadpool_redis::Pool) -> Self {
        Self {
            list: Arc::new(RwLock::new(HashSet::new())),
            pool,
        }
    }

    pub fn fetch_lists(&self, urls: &[String]) {}

    pub async fn is_blocked(&self, query: &Query) -> Result<bool, BlocklistError> {
        let lower = query.name.to_lowercase();
        let local = self.list.read().await;
        if local.contains(&lower) {
            debug!("in memory block");
            return Ok(true);
        }

        let parts: Vec<&str> = lower.split('.').collect();
        for i in 1..parts.len() {
            let parent = parts[i..].join(".");
            if local.contains(&parent) {
                debug!("in memory block");
                return Ok(true);
            }
        }

        let mut conn = self.pool.get().await?;
        cmd("SISMEMBER")
            .arg("domains")
            .arg(&query.name)
            .query_async::<bool>(&mut conn)
            .await
            .map_err(|e| BlocklistError::IsBlocked(e, query.name.clone()))
    }

    pub async fn add_block_domain(&self, domain: &str) -> Result<(), BlocklistError> {
        debug!("block list: {:#?}", self.list);
        let lower = domain.to_lowercase();
        let mut local = self.list.write().await;
        local.insert(lower.clone());

        let mut conn = self.pool.get().await?;
        cmd("SADD")
            .arg("block:domains")
            .arg(domain.trim())
            .query_async::<()>(&mut conn)
            .await
            .map_err(|e| BlocklistError::AddDomain(e, domain.to_string()))
    }

    pub async fn add_block_domain_batch(&self, domains: &[String]) -> Result<(), BlocklistError> {
        todo!()
    }

    fn block_key(&self, query: &Query) -> String {
        format!("block:{}", query.name.trim())
    }
}
