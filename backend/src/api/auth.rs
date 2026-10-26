use std::{
    collections::HashMap,
    sync::{Arc, RwLock},
    time::{Duration, Instant},
};

use argon2::{
    Argon2, PasswordHash, PasswordHasher, PasswordVerifier,
    password_hash::{SaltString, rand_core::OsRng},
};
use axum::{
    Json,
    extract::{Path, Request, State},
    http::{StatusCode, header::AUTHORIZATION},
    middleware::Next,
    response::{IntoResponse, Response},
};
use ftlog::warn;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use super::{ApiError, ApiResponse, ApiState};
use crate::db::{DbError, DbPool, ID_PK, QueryBuilder, now_unix};

const SESSION_TTL: Duration = Duration::from_secs(86_400);

pub struct Auth {
    pool: DbPool,
    sessions: RwLock<HashMap<String, Instant>>,
}

impl Auth {
    pub async fn new(pool: DbPool) -> Result<Arc<Self>, DbError> {
        ensure_schema(&pool).await?;

        let users: i64 = sqlx::query_scalar("SELECT count(*) FROM users")
            .fetch_one(&pool)
            .await?;
        if users == 0 {
            QueryBuilder::new("INSERT INTO users (username, password_hash, created_at) ")
                .push_values(
                    [("admin", Self::hash_password("admin"))],
                    |mut b, (u, h)| {
                        b.push_bind(u).push_bind(h).push_bind(now_unix());
                    },
                )
                .build()
                .execute(&pool)
                .await?;
            warn!(
                "seeded default admin/admin — change it via PATCH /users/admin/password (or create your own user and delete admin)"
            );
        }

        Ok(Arc::new(Self {
            pool,
            sessions: RwLock::new(HashMap::new()),
        }))
    }

    pub fn hash_password(password: &str) -> String {
        let salt = SaltString::generate(&mut OsRng);
        Argon2::default()
            .hash_password(password.as_bytes(), &salt)
            .expect("argon2 hash")
            .to_string()
    }

    async fn login(&self, user: &str, pass: &str) -> Option<(String, u64)> {
        let row: Option<(i64, String)> =
            QueryBuilder::new("SELECT id, password_hash FROM users WHERE username = ")
                .push_bind(user)
                .build_query_as()
                .fetch_optional(&self.pool)
                .await
                .ok()?;
        let (id, hash) = row?;
        if !verify(pass, &hash) {
            return None;
        }
        let _ = QueryBuilder::new("UPDATE users SET last_login = ")
            .push_bind(now_unix())
            .push(" WHERE id = ")
            .push_bind(id)
            .build()
            .execute(&self.pool)
            .await;

        let token = random_token();
        self.sessions
            .write()
            .unwrap()
            .insert(token.clone(), Instant::now() + SESSION_TTL);
        Some((token, SESSION_TTL.as_secs()))
    }

    async fn validate(&self, token: &str) -> bool {
        {
            let mut sessions = self.sessions.write().unwrap();
            match sessions.get(token) {
                Some(exp) if *exp > Instant::now() => return true,
                Some(_) => {
                    sessions.remove(token);
                }
                None => {}
            }
        }
        let hash = sha256_hex(token);
        let id: Option<i64> = QueryBuilder::new("SELECT id FROM api_tokens WHERE token_hash = ")
            .push_bind(hash)
            .push(" AND (expires_at IS NULL OR expires_at > ")
            .push_bind(now_unix())
            .push(")")
            .build_query_scalar()
            .fetch_optional(&self.pool)
            .await
            .ok()
            .flatten();
        if let Some(id) = id {
            let _ = QueryBuilder::new("UPDATE api_tokens SET last_used = ")
                .push_bind(now_unix())
                .push(" WHERE id = ")
                .push_bind(id)
                .build()
                .execute(&self.pool)
                .await;
            return true;
        }
        false
    }

    async fn list_users(&self) -> Result<Vec<UserInfo>, DbError> {
        let rows: Vec<(String, i64, Option<i64>)> =
            sqlx::query_as("SELECT username, created_at, last_login FROM users ORDER BY username")
                .fetch_all(&self.pool)
                .await?;
        Ok(rows
            .into_iter()
            .map(|(username, created_at, last_login)| UserInfo {
                username,
                created_at,
                last_login,
            })
            .collect())
    }

