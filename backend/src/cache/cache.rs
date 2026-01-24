use deadpool_redis::{
    Config, CreatePoolError, Pool, PoolError, Runtime,
    redis::{RedisError, cmd},
};

use crate::{cache::RedisCacheConfig, handler::Query};

#[derive(thiserror::Error, Debug)]
pub enum CacheError {
    #[error("could not connect to redis: {0}")]
    CreatePool(CreatePoolError),
    #[error("could not get pool: {0}")]
    GetConn(PoolError),
    #[error("could not get key \"{1}\": {0}")]
    Get(RedisError, String),
    #[error("could not set key \"{1}\": {0}")]
    Set(RedisError, String),
}

impl From<PoolError> for CacheError {
    fn from(err: PoolError) -> Self {
        CacheError::GetConn(err)
    }
}

pub struct RdsCache {
    pool: Pool,
}

impl RdsCache {
    pub fn new(config: RedisCacheConfig) -> Result<Self, CacheError> {
        let conf = Config::from_url(&config.url);
        let pool = conf
            .create_pool(Some(Runtime::Tokio1))
            .map_err(|e| CacheError::CreatePool(e))?;
        Ok(Self { pool })
    }

    pub async fn get_query(&self, query: &Query) -> Result<Option<Vec<u8>>, CacheError> {
        let mut conn = self.pool.get().await?;
        let key = self.key(&query);
        let res = cmd("GET")
            .arg(&key)
            .query_async::<Option<Vec<u8>>>(&mut conn)
            .await
            .map_err(|e| CacheError::Get(e, key))?;
        Ok(res)
    }

    pub async fn add_query(
        &self,
        query: &Query,
        response: &[u8],
        ttl: u32,
    ) -> Result<(), CacheError> {
        let mut conn = self.pool.get().await?;
        let key = self.key(&query);
        cmd("SETEX")
            .arg(&key)
            .arg(ttl)
            .arg(response)
            .query_async::<()>(&mut conn)
            .await
            .map_err(|e| CacheError::Set(e, key))
    }

    fn key(&self, query: &Query) -> String {
        format!("{}:{}", query.name, query.query_type)
    }
}
