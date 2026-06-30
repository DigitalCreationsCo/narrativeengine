//! NAP HTTP Resolver Server — Axum-based REST API.
//!
//! Endpoints:
//!   GET   /resolve/:universe/:entity_type/:entity_id   — Resolve a manifest
//!   POST  /commit/:universe/:entity_type/:entity_id    — Commit changes
//!   POST  /create/:universe/:entity_type/:entity_id    — Create entity
//!   DELETE /:universe/:entity_type/:entity_id          — Delete entity
//!   POST  /revert/:universe                            — Revert a commit
//!   GET   /history/:universe/:entity_type/:entity_id   — Get commit history
//!   GET   /schema/{name}                               — Get JSON Schema for a type
//!   GET   /universes                                   — List all universes
//!   GET   /universes/:universe/entities                 — List entities in a universe
//!   POST  /init/:universe                              — Initialize a universe
//!   POST  /switch/:universe                            — Switch to a branch
//!   GET   /head/:universe                              — Get HEAD commit hash
//!   GET   /branches/:universe                          — List branches
//!   POST  /branches/:universe                          — Create a branch
//!   GET   /tags/:universe                              — List tags
//!   POST  /tags/:universe                              — Create a tag
//!   GET   /remotes/:universe                           — List remotes
//!   POST  /remotes/:universe                           — Add a remote
//!   DELETE /remotes/:universe/:name                    — Remove a remote
//!   POST  /pull/:universe                              — Pull from remote
//!   POST  /push/:universe                              — Push to remote
//!   POST  /sync/:universe                              — Push configured remote
//!   POST  /content-hash                                — Compute content hash
//!   GET   /validate/:universe/:entity_type/:entity_id  — Validate a manifest
//!   GET   /health                                      — Health check
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
    routing::{delete, get, post},
};
use serde::{Deserialize, Serialize};
use tower_http::cors::CorsLayer;
use tower_http::trace::TraceLayer;
use tracing::{error, info};

