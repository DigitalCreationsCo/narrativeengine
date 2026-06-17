//! NAP HTTP Resolver Server — Axum-based REST API.
//!
//! Endpoints:
//!   GET  /resolve/:universe/:entity_type/:entity_id  — Resolve a manifest
//!   POST /commit/:universe/:entity_type/:entity_id   — Commit changes
//!   GET  /history/:universe/:entity_type/:entity_id   — Get commit history
//!   GET  /schema/{name}                               — Get JSON Schema for a type
//!   GET  /universes                                   — List all universes
//!   GET  /universes/:universe/entities                 — List entities in a universe
//!   POST /sync/:universe                              — Push configured remote
//!   GET  /health                                      — Health check
//!
//! Query parameters:
//!   branch — Resolve at a specific branch
//!   commit — Resolve at a specific commit hash
//!   tag    — Resolve at a specific tag
//!   path   — Subtree query path

use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;

use axum::{
    Router,
    extract::{Path, Query, State},
    http::StatusCode,
    response::Json,
    routing::{get, post},
};
use serde::{Deserialize, Serialize};
use tower_http::cors::CorsLayer;
use tower_http::trace::TraceLayer;
use tracing::{error, info};

use nap_core::{
    commit::Change,
    repository::Repository,
    resolver::{ResolveOptions, ResolveResult, Resolver},
    schema,
    types::EntityType,
    vcs_git::GitBackend,
};

/// Application state shared across handlers.
struct AppState {
    base_path: PathBuf,
}

/// Query parameters for resolution.
#[derive(Debug, Deserialize)]
struct ResolveQuery {
    branch: Option<String>,
    commit: Option<String>,
    tag: Option<String>,
    path: Option<String>,
}

/// Request body for commits.
#[derive(Debug, Deserialize)]
struct CommitRequest {
    message: String,
    author: String,
    properties: Option<serde_json::Map<String, serde_json::Value>>,
}

/// Request body for reverts.
#[derive(Debug, Deserialize)]
struct RevertRequest {
    commit: String,
    author: String,
}

/// API error response.
#[derive(Debug, Serialize)]
struct ApiError {
    error: String,
    code: String,
}

#[tokio::main]
async fn main() {
    // Initialize tracing
    tracing_subscriber::fmt()
        .with_env_filter("nap_server=debug,nap_core=debug,tower_http=debug")
        .with_target(false)
        .init();

    let base_path = std::env::var("NAP_DIR")
        .or_else(|_| std::env::var("NAP_BASE_PATH"))
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("."));

    info!(base_path = %base_path.display(), "starting NAP resolver server");

    let state = Arc::new(AppState { base_path });

    let app = Router::new()
        // Resolution
        .route(
            "/resolve/{universe}/{entity_type}/{entity_id}",
            get(handle_resolve),
        )
        // Commit
        .route(
            "/commit/{universe}/{entity_type}/{entity_id}",
            post(handle_commit),
        )
        // Revert
        .route("/revert/{universe}", post(handle_revert))
        // History
        .route(
            "/history/{universe}/{entity_type}/{entity_id}",
            get(handle_history),
        )
        // JSON Schema
        .route("/schema/{name}", get(handle_schema))
        // List universes
        .route("/universes", get(handle_list_universes))
        // List entities
        .route("/universes/{universe}/entities", get(handle_list_entities))
        // Sync (push to configured remote)
        .route("/sync/{universe}", post(handle_sync))
        // Health
        .route("/health", get(handle_health))
        .layer(CorsLayer::permissive())
        .layer(TraceLayer::new_for_http())
        .with_state(state);

    let port: u16 = std::env::var("NAP_PORT")
        .ok()
        .and_then(|p| p.parse().ok())
        .unwrap_or(3100);

    let addr = SocketAddr::from(([0, 0, 0, 0], port));
    info!("NAP resolver listening on http://{addr}");

    let listener = tokio::net::TcpListener::bind(addr).await.unwrap();
    axum::serve(listener, app).await.unwrap();
}

