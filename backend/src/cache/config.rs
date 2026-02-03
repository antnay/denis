// use std::time::Duration;

use redis::ConnectionAddr;

#[derive(Debug, Clone)]
pub struct RedisConfig {
    pub url: ConnectionAddr,
    // pub max_connections: usize,
    // pub connection_timeout: Duration,
}

impl Default for RedisConfig {
    fn default() -> Self {
        Self {
            // url: "redis://localhost:6379".into(),
            url: ConnectionAddr::Tcp("localhost".into(), 6379),
            // max_connections: 16,
            // connection_timeout: Duration::from_secs(5),
        }
    }
}
