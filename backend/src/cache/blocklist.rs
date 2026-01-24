use deadpool_redis::{
    Pool, PoolError,
    redis::{RedisError, cmd},
};

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
    pool: Pool,
}

impl Blocklist {
    pub fn new(pool: deadpool_redis::Pool) -> Self {
        Self { pool }
    }

    pub async fn is_blocked(&self, query: &Query) -> Result<bool, BlocklistError> {
        let mut conn = self.pool.get().await?;
        cmd("SISMEMBER")
            .arg("domains")
            .arg(&query.name)
            .query_async::<bool>(&mut conn)
            .await
            .map_err(|e| BlocklistError::IsBlocked(e, query.name.clone()))
    }

    pub async fn add_block_domain(&self, domain: &String) -> Result<(), BlocklistError> {
        let mut conn = self.pool.get().await?;
        cmd("SADD")
            .arg("domains")
            .arg(domain.trim())
            .query_async::<()>(&mut conn)
            .await
            .map_err(|e| BlocklistError::AddDomain(e, domain.clone()))
    }

    pub async fn add_block_domain_batch(&self, domains: &[String]) -> Result<(), BlocklistError> {
        todo!()
    }

    fn block_key(&self, query: &Query) -> String {
        format!("block:{}", query.name.trim())
    }
}