use nap_core::{
    commit::Change,
    content::ContentHash,
    repository::Repository,
    resolver::{ResolveOptions, ResolveResult, Resolver},
    schema,
    types::EntityType,
    vcs_lore::LoreBackend,
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

/// Request body for entity creation.
#[derive(Debug, Deserialize)]
struct CreateRequest {
    name: String,
    author: String,
}

/// Request body for entity deletion.
#[derive(Debug, Deserialize)]
struct DeleteRequest {
    author: String,
}

/// Request body for branch/tag creation.
#[derive(Debug, Deserialize)]
struct BranchTagRequest {
    name: String,
}

/// Request body for branch switch.
#[derive(Debug, Deserialize)]
struct SwitchRequest {
    name: String,
}

/// Request body for remote add.
#[derive(Debug, Deserialize)]
struct RemoteAddRequest {
    name: String,
    url: String,
}

/// Request body for push/pull.
#[derive(Debug, Deserialize)]
struct PushPullRequest {
    remote: Option<String>,
    branch: Option<String>,
}

/// Request body for content hash computation.
#[derive(Debug, Deserialize)]
struct ContentHashRequest {
    data: String, // base64-encoded data
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
        // Create entity
        .route(
            "/create/{universe}/{entity_type}/{entity_id}",
            post(handle_create),
        )
        // Delete entity
        .route(
            "/{universe}/{entity_type}/{entity_id}",
            delete(handle_delete),
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
        // Init universe
        .route("/init/{universe}", post(handle_init))
        // Switch branch
        .route("/switch/{universe}", post(handle_switch_branch))
        // Get HEAD hash
        .route("/head/{universe}", get(handle_head_hash))
        // Branches
        .route(
            "/branches/{universe}",
            get(handle_list_branches).post(handle_create_branch),
        )
        // Tags
        .route(
            "/tags/{universe}",
            get(handle_list_tags).post(handle_create_tag),
        )
        // Remotes
        .route(
            "/remotes/{universe}",
            get(handle_list_remotes).post(handle_add_remote),
        )
        .route("/remotes/{universe}/{name}", delete(handle_remove_remote))
        // Pull
        .route("/pull/{universe}", post(handle_pull))
        // Push
        .route("/push/{universe}", post(handle_push))
        // Sync (push to configured remote)
        .route("/sync/{universe}", post(handle_sync))
        // Content hash
        .route("/content-hash", post(handle_content_hash))
        // Validate
        .route(
            "/validate/{universe}/{entity_type}/{entity_id}",
            get(handle_validate),
        )
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

/// POST /init/:universe
async fn handle_init(
    State(state): State<Arc<AppState>>,
    Path(universe): Path<String>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<ApiError>)> {
    let repo = Repository::init(
        &state.base_path,
        &universe,
        Box::new(LoreBackend::from_env()),
    )
    .map_err(|e| {
        let (status, code) = match &e {
            nap_core::NapError::RepositoryAlreadyExists(_) => {
                (StatusCode::CONFLICT, "ALREADY_EXISTS")
            }
            _ => (StatusCode::INTERNAL_SERVER_ERROR, "INIT_FAILED"),
        };
        error!(error = %e, universe = %universe, "init failed");
        (
            status,
            Json(ApiError {
                error: e.to_string(),
                code: code.to_string(),
            }),
        )
    })?;

    Ok(Json(serde_json::json!({
        "success": true,
        "universe": universe,
        "path": repo.root.to_string_lossy(),
    })))
}

/// POST /create/:universe/:entity_type/:entity_id
async fn handle_create(
    State(state): State<Arc<AppState>>,
    Path((universe, entity_type_str, entity_id)): Path<(String, String, String)>,
    Json(body): Json<CreateRequest>,
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
    let repo = Repository::open(&repo_path, Box::new(LoreBackend::from_env())).map_err(|e| {
        (
            StatusCode::NOT_FOUND,
            Json(ApiError {
                error: e.to_string(),
                code: "UNIVERSE_NOT_FOUND".to_string(),
            }),
        )
    })?;

    let (manifest, commit_hash) = repo
        .create_entity(entity_type, &entity_id, &body.name, &body.author)
        .map_err(|e| {
            error!(error = %e, "create entity failed");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ApiError {
                    error: e.to_string(),
                    code: "CREATE_FAILED".to_string(),
                }),
            )
        })?;

    Ok(Json(serde_json::json!({
        "success": true,
        "uri": manifest.id,
        "commit_id": commit_hash,
        "version": manifest.version,
    })))
}

/// DELETE /:universe/:entity_type/:entity_id
async fn handle_delete(
    State(state): State<Arc<AppState>>,
    Path((universe, entity_type_str, entity_id)): Path<(String, String, String)>,
    Json(body): Json<DeleteRequest>,
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
    let repo = Repository::open(&repo_path, Box::new(LoreBackend::from_env())).map_err(|e| {
        (
            StatusCode::NOT_FOUND,
            Json(ApiError {
                error: e.to_string(),
                code: "UNIVERSE_NOT_FOUND".to_string(),
            }),
        )
    })?;

    let commit_hash = repo
        .delete_entity(entity_type, &entity_id, &body.author)
        .map_err(|e| {
            error!(error = %e, "delete entity failed");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ApiError {
                    error: e.to_string(),
                    code: "DELETE_FAILED".to_string(),
                }),
            )
        })?;

    Ok(Json(serde_json::json!({
        "success": true,
        "commit_id": commit_hash,
    })))
}

/// POST /switch/:universe
async fn handle_switch_branch(
    State(state): State<Arc<AppState>>,
    Path(universe): Path<String>,
    Json(body): Json<SwitchRequest>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<ApiError>)> {
    let repo_path = state.base_path.join(&universe);
    let repo = Repository::open(&repo_path, Box::new(LoreBackend::from_env())).map_err(|e| {
        (
            StatusCode::NOT_FOUND,
            Json(ApiError {
                error: e.to_string(),
                code: "UNIVERSE_NOT_FOUND".to_string(),
            }),
        )
    })?;

    repo.switch_branch(&body.name).map_err(|e| {
        error!(error = %e, branch = %body.name, "switch branch failed");
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ApiError {
                error: e.to_string(),
                code: "SWITCH_FAILED".to_string(),
            }),
        )
    })?;

    Ok(Json(serde_json::json!({
        "success": true,
        "branch": body.name,
    })))
}