/// GET /resolve/:universe/:entity_type/:entity_id
async fn handle_resolve(
    State(state): State<Arc<AppState>>,
    Path((universe, entity_type, entity_id)): Path<(String, String, String)>,
    Query(query): Query<ResolveQuery>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<ApiError>)> {
    let uri_str = format!("nap://{universe}/{entity_type}/{entity_id}");
    let options = ResolveOptions {
        branch: query.branch,
        commit: query.commit,
        tag: query.tag,
        path: query.path,
    };

    let resolver = Resolver::new(&state.base_path);
    match resolver.resolve(&uri_str, &options) {
        Ok(ResolveResult::Full(manifest)) => {
            let json = serde_json::to_value(&manifest).map_err(|e| {
                error!(error = %e, "serialization error");
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(ApiError {
                        error: e.to_string(),
                        code: "SERIALIZATION_ERROR".to_string(),
                    }),
                )
            })?;
            Ok(Json(json))
        }
        Ok(ResolveResult::Subtree(value)) => Ok(Json(value)),
        Err(e) => {
            let (status, code) = match &e {
                nap_core::NapError::ManifestNotFound(_) => (StatusCode::NOT_FOUND, "NOT_FOUND"),
                nap_core::NapError::RepositoryNotFound(_) => {
                    (StatusCode::NOT_FOUND, "UNIVERSE_NOT_FOUND")
                }
                nap_core::NapError::UnknownEntityType(_) => {
                    (StatusCode::BAD_REQUEST, "INVALID_ENTITY_TYPE")
                }
                nap_core::NapError::InvalidUri { .. } => (StatusCode::BAD_REQUEST, "INVALID_URI"),
                nap_core::NapError::QueryPathNotFound { .. } => {
                    (StatusCode::NOT_FOUND, "QUERY_PATH_NOT_FOUND")
                }
                _ => (StatusCode::INTERNAL_SERVER_ERROR, "INTERNAL_ERROR"),
            };
            error!(error = %e, uri = %uri_str, "resolve failed");
            Err((
                status,
                Json(ApiError {
                    error: e.to_string(),
                    code: code.to_string(),
                }),
            ))
        }
    }
}

/// POST /commit/:universe/:entity_type/:entity_id
async fn handle_commit(
    State(state): State<Arc<AppState>>,
    Path((universe, entity_type_str, entity_id)): Path<(String, String, String)>,
    Json(body): Json<CommitRequest>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<ApiError>)> {
    let entity_type: EntityType = entity_type_str.parse().map_err(|e: nap_core::NapError| {
        (
            StatusCode::BAD_REQUEST,
            Json(ApiError {
                error: e.to_string(),
                code: "INVALID_ENTITY_TYPE".to_string(),
            }),
        )
    })?;

    let repo_path = state.base_path.join(&universe);
    let repo = Repository::open(&repo_path, Box::new(GitBackend::new())).map_err(|e| {
        (
            StatusCode::NOT_FOUND,
            Json(ApiError {
                error: e.to_string(),
                code: "UNIVERSE_NOT_FOUND".to_string(),
            }),
        )
    })?;

    let mut manifest = repo.read_manifest(entity_type, &entity_id).map_err(|e| {
        (
            StatusCode::NOT_FOUND,
            Json(ApiError {
                error: e.to_string(),
                code: "NOT_FOUND".to_string(),
            }),
        )
    })?;

    // Apply property updates
    let mut changes = Vec::new();
    if let Some(props) = body.properties {
        for (key, value) in props {
            let yaml_value: serde_yaml::Value = serde_json::from_value(value.clone())
                .unwrap_or(serde_yaml::Value::String(value.to_string()));
            manifest.set_property(&key, yaml_value);
            changes.push(Change::set(
                &format!("properties.{key}"),
                None,
                value.to_string(),
            ));
        }
    }

    let commit = repo
        .commit_manifest(&mut manifest, &body.message, &body.author, changes)
        .map_err(|e| {
            error!(error = %e, "commit failed");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ApiError {
                    error: e.to_string(),
                    code: "COMMIT_FAILED".to_string(),
                }),
            )
        })?;

    let response = serde_json::json!({
        "commit_id": commit.id,
        "manifest_hash": commit.manifest_hash,
        "version": manifest.version,
        "message": commit.message,
    });
    Ok(Json(response))
}

/// POST /revert/{universe}
async fn handle_revert(
    State(state): State<Arc<AppState>>,
    Path(universe): Path<String>,
    Json(body): Json<RevertRequest>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<ApiError>)> {
    let repo_path = state.base_path.join(&universe);
    let repo = Repository::open(&repo_path, Box::new(GitBackend::new())).map_err(|e| {
        (
            StatusCode::NOT_FOUND,
            Json(ApiError {
                error: e.to_string(),
                code: "UNIVERSE_NOT_FOUND".to_string(),
            }),
        )
    })?;

    let new_hash = repo
        .revert_commit(&body.commit, &body.author)
        .map_err(|e| {
            error!(error = %e, commit = %body.commit, "revert failed");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ApiError {
                    error: e.to_string(),
                    code: "REVERT_FAILED".to_string(),
                }),
            )
        })?;

    let response = serde_json::json!({
        "reverted_commit": body.commit,
        "new_commit": new_hash,
        "author": body.author,
    });
    Ok(Json(response))
}