    async fn create_user(&self, user: &str, pass: &str) -> Result<(), DbError> {
        QueryBuilder::new("INSERT INTO users (username, password_hash, created_at) ")
            .push_values([(user, Self::hash_password(pass))], |mut b, (u, h)| {
                b.push_bind(u).push_bind(h).push_bind(now_unix());
            })
            .build()
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    async fn set_password(&self, user: &str, pass: &str) -> Result<bool, DbError> {
        let res = QueryBuilder::new("UPDATE users SET password_hash = ")
            .push_bind(Self::hash_password(pass))
            .push(" WHERE username = ")
            .push_bind(user)
            .build()
            .execute(&self.pool)
            .await?;
        Ok(res.rows_affected() > 0)
    }

    async fn delete_user(&self, user: &str) -> Result<bool, DbError> {
        let res = QueryBuilder::new("DELETE FROM users WHERE username = ")
            .push_bind(user)
            .build()
            .execute(&self.pool)
            .await?;
        Ok(res.rows_affected() > 0)
    }

    async fn create_token(&self, name: &str, ttl_secs: Option<i64>) -> Result<String, DbError> {
        let token = random_token();
        let hash = sha256_hex(&token);
        let expires_at = ttl_secs.map(|t| now_unix() + t);
        QueryBuilder::new("INSERT INTO api_tokens (name, token_hash, created_at, expires_at) ")
            .push_values(
                [(name, hash, now_unix(), expires_at)],
                |mut b, (n, h, c, e)| {
                    b.push_bind(n).push_bind(h).push_bind(c).push_bind(e);
                },
            )
            .build()
            .execute(&self.pool)
            .await?;
        Ok(token)
    }

    async fn list_tokens(&self) -> Result<Vec<TokenInfo>, DbError> {
        let rows: Vec<(i64, String, i64, Option<i64>, Option<i64>)> = sqlx::query_as(
            "SELECT id, name, created_at, last_used, expires_at FROM api_tokens ORDER BY id",
        )
        .fetch_all(&self.pool)
        .await?;
        Ok(rows
            .into_iter()
            .map(|(id, name, created_at, last_used, expires_at)| TokenInfo {
                id,
                name,
                created_at,
                last_used,
                expires_at,
            })
            .collect())
    }

    async fn revoke_token(&self, id: i64) -> Result<bool, DbError> {
        let res = QueryBuilder::new("DELETE FROM api_tokens WHERE id = ")
            .push_bind(id)
            .build()
            .execute(&self.pool)
            .await?;
        Ok(res.rows_affected() > 0)
    }
}

async fn ensure_schema(pool: &DbPool) -> Result<(), DbError> {
    sqlx::query(&format!(
        "CREATE TABLE IF NOT EXISTS users (
            id            {ID_PK},
            username      TEXT NOT NULL UNIQUE,
            password_hash TEXT NOT NULL,
            created_at    BIGINT NOT NULL,
            last_login    BIGINT
        )"
    ))
    .execute(pool)
    .await?;
    sqlx::query(&format!(
        "CREATE TABLE IF NOT EXISTS api_tokens (
            id         {ID_PK},
            name       TEXT NOT NULL,
            token_hash TEXT NOT NULL UNIQUE,
            created_at BIGINT NOT NULL,
            last_used  BIGINT,
            expires_at BIGINT
        )"
    ))
    .execute(pool)
    .await?;
    Ok(())
}

// ── request/response types ────────────────────────────────────────────────

#[derive(Deserialize)]
pub struct LoginReq {
    username: String,
    password: String,
}

#[derive(Serialize)]
pub struct LoginResp {
    token: String,
    expires_in: u64,
}

#[derive(Deserialize)]
pub struct CreateUserReq {
    username: String,
    password: String,
}

#[derive(Deserialize)]
pub struct PasswordReq {
    password: String,
}

#[derive(Serialize)]
pub struct UserInfo {
    username: String,
    created_at: i64,
    last_login: Option<i64>,
}

#[derive(Deserialize)]
pub struct CreateTokenReq {
    name: String,
    ttl_secs: Option<i64>,
}

#[derive(Serialize)]
pub struct TokenCreated {
    token: String,
}

#[derive(Serialize)]
pub struct TokenInfo {
    id: i64,
    name: String,
    created_at: i64,
    last_used: Option<i64>,
    expires_at: Option<i64>,
}

#[derive(Serialize)]
pub struct Changed {
    ok: bool,
}

// ── handlers ──────────────────────────────────────────────────────────────