/// GET /head/:universe
async fn handle_head_hash(
    State(state): State<Arc<AppState>>,
    Path(universe): Path<String>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<ApiError>)> {
    let repo_path = state.base_path.join(&universe);
    let repo = Repository::open(&repo_path, Box::new(LoreBackend::from_env())).map_err(|e| {
        (
            StatusCode::NOT_FOUND,
            Json(ApiError {
                error: e.to_string(),
                code: "UNIVERSE_NOT_FOUND".to_string(),
            }),
        )
    })?;

    let hash = repo.head_hash().map_err(|e| {
        error!(error = %e, "head hash failed");
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ApiError {
                error: e.to_string(),
                code: "HEAD_HASH_FAILED".to_string(),
            }),
        )
    })?;

    Ok(Json(serde_json::json!({
        "universe": universe,
        "head": hash,
    })))
}

/// GET /branches/:universe
async fn handle_list_branches(
    State(state): State<Arc<AppState>>,
    Path(universe): Path<String>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<ApiError>)> {
    let repo_path = state.base_path.join(&universe);
    let repo = Repository::open(&repo_path, Box::new(LoreBackend::from_env())).map_err(|e| {
        (
            StatusCode::NOT_FOUND,
            Json(ApiError {
                error: e.to_string(),
                code: "UNIVERSE_NOT_FOUND".to_string(),
            }),
        )
    })?;

    let branches = repo.list_branches().map_err(|e| {
        error!(error = %e, "list branches failed");
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ApiError {
                error: e.to_string(),
                code: "LIST_BRANCHES_FAILED".to_string(),
            }),
        )
    })?;

    Ok(Json(serde_json::json!({
        "universe": universe,
        "branches": branches,
    })))
}

/// POST /branches/:universe
async fn handle_create_branch(
    State(state): State<Arc<AppState>>,
    Path(universe): Path<String>,
    Json(body): Json<BranchTagRequest>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<ApiError>)> {
    let repo_path = state.base_path.join(&universe);
    let repo = Repository::open(&repo_path, Box::new(LoreBackend::from_env())).map_err(|e| {
        (
            StatusCode::NOT_FOUND,
            Json(ApiError {
                error: e.to_string(),
                code: "UNIVERSE_NOT_FOUND".to_string(),
            }),
        )
    })?;

    repo.create_branch(&body.name).map_err(|e| {
        error!(error = %e, branch = %body.name, "create branch failed");
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ApiError {
                error: e.to_string(),
                code: "CREATE_BRANCH_FAILED".to_string(),
            }),
        )
    })?;

    Ok(Json(serde_json::json!({
        "success": true,
        "branch": body.name,
    })))
}

/// GET /tags/:universe
async fn handle_list_tags(
    State(state): State<Arc<AppState>>,
    Path(universe): Path<String>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<ApiError>)> {
    // To avoid conflicting with POST /tags/:universe body parsing,
    // we use a function-level approach
    let repo_path = state.base_path.join(&universe);
    let repo = match Repository::open(&repo_path, Box::new(LoreBackend::from_env())) {
        Ok(r) => r,
        Err(e) => {
            return Err((
                StatusCode::NOT_FOUND,
                Json(ApiError {
                    error: e.to_string(),
                    code: "UNIVERSE_NOT_FOUND".to_string(),
                }),
            ));
        }
    };

    let tags = match repo.list_tags() {
        Ok(t) => t,
        Err(e) => {
            error!(error = %e, "list tags failed");
            return Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ApiError {
                    error: e.to_string(),
                    code: "LIST_TAGS_FAILED".to_string(),
                }),
            ));
        }
    };

    Ok(Json(serde_json::json!({
        "universe": universe,
        "tags": tags,
    })))
}