/// POST /sync/:universe
async fn handle_sync(
    State(state): State<Arc<AppState>>,
    Path(universe): Path<String>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<ApiError>)> {
    let repo_path = state.base_path.join(&universe);
    let repo = Repository::open(&repo_path, Box::new(GitBackend::new())).map_err(|e| {
        (
            StatusCode::NOT_FOUND,
            Json(ApiError {
                error: e.to_string(),
                code: "UNIVERSE_NOT_FOUND".to_string(),
            }),
        )
    })?;

    // Use the current branch and let git figure out the remote from tracking config.
    // Default to pushing "origin" if no tracking branch is set.
    let branch = repo.vcs().current_branch(&repo.root).ok();
    let branch_str = branch.as_deref().unwrap_or("main");

    repo.push(Some("origin"), Some(branch_str)).map_err(|e| {
        error!(error = %e, universe = %universe, "sync (push) failed");
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ApiError {
                error: e.to_string(),
                code: "SYNC_FAILED".to_string(),
            }),
        )
    })?;

    Ok(Json(serde_json::json!({
        "success": true,
        "universe": universe,
        "remote": "origin",
        "branch": branch_str,
    })))
}

/// GET /history/:universe/:entity_type/:entity_id
async fn handle_history(
    State(state): State<Arc<AppState>>,
    Path((universe, entity_type_str, entity_id)): Path<(String, String, String)>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<ApiError>)> {
    let entity_type: EntityType = entity_type_str.parse().map_err(|e: nap_core::NapError| {
        (
            StatusCode::BAD_REQUEST,
            Json(ApiError {
                error: e.to_string(),
                code: "INVALID_ENTITY_TYPE".to_string(),
            }),
        )
    })?;

    let repo_path = state.base_path.join(&universe);
    let repo = Repository::open(&repo_path, Box::new(GitBackend::new())).map_err(|e| {
        (
            StatusCode::NOT_FOUND,
            Json(ApiError {
                error: e.to_string(),
                code: "UNIVERSE_NOT_FOUND".to_string(),
            }),
        )
    })?;

    let history = repo.history(entity_type, &entity_id, 50).map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ApiError {
                error: e.to_string(),
                code: "HISTORY_FAILED".to_string(),
            }),
        )
    })?;

    Ok(Json(serde_json::to_value(&history).unwrap()))
}

/// GET /universes
async fn handle_list_universes(
    State(state): State<Arc<AppState>>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<ApiError>)> {
    let resolver = Resolver::new(&state.base_path);
    let universes = resolver.list_universes().map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ApiError {
                error: e.to_string(),
                code: "LIST_FAILED".to_string(),
            }),
        )
    })?;
    Ok(Json(serde_json::json!({ "universes": universes })))
}

/// GET /universes/:universe/entities
async fn handle_list_entities(
    State(state): State<Arc<AppState>>,
    Path(universe): Path<String>,
    Query(params): Query<std::collections::HashMap<String, String>>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<ApiError>)> {
    let repo_path = state.base_path.join(&universe);
    let repo = Repository::open(&repo_path, Box::new(GitBackend::new())).map_err(|e| {
        (
            StatusCode::NOT_FOUND,
            Json(ApiError {
                error: e.to_string(),
                code: "UNIVERSE_NOT_FOUND".to_string(),
            }),
        )
    })?;

    let mut result = serde_json::Map::new();
    let type_filter = params.get("type");

    for et in EntityType::subdirectory_types() {
        if let Some(filter) = type_filter
            && et.to_string() != *filter
        {
            continue;
        }
        let entities = repo.list_entities(*et).unwrap_or_default();
        let uris: Vec<String> = entities
            .iter()
            .map(|e| format!("nap://{universe}/{et}/{e}"))
            .collect();
        result.insert(et.to_string(), serde_json::json!(uris));
    }

    Ok(Json(serde_json::Value::Object(result)))
}

/// GET /health
async fn handle_health() -> Json<serde_json::Value> {
    Json(serde_json::json!({
        "status": "ok",
        "protocol": "NAP",
        "version": "0.1.0",
    }))
}

/// GET /schema/:name
async fn handle_schema(
    Path(name): Path<String>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<ApiError>)> {
    match name.as_str() {
        "manifest" => Ok(Json(schema::manifest_schema())),
        "commit" => Ok(Json(schema::commit_schema())),
        _ => Err((
            StatusCode::NOT_FOUND,
            Json(ApiError {
                error: format!("unknown schema '{name}'. Available: 'manifest', 'commit'"),
                code: "SCHEMA_NOT_FOUND".to_string(),
            }),
        )),
    }
}
