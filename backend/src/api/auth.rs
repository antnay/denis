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
use sqlx::{PgPool, Row};

use super::{ApiError, ApiResponse, ApiState};

pub struct Auth {
    pool: PgPool,
    /// Optional static token from env, for automation/bootstrap.
    api_token: Option<String>,
    ttl: Duration,
    /// Login-issued session tokens: token -> expiry (in-memory; restart = re-login).
    sessions: RwLock<HashMap<String, Instant>>,
}

impl Auth {
    /// Create the auth tables, seed a bootstrap admin if the users table is
    /// empty (env creds if given, else `admin`/`admin`), and return the handle.
    pub async fn new(
        pool: PgPool,
        api_token: Option<String>,
        ttl: Duration,
        bootstrap_user: Option<String>,
        bootstrap_password_hash: Option<String>,
    ) -> Result<Arc<Self>, sqlx::Error> {
        ensure_schema(&pool).await?;

        let users: i64 = sqlx::query_scalar("SELECT count(*) FROM users")
            .fetch_one(&pool)
            .await?;
        if users == 0 {
            let user = bootstrap_user.unwrap_or_else(|| "admin".to_string());
            let (hash, is_default) = match bootstrap_password_hash {
                Some(h) => (h, false),
                None => (Self::hash_password("admin"), true),
            };
            sqlx::query("INSERT INTO users (username, password_hash) VALUES ($1, $2)")
                .bind(&user)
                .bind(&hash)
                .execute(&pool)
                .await?;
            if is_default {
                warn!("seeded default admin user '{user}' with password 'admin' — change it via PATCH /users/{user}/password");
            } else {
                warn!("seeded bootstrap admin user '{user}'");
            }
        }

        Ok(Arc::new(Self {
            pool,
            api_token,
            ttl,
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
        let row = sqlx::query("SELECT id, password_hash FROM users WHERE username = $1")
            .bind(user)
            .fetch_optional(&self.pool)
            .await
            .ok()??;
        let id: i64 = row.get("id");
        let hash: String = row.get("password_hash");
        if !verify(pass, &hash) {
            return None;
        }
        let _ = sqlx::query("UPDATE users SET last_login = now() WHERE id = $1")
            .bind(id)
            .execute(&self.pool)
            .await;

        let token = random_token();
        self.sessions
            .write()
            .unwrap()
            .insert(token.clone(), Instant::now() + self.ttl);
        Some((token, self.ttl.as_secs()))
    }

    async fn validate(&self, token: &str) -> bool {
        if let Some(t) = &self.api_token {
            if ct_eq(token, t) {
                return true;
            }
        }
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
        // DB-backed API tokens are stored as SHA-256 hashes.
        let hash = sha256_hex(token);
        let id: Option<i64> = sqlx::query_scalar(
            "SELECT id FROM api_tokens
             WHERE token_hash = $1 AND (expires_at IS NULL OR expires_at > now())",
        )
        .bind(&hash)
        .fetch_optional(&self.pool)
        .await
        .ok()
        .flatten();
        if let Some(id) = id {
            let _ = sqlx::query("UPDATE api_tokens SET last_used = now() WHERE id = $1")
                .bind(id)
                .execute(&self.pool)
                .await;
            return true;
        }
        false
    }

    async fn list_users(&self) -> Result<Vec<UserInfo>, sqlx::Error> {
        let rows = sqlx::query(
            "SELECT username, created_at::text AS created_at, last_login::text AS last_login
             FROM users ORDER BY username",
        )
        .fetch_all(&self.pool)
        .await?;
        Ok(rows
            .into_iter()
            .map(|r| UserInfo {
                username: r.get("username"),
                created_at: r.get("created_at"),
                last_login: r.get("last_login"),
            })
            .collect())
    }

    async fn create_user(&self, user: &str, pass: &str) -> Result<(), sqlx::Error> {
        let hash = Self::hash_password(pass);
        sqlx::query("INSERT INTO users (username, password_hash) VALUES ($1, $2)")
            .bind(user)
            .bind(&hash)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    async fn set_password(&self, user: &str, pass: &str) -> Result<bool, sqlx::Error> {
        let hash = Self::hash_password(pass);
        let res = sqlx::query("UPDATE users SET password_hash = $1 WHERE username = $2")
            .bind(&hash)
            .bind(user)
            .execute(&self.pool)
            .await?;
        Ok(res.rows_affected() > 0)
    }

    async fn delete_user(&self, user: &str) -> Result<bool, sqlx::Error> {
        let res = sqlx::query("DELETE FROM users WHERE username = $1")
            .bind(user)
            .execute(&self.pool)
            .await?;
        Ok(res.rows_affected() > 0)
    }

    async fn create_token(&self, name: &str, ttl_secs: Option<i64>) -> Result<String, sqlx::Error> {
        let token = random_token();
        let hash = sha256_hex(&token);
        sqlx::query(
            "INSERT INTO api_tokens (name, token_hash, expires_at)
             VALUES ($1, $2, CASE WHEN $3::bigint IS NULL THEN NULL
                                  ELSE now() + ($3 * interval '1 second') END)",
        )
        .bind(name)
        .bind(&hash)
        .bind(ttl_secs)
        .execute(&self.pool)
        .await?;
        Ok(token)
    }

    async fn list_tokens(&self) -> Result<Vec<TokenInfo>, sqlx::Error> {
        let rows = sqlx::query(
            "SELECT id, name, created_at::text AS created_at,
                    last_used::text AS last_used, expires_at::text AS expires_at
             FROM api_tokens ORDER BY id",
        )
        .fetch_all(&self.pool)
        .await?;
        Ok(rows
            .into_iter()
            .map(|r| TokenInfo {
                id: r.get("id"),
                name: r.get("name"),
                created_at: r.get("created_at"),
                last_used: r.get("last_used"),
                expires_at: r.get("expires_at"),
            })
            .collect())
    }

    async fn revoke_token(&self, id: i64) -> Result<bool, sqlx::Error> {
        let res = sqlx::query("DELETE FROM api_tokens WHERE id = $1")
            .bind(id)
            .execute(&self.pool)
            .await?;
        Ok(res.rows_affected() > 0)
    }
}

async fn ensure_schema(pool: &PgPool) -> Result<(), sqlx::Error> {
    sqlx::query(
        "CREATE TABLE IF NOT EXISTS users (
            id            BIGSERIAL PRIMARY KEY,
            username      VARCHAR(64) NOT NULL UNIQUE,
            password_hash TEXT NOT NULL,
            created_at    TIMESTAMPTZ NOT NULL DEFAULT now(),
            last_login    TIMESTAMPTZ
        )",
    )
    .execute(pool)
    .await?;
    sqlx::query(
        "CREATE TABLE IF NOT EXISTS api_tokens (
            id         BIGSERIAL PRIMARY KEY,
            name       VARCHAR(128) NOT NULL,
            token_hash CHAR(64) NOT NULL UNIQUE,
            created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
            last_used  TIMESTAMPTZ,
            expires_at TIMESTAMPTZ
        )",
    )
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
    created_at: Option<String>,
    last_login: Option<String>,
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
    created_at: Option<String>,
    last_used: Option<String>,
    expires_at: Option<String>,
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
        None => Err(ApiError::new(StatusCode::UNAUTHORIZED, "invalid credentials")),
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

fn ct_eq(a: &str, b: &str) -> bool {
    let (a, b) = (a.as_bytes(), b.as_bytes());
    if a.len() != b.len() {
        return false;
    }
    let mut diff = 0u8;
    for (x, y) in a.iter().zip(b) {
        diff |= x ^ y;
    }
    diff == 0
}
