use std::time::Duration;

#[derive(Debug, Clone)]
pub struct RedisCacheConfig {
    pub url: String,
    pub max_connections: usize,
    pub connection_timeout: Duration,
    pub key_prefix: String,
    pub default_ttl: Duration,
    pub min_ttl: Duration,
    pub max_ttl: Duration,
}

impl Default for RedisCacheConfig {
    fn default() -> Self {
        Self {
            url: "redis://localhost:6379".to_string(),
            max_connections: 16,
            connection_timeout: Duration::from_secs(5),
            key_prefix: "dns".to_string(),
            default_ttl: Duration::from_secs(300),
            min_ttl: Duration::from_secs(60),
            max_ttl: Duration::from_secs(86400),
        }
    }
}
