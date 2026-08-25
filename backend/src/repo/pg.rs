use std::time::Duration;

#[derive(Debug, Clone)]
pub struct PGConfig {
    pub url: String,
    pub max_connections: u32,
    pub idle_timeout: Duration,
}

impl Default for PGConfig {
    fn default() -> Self {
        Self {
            url: std::env::var("DATABASE_URL")
                .unwrap_or_else(|_| "postgresql://postgres:postgres@localhost:5433/denis".into()),
            max_connections: 16,
            idle_timeout: Duration::from_secs(5),
        }
    }
}