/// POST /tags/:universe
async fn handle_create_tag(
    State(state): State<Arc<AppState>>,
    Path(universe): Path<String>,
    Json(body): Json<BranchTagRequest>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<ApiError>)> {
    let repo_path = state.base_path.join(&universe);
    let repo = Repository::open(&repo_path, Box::new(LoreBackend::from_env())).map_err(|e| {
        (
            StatusCode::NOT_FOUND,
            Json(ApiError {
                error: e.to_string(),
                code: "UNIVERSE_NOT_FOUND".to_string(),
            }),
        )
    })?;

    repo.create_tag(&body.name).map_err(|e| {
        error!(error = %e, tag = %body.name, "create tag failed");
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ApiError {
                error: e.to_string(),
                code: "CREATE_TAG_FAILED".to_string(),
            }),
        )
    })?;

    Ok(Json(serde_json::json!({
        "success": true,
        "tag": body.name,
    })))
}

/// GET /remotes/:universe
async fn handle_list_remotes(
    State(state): State<Arc<AppState>>,
    Path(universe): Path<String>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<ApiError>)> {
    let repo_path = state.base_path.join(&universe);
    let repo = Repository::open(&repo_path, Box::new(LoreBackend::from_env())).map_err(|e| {
        (
            StatusCode::NOT_FOUND,
            Json(ApiError {
                error: e.to_string(),
                code: "UNIVERSE_NOT_FOUND".to_string(),
            }),
        )
    })?;

    let remotes = repo.list_remotes().map_err(|e| {
        error!(error = %e, "list remotes failed");
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ApiError {
                error: e.to_string(),
                code: "LIST_REMOTES_FAILED".to_string(),
            }),
        )
    })?;

    let pairs: Vec<serde_json::Value> = remotes
        .iter()
        .map(|(n, u)| serde_json::json!({ "name": n, "url": u }))
        .collect();

    Ok(Json(serde_json::json!({
        "universe": universe,
        "remotes": pairs,
    })))
}

/// POST /remotes/:universe
async fn handle_add_remote(
    State(state): State<Arc<AppState>>,
    Path(universe): Path<String>,
    Json(body): Json<RemoteAddRequest>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<ApiError>)> {
    let repo_path = state.base_path.join(&universe);
    let repo = Repository::open(&repo_path, Box::new(LoreBackend::from_env())).map_err(|e| {
        (
            StatusCode::NOT_FOUND,
            Json(ApiError {
                error: e.to_string(),
                code: "UNIVERSE_NOT_FOUND".to_string(),
            }),
        )
    })?;

    repo.add_remote(&body.name, &body.url).map_err(|e| {
        error!(error = %e, remote = %body.name, "add remote failed");
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ApiError {
                error: e.to_string(),
                code: "ADD_REMOTE_FAILED".to_string(),
            }),
        )
    })?;

    Ok(Json(serde_json::json!({
        "success": true,
        "remote": body.name,
        "url": body.url,
    })))
}

/// DELETE /remotes/:universe/:name
async fn handle_remove_remote(
    State(state): State<Arc<AppState>>,
    Path((universe, name)): Path<(String, String)>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<ApiError>)> {
    let repo_path = state.base_path.join(&universe);
    let repo = Repository::open(&repo_path, Box::new(LoreBackend::from_env())).map_err(|e| {
        (
            StatusCode::NOT_FOUND,
            Json(ApiError {
                error: e.to_string(),
                code: "UNIVERSE_NOT_FOUND".to_string(),
            }),
        )
    })?;

    repo.remove_remote(&name).map_err(|e| {
        error!(error = %e, remote = %name, "remove remote failed");
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ApiError {
                error: e.to_string(),
                code: "REMOVE_REMOTE_FAILED".to_string(),
            }),
        )
    })?;

    Ok(Json(serde_json::json!({
        "success": true,
        "removed": name,
    })))
}