pub async fn login(
    State(s): State<ApiState>,
    Json(req): Json<LoginReq>,
) -> Result<Json<ApiResponse<LoginResp>>, ApiError> {
    match s.auth.login(&req.username, &req.password).await {
        Some((token, expires_in)) => Ok(ApiResponse::ok(LoginResp { token, expires_in })),
        None => Err(ApiError::new(
            StatusCode::UNAUTHORIZED,
            "invalid credentials",
        )),
    }
}

pub async fn list_users(
    State(s): State<ApiState>,
) -> Result<Json<ApiResponse<Vec<UserInfo>>>, ApiError> {
    let users = s.auth.list_users().await.map_err(ApiError::internal)?;
    Ok(ApiResponse::ok(users))
}

pub async fn create_user(
    State(s): State<ApiState>,
    Json(req): Json<CreateUserReq>,
) -> Result<Json<ApiResponse<Changed>>, ApiError> {
    s.auth
        .create_user(&req.username, &req.password)
        .await
        .map_err(ApiError::internal)?;
    Ok(ApiResponse::ok(Changed { ok: true }))
}

pub async fn set_password(
    State(s): State<ApiState>,
    Path(username): Path<String>,
    Json(req): Json<PasswordReq>,
) -> Result<Json<ApiResponse<Changed>>, ApiError> {
    match s.auth.set_password(&username, &req.password).await {
        Ok(true) => Ok(ApiResponse::ok(Changed { ok: true })),
        Ok(false) => Err(ApiError::new(StatusCode::NOT_FOUND, "user not found")),
        Err(e) => Err(ApiError::internal(e)),
    }
}

pub async fn delete_user(
    State(s): State<ApiState>,
    Path(username): Path<String>,
) -> Result<Json<ApiResponse<Changed>>, ApiError> {
    match s.auth.delete_user(&username).await {
        Ok(true) => Ok(ApiResponse::ok(Changed { ok: true })),
        Ok(false) => Err(ApiError::new(StatusCode::NOT_FOUND, "user not found")),
        Err(e) => Err(ApiError::internal(e)),
    }
}

pub async fn create_token(
    State(s): State<ApiState>,
    Json(req): Json<CreateTokenReq>,
) -> Result<Json<ApiResponse<TokenCreated>>, ApiError> {
    let token = s
        .auth
        .create_token(&req.name, req.ttl_secs)
        .await
        .map_err(ApiError::internal)?;
    Ok(ApiResponse::ok(TokenCreated { token }))
}

pub async fn list_tokens(
    State(s): State<ApiState>,
) -> Result<Json<ApiResponse<Vec<TokenInfo>>>, ApiError> {
    let tokens = s.auth.list_tokens().await.map_err(ApiError::internal)?;
    Ok(ApiResponse::ok(tokens))
}

pub async fn revoke_token(
    State(s): State<ApiState>,
    Path(id): Path<i64>,
) -> Result<Json<ApiResponse<Changed>>, ApiError> {
    match s.auth.revoke_token(id).await {
        Ok(true) => Ok(ApiResponse::ok(Changed { ok: true })),
        Ok(false) => Err(ApiError::new(StatusCode::NOT_FOUND, "token not found")),
        Err(e) => Err(ApiError::internal(e)),
    }
}

/// Middleware guarding management routes.
pub async fn require_auth(State(s): State<ApiState>, req: Request, next: Next) -> Response {
    let token = req
        .headers()
        .get(AUTHORIZATION)
        .and_then(|h| h.to_str().ok())
        .and_then(|h| h.strip_prefix("Bearer "));

    match token {
        Some(t) if s.auth.validate(t).await => next.run(req).await,
        _ => ApiError::new(StatusCode::UNAUTHORIZED, "unauthorized").into_response(),
    }
}

// ── helpers ───────────────────────────────────────────────────────────────

fn verify(password: &str, phc: &str) -> bool {
    match PasswordHash::new(phc) {
        Ok(parsed) => Argon2::default()
            .verify_password(password.as_bytes(), &parsed)
            .is_ok(),
        Err(_) => false,
    }
}

fn sha256_hex(input: &str) -> String {
    let digest = Sha256::digest(input.as_bytes());
    let mut s = String::with_capacity(64);
    for b in digest {
        use std::fmt::Write;
        let _ = write!(s, "{b:02x}");
    }
    s
}

fn random_token() -> String {
    use rand::RngCore;
    let mut bytes = [0u8; 32];
    rand::thread_rng().fill_bytes(&mut bytes);
    let mut s = String::with_capacity(64);
    for b in bytes {
        use std::fmt::Write;
        let _ = write!(s, "{b:02x}");
    }
    s
}
