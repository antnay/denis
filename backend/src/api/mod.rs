use axum::{
    Json, Router,
    extract::{Path, State},
    http::StatusCode,
    routing::{delete, get, post},
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

use crate::{analytics::StatsClient, repo::Cache};

#[derive(Clone)]
pub struct ApiState {
    pub cache: Arc<Cache>,
    pub stats: Arc<StatsClient>,
}

#[derive(Deserialize)]
struct AddOneReq {
    domain: String,
}

#[derive(Deserialize)]
struct AddBulkReq {
    domains: Vec<String>,
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
        Json(ErrResp { error: e.to_string() }),
    )
}

pub fn router(state: ApiState) -> Router {
    Router::new()
        .route("/health", get(health))
        .route("/stats", get(stats))
        .route("/blocklist", get(list_blocked))
        .route("/blocklist", post(add_one))
        .route("/blocklist/bulk", post(add_bulk))
        .route("/blocklist/bulk/remove", post(remove_bulk))
        .route("/blocklist/:domain", delete(remove_one))
        .with_state(state)
}

async fn stats(State(s): State<ApiState>) -> Json<crate::analytics::AllStats> {
    Json(s.stats.get_stats().await)
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

async fn list_blocked(State(s): State<ApiState>) -> ApiResult<Json<Vec<String>>> {
    s.cache
        .list_block_domains()
        .await
        .map(Json)
        .map_err(internal)
}

async fn add_one(
    State(s): State<ApiState>,
    Json(req): Json<AddOneReq>,
) -> ApiResult<StatusCode> {
    s.cache
        .add_block_domain("custom", &req.domain)
        .await
        .map(|_| StatusCode::CREATED)
        .map_err(internal)
}

async fn add_bulk(
    State(s): State<ApiState>,
    Json(req): Json<AddBulkReq>,
) -> ApiResult<Json<BulkResult>> {
    let mut added = 0usize;
    let mut skipped = 0usize;
    let mut errors: Vec<String> = Vec::new();

    for domain in req.domains {
        match s.cache.add_block_domain("custom", &domain).await {
            Ok(_) => added += 1,
            Err(e) => {
                let msg = e.to_string();
                // ON CONFLICT DO NOTHING means the row existed — count as skipped
                if msg.contains("unique") || msg.contains("duplicate") {
                    skipped += 1;
                } else {
                    errors.push(format!("{domain}: {msg}"));
                }
            }
        }
    }

    Ok(Json(BulkResult { added, skipped, errors }))
}

#[derive(Deserialize)]
struct RemoveBulkReq {
    domains: Vec<String>,
}

async fn remove_bulk(
    State(s): State<ApiState>,
    Json(req): Json<RemoveBulkReq>,
) -> ApiResult<Json<BulkResult>> {
    let mut added = 0usize; // reused as "removed"
    let mut skipped = 0usize;
    let mut errors: Vec<String> = Vec::new();

    for domain in req.domains {
        match s.cache.remove_block_domain(&domain).await {
            Ok(true) => added += 1,
            Ok(false) => skipped += 1,
            Err(e) => errors.push(format!("{domain}: {e}")),
        }
    }

    Ok(Json(BulkResult { added, skipped, errors }))
}

async fn remove_one(
    State(s): State<ApiState>,
    Path(domain): Path<String>,
) -> ApiResult<StatusCode> {
    match s.cache.remove_block_domain(&domain).await {
        Ok(true) => Ok(StatusCode::NO_CONTENT),
        Ok(false) => Err((
            StatusCode::NOT_FOUND,
            Json(ErrResp { error: format!("{domain} not found in blocklist") }),
        )),
        Err(e) => Err(internal(e)),
    }
}