/// POST /pull/:universe
async fn handle_pull(
    State(state): State<Arc<AppState>>,
    Path(universe): Path<String>,
    Json(body): Json<PushPullRequest>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<ApiError>)> {
    let repo_path = state.base_path.join(&universe);
    let repo = Repository::open(&repo_path, Box::new(LoreBackend::from_env())).map_err(|e| {
        (
            StatusCode::NOT_FOUND,
            Json(ApiError {
                error: e.to_string(),
                code: "UNIVERSE_NOT_FOUND".to_string(),
            }),
        )
    })?;

    repo.pull(body.remote.as_deref(), body.branch.as_deref())
        .map_err(|e| {
            error!(error = %e, "pull failed");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ApiError {
                    error: e.to_string(),
                    code: "PULL_FAILED".to_string(),
                }),
            )
        })?;

    Ok(Json(serde_json::json!({
        "success": true,
        "universe": universe,
    })))
}

/// POST /push/:universe
async fn handle_push(
    State(state): State<Arc<AppState>>,
    Path(universe): Path<String>,
    Json(body): Json<PushPullRequest>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<ApiError>)> {
    let repo_path = state.base_path.join(&universe);
    let repo = Repository::open(&repo_path, Box::new(LoreBackend::from_env())).map_err(|e| {
        (
            StatusCode::NOT_FOUND,
            Json(ApiError {
                error: e.to_string(),
                code: "UNIVERSE_NOT_FOUND".to_string(),
            }),
        )
    })?;

    repo.push(body.remote.as_deref(), body.branch.as_deref())
        .map_err(|e| {
            error!(error = %e, "push failed");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ApiError {
                    error: e.to_string(),
                    code: "PUSH_FAILED".to_string(),
                }),
            )
        })?;

    Ok(Json(serde_json::json!({
        "success": true,
        "universe": universe,
    })))
}

/// POST /content-hash
async fn handle_content_hash(
    Json(body): Json<ContentHashRequest>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<ApiError>)> {
    use base64::Engine;

    let bytes = base64::engine::general_purpose::STANDARD
        .decode(&body.data)
        .map_err(|e| {
            (
                StatusCode::BAD_REQUEST,
                Json(ApiError {
                    error: format!("invalid base64: {e}"),
                    code: "INVALID_BASE64".to_string(),
                }),
            )
        })?;

    let hash = ContentHash::from_bytes(&bytes);
    Ok(Json(serde_json::json!({
        "hash": hash.as_str(),
        "algorithm": "sha256",
    })))
}

/// GET /validate/:universe/:entity_type/:entity_id
async fn handle_validate(
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
    let repo = Repository::open(&repo_path, Box::new(LoreBackend::from_env())).map_err(|e| {
        (
            StatusCode::NOT_FOUND,
            Json(ApiError {
                error: e.to_string(),
                code: "UNIVERSE_NOT_FOUND".to_string(),
            }),
        )
    })?;

    let manifest = repo.read_manifest(entity_type, &entity_id).map_err(|e| {
        (
            StatusCode::NOT_FOUND,
            Json(ApiError {
                error: e.to_string(),
                code: "NOT_FOUND".to_string(),
            }),
        )
    })?;

    match nap_core::schema::validate_manifest(&manifest) {
        Ok(()) => Ok(Json(serde_json::json!({
            "valid": true,
            "uri": manifest.id,
            "errors": [],
        }))),
        Err(errors) => Ok(Json(serde_json::json!({
            "valid": false,
            "uri": manifest.id,
            "errors": errors,
        }))),
    }
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
    let repo = Repository::open(&repo_path, Box::new(LoreBackend::from_env())).map_err(|e| {
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
    let repo = Repository::open(&repo_path, Box::new(LoreBackend::from_env())).map_err(|e| {
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
    let repo = Repository::open(&repo_path, Box::new(LoreBackend::from_env())).map_err(|e| {
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
    let repo = Repository::open(&repo_path, Box::new(LoreBackend::from_env())).map_err(|e| {
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
    let repo = Repository::open(&repo_path, Box::new(LoreBackend::from_env())).map_err(|e| {
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
