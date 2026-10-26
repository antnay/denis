#[cfg(all(feature = "sqlite", feature = "postgres"))]
compile_error!("enable only one database backend: `sqlite` or `postgres`");
#[cfg(not(any(feature = "sqlite", feature = "postgres")))]
compile_error!("enable a database backend: `sqlite` (default) or `postgres`");

use std::time::{SystemTime, UNIX_EPOCH};

#[cfg(feature = "postgres")]
pub type Db = sqlx::Postgres;
#[cfg(all(feature = "sqlite", not(feature = "postgres")))]
pub type Db = sqlx::Sqlite;

pub type DbPool = sqlx::Pool<Db>;
pub type QueryBuilder<'a> = sqlx::QueryBuilder<'a, Db>;

#[derive(thiserror::Error, Debug)]
pub enum DbError {
    #[error("database: {0}")]
    Sql(#[from] sqlx::Error),
}

#[cfg(feature = "postgres")]
pub const ID_PK: &str = "BIGSERIAL PRIMARY KEY";
#[cfg(all(feature = "sqlite", not(feature = "postgres")))]
pub const ID_PK: &str = "INTEGER PRIMARY KEY AUTOINCREMENT";

pub async fn connect(url: &str, max_connections: u32) -> Result<DbPool, DbError> {
    #[cfg(all(feature = "sqlite", not(feature = "postgres")))]
    {
        use std::str::FromStr;
        let opts = sqlx::sqlite::SqliteConnectOptions::from_str(url)?.create_if_missing(true);
        Ok(sqlx::sqlite::SqlitePoolOptions::new()
            .max_connections(max_connections)
            .connect_with(opts)
            .await?)
    }
    #[cfg(feature = "postgres")]
    {
        Ok(sqlx::postgres::PgPoolOptions::new()
            .max_connections(max_connections)
            .connect(url)
            .await?)
    }
}

pub fn now_unix() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64
}
