use axum::{
    Json, Router,
    extract::{Path, State},
    http::StatusCode,
    routing::{delete, get, patch, post},
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

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

#[derive(Serialize)]
struct ErrResp {
    error: String,
}

type ApiResult<T> = Result<T, (StatusCode, Json<ErrResp>)>;

fn internal(e: impl std::fmt::Display) -> (StatusCode, Json<ErrResp>) {
    (
        StatusCode::INTERNAL_SERVER_ERROR,
        Json(ErrResp {
            error: e.to_string(),
        }),
    )
}

fn bad_request(e: impl std::fmt::Display) -> (StatusCode, Json<ErrResp>) {
    (
        StatusCode::BAD_REQUEST,
        Json(ErrResp {
            error: e.to_string(),
        }),
    )
}

pub fn router(state: ApiState) -> Router {
    #[allow(unused_mut)]
    let mut router = Router::new()
        .route("/health", get(health))
        .route("/metrics", get(metrics))
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
        .route("/config", patch(patch_config));

    #[cfg(feature = "analytics")]
    {
        router = router.route("/stats", get(stats));
    }

    router.with_state(state)
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

async fn get_config(State(s): State<ApiState>) -> Json<RuntimeConfig> {
    Json(RuntimeConfig::clone(&s.config.load()))
}

async fn patch_config(
    State(s): State<ApiState>,
    Json(patch): Json<RuntimeConfigPatch>,
) -> ApiResult<Json<RuntimeConfig>> {
    let merged = patch.apply_to(&s.config.load());

    crate::config::persist(s.cache.pool(), &merged)
        .await
        .map_err(internal)?;

    s.config.store(Arc::new(merged.clone()));
    Ok(Json(merged))
}

#[cfg(feature = "analytics")]
async fn stats(State(s): State<ApiState>) -> axum::response::Response {
    use axum::response::IntoResponse;
    match &s.stats {
        Some(client) => Json(client.get_stats().await).into_response(),
        None => (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(serde_json::json!({ "analytics": "disabled" })),
        )
            .into_response(),
    }
}

async fn health(State(s): State<ApiState>) -> Json<serde_json::Value> {
    let stats = s.cache.stats().await;
    Json(serde_json::json!({
        "status": "ok",
        "block_list_size": stats.block_list_size,
        "allow_list_size": stats.allow_list_size,
        "l1_cache_entries": stats.l1_entry_count,
    }))
}

// ── Blocklist ─────────────────────────────────────────────────────────────

async fn list_blocked(State(s): State<ApiState>) -> ApiResult<Json<Vec<String>>> {
    s.cache
        .list_block_domains()
        .await
        .map(Json)
        .map_err(internal)
}

async fn block_add_one(
    State(s): State<ApiState>,
    Json(req): Json<AddOneReq>,
) -> ApiResult<Json<AddResult>> {
    let outcome = s
        .cache
        .add_block_domain("custom", &req.domain)
        .await
        .map_err(internal)?;
    Ok(Json(AddResult::from_outcome(outcome, 1)))
}

async fn block_add_bulk(
    State(s): State<ApiState>,
    Json(req): Json<AddBulkReq>,
) -> ApiResult<Json<AddResult>> {
    let total = req.domains.len();
    let outcome = s
        .cache
        .add_block_domains("custom", &req.domains)
        .await
        .map_err(internal)?;
    Ok(Json(AddResult::from_outcome(outcome, total)))
}

/// Download a blocklist (hosts-file or domain-per-line) and import it.
async fn block_add_url(
    State(s): State<ApiState>,
    Json(req): Json<AddUrlReq>,
) -> ApiResult<Json<AddResult>> {
    let body = reqwest::get(&req.url)
        .await
        .and_then(|r| r.error_for_status())
        .map_err(bad_request)?
        .text()
        .await
        .map_err(bad_request)?;

    let domains = parse_domain_list(&body);
    let total = domains.len();
    let list = req.list.unwrap_or(req.url);

    let outcome = s
        .cache
        .add_block_domains(&list, &domains)
        .await
        .map_err(internal)?;
    Ok(Json(AddResult::from_outcome(outcome, total)))
}

// ── Allowlist ─────────────────────────────────────────────────────────────

async fn list_allowed(State(s): State<ApiState>) -> ApiResult<Json<Vec<String>>> {
    s.cache
        .list_allow_domains()
        .await
        .map(Json)
        .map_err(internal)
}

async fn allow_add_one(
    State(s): State<ApiState>,
    Json(req): Json<AddOneReq>,
) -> ApiResult<Json<AddResult>> {
    let outcome = s
        .cache
        .add_allow_domain(&req.domain)
        .await
        .map_err(internal)?;
    Ok(Json(AddResult::from_outcome(outcome, 1)))
}

async fn allow_add_bulk(
    State(s): State<ApiState>,
    Json(req): Json<AddBulkReq>,
) -> ApiResult<Json<AddResult>> {
    let total = req.domains.len();
    let outcome = s
        .cache
        .add_allow_domains(&req.domains)
        .await
        .map_err(internal)?;
    Ok(Json(AddResult::from_outcome(outcome, total)))
}

#[derive(Deserialize)]
struct RemoveBulkReq {
    domains: Vec<String>,
}

async fn remove_bulk(
    State(s): State<ApiState>,
    Json(req): Json<RemoveBulkReq>,
) -> ApiResult<Json<BulkResult>> {
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

    Ok(Json(BulkResult {
        added,
        skipped,
        errors,
    }))
}

async fn remove_one(
    State(s): State<ApiState>,
    Path(domain): Path<String>,
) -> ApiResult<StatusCode> {
    match s.cache.remove_block_domain(&domain).await {
        Ok(true) => Ok(StatusCode::NO_CONTENT),
        Ok(false) => Err((
            StatusCode::NOT_FOUND,
            Json(ErrResp {
                error: format!("{domain} not found in blocklist"),
            }),
        )),
        Err(e) => Err(internal(e)),
    }
}
