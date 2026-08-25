use axum::{
    Json, Router,
    extract::{Path, State},
    http::StatusCode,
    routing::{delete, get, patch, post},
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

mod auth;

pub use auth::Auth;

use crate::{
    analytics::Metrics,
    config::{RuntimeConfig, RuntimeConfigPatch, SharedConfig},
    repo::{AddOutcome, Cache, parse_domain_list},
};

#[cfg(feature = "analytics")]
use crate::analytics::StatsClient;

#[derive(Clone)]
pub struct ApiState {
    pub cache: Arc<Cache>,
    #[cfg(feature = "analytics")]
    pub stats: Option<Arc<StatsClient>>,
    pub metrics: Option<Arc<Metrics>>,
    pub config: SharedConfig,
    pub auth: Arc<Auth>,
}

#[derive(Deserialize)]
struct AddOneReq {
    domain: String,
}

#[derive(Deserialize)]
struct AddBulkReq {
    domains: Vec<String>,
}

#[derive(Deserialize)]
struct AddUrlReq {
    url: String,
    list: Option<String>,
}

#[derive(Serialize)]
struct AddResult {
    added: usize,
    skipped: usize,
    invalid: usize,
}

impl AddResult {
    fn from_outcome(outcome: AddOutcome, total_input: usize) -> Self {
        Self {
            added: outcome.added,
            skipped: outcome.considered - outcome.added,
            invalid: total_input - outcome.considered,
        }
    }
}

#[derive(Serialize)]
struct BulkResult {
    added: usize,
    skipped: usize,
    errors: Vec<String>,
}

/// Uniform envelope for every JSON response (success and error).
/// `/metrics` is the only exception — it must emit Prometheus text format.
#[derive(Serialize)]
pub(crate) struct ApiResponse<T> {
    success: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    data: Option<T>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<String>,
}

impl<T: Serialize> ApiResponse<T> {
    fn ok(data: T) -> Json<ApiResponse<T>> {
        Json(ApiResponse {
            success: true,
            data: Some(data),
            error: None,
        })
    }
}

pub(crate) struct ApiError {
    status: StatusCode,
    message: String,
}

impl ApiError {
    pub(crate) fn new(status: StatusCode, message: impl std::fmt::Display) -> Self {
        Self {
            status,
            message: message.to_string(),
        }
    }
    fn internal(e: impl std::fmt::Display) -> Self {
        Self::new(StatusCode::INTERNAL_SERVER_ERROR, e)
    }
    fn bad_request(e: impl std::fmt::Display) -> Self {
        Self::new(StatusCode::BAD_REQUEST, e)
    }
}

impl axum::response::IntoResponse for ApiError {
    fn into_response(self) -> axum::response::Response {
        (
            self.status,
            Json(ApiResponse::<()> {
                success: false,
                data: None,
                error: Some(self.message),
            }),
        )
            .into_response()
    }
}

type ApiResult<T> = Result<Json<ApiResponse<T>>, ApiError>;

pub fn router(state: ApiState) -> Router {
    // Public: no auth (Prometheus scrape, health checks, obtaining a token).
    let public = Router::new()
        .route("/health", get(health))
        .route("/metrics", get(metrics))
        .route("/auth/login", post(auth::login));

    // Protected: everything that reads or mutates management state.
    #[allow(unused_mut)]
    let mut protected = Router::new()
        .route("/blocklist", get(list_blocked))
        .route("/blocklist", post(block_add_one))
        .route("/blocklist/bulk", post(block_add_bulk))
        .route("/blocklist/url", post(block_add_url))
        .route("/blocklist/bulk/remove", post(remove_bulk))
        .route("/blocklist/:domain", delete(remove_one))
        .route("/allowlist", get(list_allowed))
        .route("/allowlist", post(allow_add_one))
        .route("/allowlist/bulk", post(allow_add_bulk))
        .route("/config", get(get_config))
        .route("/config", patch(patch_config))
        .route("/users", get(auth::list_users))
        .route("/users", post(auth::create_user))
        .route("/users/:username", delete(auth::delete_user))
        .route("/users/:username/password", patch(auth::set_password))
        .route("/tokens", get(auth::list_tokens))
        .route("/tokens", post(auth::create_token))
        .route("/tokens/:id", delete(auth::revoke_token));

    #[cfg(feature = "analytics")]
    {
        protected = protected.route("/stats", get(stats));
    }

    let protected = protected.route_layer(axum::middleware::from_fn_with_state(
        state.clone(),
        auth::require_auth,
    ));

    public.merge(protected).with_state(state)
}

/// Prometheus scrape endpoint. Sink gauges are sampled here at scrape time.
async fn metrics(State(s): State<ApiState>) -> axum::response::Response {
    use axum::response::IntoResponse;
    match &s.metrics {
        Some(m) => {
            let stats = s.cache.stats().await;
            let body = m.render(
                stats.block_list_size,
                stats.allow_list_size,
                stats.l1_entry_count,
            );
            ([("content-type", "text/plain; version=0.0.4")], body).into_response()
        }
        None => (StatusCode::SERVICE_UNAVAILABLE, "prometheus disabled").into_response(),
    }
}

async fn get_config(State(s): State<ApiState>) -> ApiResult<RuntimeConfig> {
    Ok(ApiResponse::ok(RuntimeConfig::clone(&s.config.load())))
}

async fn patch_config(
    State(s): State<ApiState>,
    Json(patch): Json<RuntimeConfigPatch>,
) -> ApiResult<RuntimeConfig> {
    let merged = patch.apply_to(&s.config.load());

    crate::config::persist(s.cache.pool(), &merged)
        .await
        .map_err(ApiError::internal)?;

    s.config.store(Arc::new(merged.clone()));
    Ok(ApiResponse::ok(merged))
}

#[cfg(feature = "analytics")]
async fn stats(State(s): State<ApiState>) -> axum::response::Response {
    use axum::response::IntoResponse;
    match &s.stats {
        Some(client) => ApiResponse::ok(client.get_stats().await).into_response(),
        None => ApiError::new(StatusCode::SERVICE_UNAVAILABLE, "analytics disabled").into_response(),
    }
}

#[derive(Serialize)]
struct HealthData {
    status: &'static str,
    block_list_size: usize,
    allow_list_size: usize,
    l1_cache_entries: u64,
}

async fn health(State(s): State<ApiState>) -> Json<ApiResponse<HealthData>> {
    let stats = s.cache.stats().await;
    ApiResponse::ok(HealthData {
        status: "ok",
        block_list_size: stats.block_list_size,
        allow_list_size: stats.allow_list_size,
        l1_cache_entries: stats.l1_entry_count,
    })
}

// ── Blocklist ─────────────────────────────────────────────────────────────

async fn list_blocked(State(s): State<ApiState>) -> ApiResult<Vec<String>> {
    let domains = s.cache.list_block_domains().await.map_err(ApiError::internal)?;
    Ok(ApiResponse::ok(domains))
}

async fn block_add_one(
    State(s): State<ApiState>,
    Json(req): Json<AddOneReq>,
) -> ApiResult<AddResult> {
    let outcome = s
        .cache
        .add_block_domain("custom", &req.domain)
        .await
        .map_err(ApiError::internal)?;
    Ok(ApiResponse::ok(AddResult::from_outcome(outcome, 1)))
}

async fn block_add_bulk(
    State(s): State<ApiState>,
    Json(req): Json<AddBulkReq>,
) -> ApiResult<AddResult> {
    let total = req.domains.len();
    let outcome = s
        .cache
        .add_block_domains("custom", &req.domains)
        .await
        .map_err(ApiError::internal)?;
    Ok(ApiResponse::ok(AddResult::from_outcome(outcome, total)))
}

/// Download a blocklist (hosts-file or domain-per-line) and import it.
async fn block_add_url(
    State(s): State<ApiState>,
    Json(req): Json<AddUrlReq>,
) -> ApiResult<AddResult> {
    let body = reqwest::get(&req.url)
        .await
        .and_then(|r| r.error_for_status())
        .map_err(ApiError::bad_request)?
        .text()
        .await
        .map_err(ApiError::bad_request)?;

    let domains = parse_domain_list(&body);
    let total = domains.len();
    let list = req.list.unwrap_or(req.url);

    let outcome = s
        .cache
        .add_block_domains(&list, &domains)
        .await
        .map_err(ApiError::internal)?;
    Ok(ApiResponse::ok(AddResult::from_outcome(outcome, total)))
}

// ── Allowlist ─────────────────────────────────────────────────────────────

async fn list_allowed(State(s): State<ApiState>) -> ApiResult<Vec<String>> {
    let domains = s.cache.list_allow_domains().await.map_err(ApiError::internal)?;
    Ok(ApiResponse::ok(domains))
}

async fn allow_add_one(
    State(s): State<ApiState>,
    Json(req): Json<AddOneReq>,
) -> ApiResult<AddResult> {
    let outcome = s
        .cache
        .add_allow_domain(&req.domain)
        .await
        .map_err(ApiError::internal)?;
    Ok(ApiResponse::ok(AddResult::from_outcome(outcome, 1)))
}

async fn allow_add_bulk(
    State(s): State<ApiState>,
    Json(req): Json<AddBulkReq>,
) -> ApiResult<AddResult> {
    let total = req.domains.len();
    let outcome = s
        .cache
        .add_allow_domains(&req.domains)
        .await
        .map_err(ApiError::internal)?;
    Ok(ApiResponse::ok(AddResult::from_outcome(outcome, total)))
}

#[derive(Deserialize)]
struct RemoveBulkReq {
    domains: Vec<String>,
}

async fn remove_bulk(
    State(s): State<ApiState>,
    Json(req): Json<RemoveBulkReq>,
) -> ApiResult<BulkResult> {
    let mut added = 0usize;
    let mut skipped = 0usize;
    let mut errors: Vec<String> = Vec::new();

    for domain in req.domains {
        match s.cache.remove_block_domain(&domain).await {
            Ok(true) => added += 1,
            Ok(false) => skipped += 1,
            Err(e) => errors.push(format!("{domain}: {e}")),
        }
    }

    Ok(ApiResponse::ok(BulkResult {
        added,
        skipped,
        errors,
    }))
}

#[derive(Serialize)]
struct Removed {
    removed: bool,
}

async fn remove_one(
    State(s): State<ApiState>,
    Path(domain): Path<String>,
) -> ApiResult<Removed> {
    match s.cache.remove_block_domain(&domain).await {
        Ok(true) => Ok(ApiResponse::ok(Removed { removed: true })),
        Ok(false) => Err(ApiError::new(
            StatusCode::NOT_FOUND,
            format!("{domain} not found in blocklist"),
        )),
        Err(e) => Err(ApiError::internal(e)),
    }
}
