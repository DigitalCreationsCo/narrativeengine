#![allow(
    clippy::useless_conversion,
    unsafe_op_in_unsafe_fn,
    deprecated,
    unused_imports
)]

use pyo3::exceptions::PyValueError;
use pyo3::prelude::*;
use std::path::Path;
use std::sync::OnceLock;
use tokio::runtime::Runtime;

use nap_core::{
    commit::{Change, ChangeOp, Commit},
    content::ContentHash,
    manifest::{Manifest, Representation},
    query::ManifestQuery,
    repository::Repository,
    resolver::{ResolveOptions, ResolveResult, Resolver},
    schema,
    types::EntityType,
    uri::NapUri,
    vcs_lore::LoreBackend,
};

// ── Tokio Runtime (lazy global) ─────────────────────────────────────
//
// The Python API is synchronous, but the storage engine is async.
// We maintain a single Tokio runtime for the entire module and release
// the Python GIL before blocking on it — essential for preventing
// deadlocks in FastAPI / any multi-threaded Python application.

static RUNTIME: OnceLock<Runtime> = OnceLock::new();

fn get_runtime() -> &'static Runtime {
    RUNTIME.get_or_init(|| Runtime::new().expect("failed to create Tokio runtime for nap-sdk-py"))
}

// ── Error mapping ───────────────────────────────────────────────────

fn map_error(error: nap_core::NapError) -> PyErr {
    PyValueError::new_err(error.to_string())
}

fn map_storage_error(error: nap_core::storage::StorageError) -> PyErr {
    PyValueError::new_err(error.to_string())
}

/// Helper: parse an entity type string, returning a PyErr on failure.
fn parse_entity_type(s: &str) -> Result<EntityType, PyErr> {
    s.parse()
        .map_err(|e: nap_core::NapError| PyValueError::new_err(e.to_string()))
}

/// Helper: open a repository at base_path/universe.
fn open_repo(base_path: &str, universe: &str) -> Result<Repository, PyErr> {
    let repo_path = Path::new(base_path).join(universe);
    Repository::open(&repo_path, Box::new(LoreBackend::from_env())).map_err(map_error)
}

/// Helper: init a repository at base_path/universe.
fn init_repo(base_path: &str, universe: &str) -> Result<Repository, PyErr> {
    Repository::init(
        Path::new(base_path),
        universe,
        Box::new(LoreBackend::from_env()),
    )
    .map_err(map_error)
}

// ═══════════════════════════════════════════════════════════════════════
// URI Operations
// ═══════════════════════════════════════════════════════════════════════

#[pyfunction]
fn parse_uri(uri_str: String) -> PyResult<String> {
    let uri: NapUri = uri_str
        .parse()
        .map_err(|e: nap_core::NapError| PyValueError::new_err(e.to_string()))?;
    serde_json::to_string(&uri).map_err(|e| PyValueError::new_err(e.to_string()))
}

#[pyfunction]
fn uri_new(
    universe: String,
    entity_type: String,
    entity_id: String,
    fragment: Option<String>,
) -> PyResult<String> {
    let et = parse_entity_type(&entity_type)?;
    let uri = match fragment {
        Some(f) => NapUri::with_fragment(universe, et, entity_id, f),
        None => NapUri::new(universe, et, entity_id),
    };
    serde_json::to_string(&uri).map_err(|e| PyValueError::new_err(e.to_string()))
}

#[pyfunction]
fn uri_identity(uri_str: String) -> PyResult<String> {
    let uri: NapUri = uri_str.parse().map_err(map_error)?;
    Ok(uri.identity())
}

#[pyfunction]
fn uri_manifest_path(uri_str: String) -> PyResult<String> {
    let uri: NapUri = uri_str.parse().map_err(map_error)?;
    Ok(uri.manifest_path())
}

/// Format a NapUri from components into a nap:// URI string.
#[pyfunction]
fn uri_format(
    universe: String,
    entity_type: String,
    entity_id: String,
    fragment: Option<String>,
) -> PyResult<String> {
    let et = parse_entity_type(&entity_type)?;
    let uri = match fragment {
        Some(f) => NapUri::with_fragment(universe, et, entity_id, f),
        None => NapUri::new(universe, et, entity_id),
    };
    Ok(uri.to_string())
}

// ═══════════════════════════════════════════════════════════════════════
// EntityType Operations
// ═══════════════════════════════════════════════════════════════════════

#[pyfunction]
fn entity_type_parse(s: String) -> PyResult<String> {
    let et: EntityType = s.parse().map_err(map_error)?;
    serde_json::to_string(&et).map_err(|e| PyValueError::new_err(e.to_string()))
}

#[pyfunction]
fn entity_type_directory_name(entity_type: String) -> PyResult<String> {
    let et = parse_entity_type(&entity_type)?;
    Ok(et.directory_name().to_string())
}

#[pyfunction]
fn entity_type_list() -> PyResult<String> {
    let types: Vec<&str> = EntityType::subdirectory_types()
        .iter()
        .map(|et| match et {
            EntityType::Character => "character",
            EntityType::Location => "location",
            EntityType::Scene => "scene",
            EntityType::Prop => "prop",
            EntityType::World => "world",
        })
        .collect();
    serde_json::to_string(&types).map_err(|e| PyValueError::new_err(e.to_string()))
}

// ═══════════════════════════════════════════════════════════════════════
// Manifest Operations
// ═══════════════════════════════════════════════════════════════════════

#[pyfunction]
fn parse_manifest(yaml_str: String) -> PyResult<String> {
    let manifest: Manifest =
        serde_yaml::from_str(&yaml_str).map_err(|e| PyValueError::new_err(e.to_string()))?;
    serde_json::to_string(&manifest).map_err(|e| PyValueError::new_err(e.to_string()))
}

#[pyfunction]
fn manifest_new(
    universe: String,
    entity_type: String,
    entity_id: String,
    name: String,
) -> PyResult<String> {
    let et = parse_entity_type(&entity_type)?;
    let manifest = Manifest::new(&universe, et, &entity_id, &name);
    serde_json::to_string(&manifest).map_err(|e| PyValueError::new_err(e.to_string()))
}

#[pyfunction]
fn manifest_to_yaml(json_str: String) -> PyResult<String> {
    let manifest: Manifest =
        serde_json::from_str(&json_str).map_err(|e| PyValueError::new_err(e.to_string()))?;
    manifest.to_yaml().map_err(map_error)
}

#[pyfunction]
fn manifest_from_yaml(yaml_str: String) -> PyResult<String> {
    let manifest = Manifest::from_yaml(&yaml_str).map_err(map_error)?;
    serde_json::to_string(&manifest).map_err(|e| PyValueError::new_err(e.to_string()))
}

#[pyfunction]
fn manifest_content_hash(json_str: String) -> PyResult<String> {
    let manifest: Manifest =
        serde_json::from_str(&json_str).map_err(|e| PyValueError::new_err(e.to_string()))?;
    let hash = manifest.content_hash().map_err(map_error)?;
    Ok(hash.as_str().to_string())
}

#[pyfunction]
fn manifest_set_property(json_str: String, key: String, value: String) -> PyResult<String> {
    let mut manifest: Manifest =
        serde_json::from_str(&json_str).map_err(|e| PyValueError::new_err(e.to_string()))?;
    let yaml_value: serde_yaml::Value =
        serde_yaml::from_str(&value).unwrap_or_else(|_| serde_yaml::Value::String(value));
    manifest.set_property(&key, yaml_value);
    serde_json::to_string(&manifest).map_err(|e| PyValueError::new_err(e.to_string()))
}

#[pyfunction]
fn manifest_add_reference(json_str: String, key: String, value: String) -> PyResult<String> {
    let mut manifest: Manifest =
        serde_json::from_str(&json_str).map_err(|e| PyValueError::new_err(e.to_string()))?;
    let yaml_value: serde_yaml::Value =
        serde_yaml::from_str(&value).unwrap_or_else(|_| serde_yaml::Value::String(value));
    manifest.add_reference(&key, yaml_value);
    serde_json::to_string(&manifest).map_err(|e| PyValueError::new_err(e.to_string()))
}

#[pyfunction]
fn manifest_set_representation(
    json_str: String,
    key: String,
    hash: String,
    format: String,
    uri: Option<String>,
    tier: Option<String>,
) -> PyResult<String> {
    let mut manifest: Manifest =
        serde_json::from_str(&json_str).map_err(|e| PyValueError::new_err(e.to_string()))?;
    let repr = Representation {
        hash,
        format,
        uri,
        tier,
    };
    manifest.set_representation(&key, repr);
    serde_json::to_string(&manifest).map_err(|e| PyValueError::new_err(e.to_string()))
}

#[pyfunction]
fn manifest_bump_version(json_str: String) -> PyResult<String> {
    let mut manifest: Manifest =
        serde_json::from_str(&json_str).map_err(|e| PyValueError::new_err(e.to_string()))?;
    manifest.bump_version();
    serde_json::to_string(&manifest).map_err(|e| PyValueError::new_err(e.to_string()))
}

// ═══════════════════════════════════════════════════════════════════════
// ContentHash Operations
// ═══════════════════════════════════════════════════════════════════════

#[pyfunction]
fn content_hash_from_bytes(data: Vec<u8>) -> String {
    ContentHash::from_bytes(&data).to_string()
}

#[pyfunction]
fn content_hash_from_string(s: String) -> String {
    ContentHash::from_str_content(&s).to_string()
}

#[pyfunction]
fn content_hash_parse(s: String) -> PyResult<String> {
    let hash = ContentHash::parse(&s).map_err(map_error)?;
    Ok(hash.to_string())
}

#[pyfunction]
fn content_hash_verify(hash_str: String, data: Vec<u8>) -> PyResult<bool> {
    let hash = ContentHash::parse(&hash_str).map_err(map_error)?;
    match hash.verify(&data) {
        Ok(()) => Ok(true),
        Err(_) => Ok(false),
    }
}

#[pyfunction]
fn content_hash_hex_digest(hash_str: String) -> PyResult<String> {
    let hash = ContentHash::parse(&hash_str).map_err(map_error)?;
    Ok(hash.hex_digest().to_string())
}

// ═══════════════════════════════════════════════════════════════════════
// Commit / Change Operations
// ═══════════════════════════════════════════════════════════════════════

#[pyfunction]
#[pyo3(signature = (path, new_value, old_value=None))]
fn change_set(path: String, new_value: String, old_value: Option<String>) -> PyResult<String> {
    let change = Change::set(&path, old_value, new_value);
    serde_json::to_string(&change).map_err(|e| PyValueError::new_err(e.to_string()))
}

#[pyfunction]
fn change_delete(path: String, old_value: String) -> PyResult<String> {
    let change = Change::delete(&path, old_value);
    serde_json::to_string(&change).map_err(|e| PyValueError::new_err(e.to_string()))
}

#[pyfunction]
fn change_append(path: String, new_value: String) -> PyResult<String> {
    let change = Change::append(&path, new_value);
    serde_json::to_string(&change).map_err(|e| PyValueError::new_err(e.to_string()))
}

#[pyfunction]
#[pyo3(signature = (author, message, manifest_hash, changes_json, parent=None))]
fn commit_new(
    author: String,
    message: String,
    manifest_hash: String,
    changes_json: String,
    parent: Option<String>,
) -> PyResult<String> {
    let changes: Vec<Change> =
        serde_json::from_str(&changes_json).map_err(|e| PyValueError::new_err(e.to_string()))?;
    let commit = Commit::new(parent, &author, &message, &manifest_hash, changes);
    serde_json::to_string(&commit).map_err(|e| PyValueError::new_err(e.to_string()))
}

#[pyfunction]
fn commit_verify_id(json_str: String) -> PyResult<bool> {
    let commit: Commit =
        serde_json::from_str(&json_str).map_err(|e| PyValueError::new_err(e.to_string()))?;
    Ok(commit.verify_id())
}

// ═══════════════════════════════════════════════════════════════════════
// Repository Operations
// ═══════════════════════════════════════════════════════════════════════

#[pyfunction]
fn repo_init(base_path: String, universe: String) -> PyResult<String> {
    let repo = init_repo(&base_path, &universe)?;
    let result = serde_json::json!({
        "root": repo.root.to_string_lossy(),
        "universe": repo.universe,
    });
    Ok(result.to_string())
}

#[pyfunction]
fn repo_open(base_path: String, universe: String) -> PyResult<String> {
    let repo = open_repo(&base_path, &universe)?;
    let result = serde_json::json!({
        "root": repo.root.to_string_lossy(),
        "universe": repo.universe,
    });
    Ok(result.to_string())
}

#[pyfunction]
fn repo_create_entity(
    base_path: String,
    universe: String,
    entity_type: String,
    entity_id: String,
    name: String,
    author: String,
) -> PyResult<String> {
    let et = parse_entity_type(&entity_type)?;
    let repo = open_repo(&base_path, &universe)?;
    let (manifest, commit_hash) = repo
        .create_entity(et, &entity_id, &name, &author)
        .map_err(map_error)?;
    let result = serde_json::json!({
        "manifest": serde_json::to_value(&manifest).map_err(|e| PyValueError::new_err(e.to_string()))?,
        "commit_hash": commit_hash,
    });
    Ok(result.to_string())
}

#[pyfunction]
fn repo_read_manifest(
    base_path: String,
    universe: String,
    entity_type: String,
    entity_id: String,
) -> PyResult<String> {
    let et = parse_entity_type(&entity_type)?;
    let repo = open_repo(&base_path, &universe)?;
    let manifest = repo.read_manifest(et, &entity_id).map_err(map_error)?;
    serde_json::to_string(&manifest).map_err(|e| PyValueError::new_err(e.to_string()))
}

#[pyfunction]
fn repo_read_manifest_at_ref(
    base_path: String,
    universe: String,
    entity_type: String,
    entity_id: String,
    reference: String,
) -> PyResult<String> {
    let et = parse_entity_type(&entity_type)?;
    let repo = open_repo(&base_path, &universe)?;
    let manifest = repo
        .read_manifest_at_ref(et, &entity_id, &reference)
        .map_err(map_error)?;
    serde_json::to_string(&manifest).map_err(|e| PyValueError::new_err(e.to_string()))
}

#[pyfunction]
fn repo_write_manifest(
    base_path: String,
    universe: String,
    manifest_json: String,
) -> PyResult<String> {
    let manifest: Manifest =
        serde_json::from_str(&manifest_json).map_err(|e| PyValueError::new_err(e.to_string()))?;
    let repo = open_repo(&base_path, &universe)?;
    let path = repo.write_manifest(&manifest).map_err(map_error)?;
    Ok(path.to_string_lossy().to_string())
}

#[pyfunction]
fn repo_commit_manifest(
    base_path: String,
    universe: String,
    entity_type: String,
    entity_id: String,
    message: String,
    author: String,
    changes_json: String,
) -> PyResult<String> {
    let et = parse_entity_type(&entity_type)?;
    let repo = open_repo(&base_path, &universe)?;
    let mut manifest = repo.read_manifest(et, &entity_id).map_err(map_error)?;
    let changes: Vec<Change> =
        serde_json::from_str(&changes_json).map_err(|e| PyValueError::new_err(e.to_string()))?;
    let commit = repo
        .commit_manifest(&mut manifest, &message, &author, changes)
        .map_err(map_error)?;
    let result = serde_json::json!({
        "commit": serde_json::to_value(&commit).map_err(|e| PyValueError::new_err(e.to_string()))?,
        "version": manifest.version,
    });
    Ok(result.to_string())
}

#[pyfunction]
fn repo_delete_entity(
    base_path: String,
    universe: String,
    entity_type: String,
    entity_id: String,
    author: String,
) -> PyResult<String> {
    let et = parse_entity_type(&entity_type)?;
    let repo = open_repo(&base_path, &universe)?;
    let hash = repo
        .delete_entity(et, &entity_id, &author)
        .map_err(map_error)?;
    Ok(hash)
}

#[pyfunction]
fn repo_history(
    base_path: String,
    universe: String,
    entity_type: String,
    entity_id: String,
    limit: usize,
) -> PyResult<String> {
    let et = parse_entity_type(&entity_type)?;
    let repo = open_repo(&base_path, &universe)?;
    let history = repo.history(et, &entity_id, limit).map_err(map_error)?;
    serde_json::to_string(&history).map_err(|e| PyValueError::new_err(e.to_string()))
}

#[pyfunction]
fn repo_list_entities(
    base_path: String,
    universe: String,
    entity_type: String,
) -> PyResult<String> {
    let et = parse_entity_type(&entity_type)?;
    let repo = open_repo(&base_path, &universe)?;
    let entities = repo.list_entities(et).map_err(map_error)?;
    serde_json::to_string(&entities).map_err(|e| PyValueError::new_err(e.to_string()))
}

#[pyfunction]
fn repo_create_branch(base_path: String, universe: String, name: String) -> PyResult<String> {
    let repo = open_repo(&base_path, &universe)?;
    repo.create_branch(&name).map_err(map_error)?;
    Ok(serde_json::json!({"success": true, "branch": name}).to_string())
}

#[pyfunction]
fn repo_switch_branch(base_path: String, universe: String, name: String) -> PyResult<String> {
    let repo = open_repo(&base_path, &universe)?;
    repo.switch_branch(&name).map_err(map_error)?;
    Ok(serde_json::json!({"success": true, "branch": name}).to_string())
}

#[pyfunction]
fn repo_list_branches(base_path: String, universe: String) -> PyResult<String> {
    let repo = open_repo(&base_path, &universe)?;
    let branches = repo.list_branches().map_err(map_error)?;
    serde_json::to_string(&branches).map_err(|e| PyValueError::new_err(e.to_string()))
}

#[pyfunction]
fn repo_create_tag(base_path: String, universe: String, name: String) -> PyResult<String> {
    let repo = open_repo(&base_path, &universe)?;
    repo.create_tag(&name).map_err(map_error)?;
    Ok(serde_json::json!({"success": true, "tag": name}).to_string())
}

#[pyfunction]
fn repo_list_tags(base_path: String, universe: String) -> PyResult<String> {
    let repo = open_repo(&base_path, &universe)?;
    let tags = repo.list_tags().map_err(map_error)?;
    serde_json::to_string(&tags).map_err(|e| PyValueError::new_err(e.to_string()))
}

#[pyfunction]
fn repo_head_hash(base_path: String, universe: String) -> PyResult<String> {
    let repo = open_repo(&base_path, &universe)?;
    let hash = repo.head_hash().map_err(map_error)?;
    Ok(hash)
}

#[pyfunction]
fn repo_revert_commit(
    base_path: String,
    universe: String,
    commit_hash: String,
    author: String,
) -> PyResult<String> {
    let repo = open_repo(&base_path, &universe)?;
    let new_hash = repo
        .revert_commit(&commit_hash, &author)
        .map_err(map_error)?;
    Ok(new_hash)
}

#[pyfunction]
fn repo_add_remote(
    base_path: String,
    universe: String,
    name: String,
    url: String,
) -> PyResult<String> {
    let repo = open_repo(&base_path, &universe)?;
    repo.add_remote(&name, &url).map_err(map_error)?;
    Ok(serde_json::json!({"success": true, "remote": name, "url": url}).to_string())
}

#[pyfunction]
fn repo_remove_remote(base_path: String, universe: String, name: String) -> PyResult<String> {
    let repo = open_repo(&base_path, &universe)?;
    repo.remove_remote(&name).map_err(map_error)?;
    Ok(serde_json::json!({"success": true, "remote": name}).to_string())
}

#[pyfunction]
fn repo_list_remotes(base_path: String, universe: String) -> PyResult<String> {
    let repo = open_repo(&base_path, &universe)?;
    let remotes = repo.list_remotes().map_err(map_error)?;
    serde_json::to_string(&remotes).map_err(|e| PyValueError::new_err(e.to_string()))
}

#[pyfunction]
fn repo_push(
    base_path: String,
    universe: String,
    remote: Option<String>,
    branch: Option<String>,
) -> PyResult<String> {
    let repo = open_repo(&base_path, &universe)?;
    repo.push(remote.as_deref(), branch.as_deref())
        .map_err(map_error)?;
    Ok(serde_json::json!({"success": true}).to_string())
}

#[pyfunction]
fn repo_pull(
    base_path: String,
    universe: String,
    remote: Option<String>,
    branch: Option<String>,
) -> PyResult<String> {
    let repo = open_repo(&base_path, &universe)?;
    repo.pull(remote.as_deref(), branch.as_deref())
        .map_err(map_error)?;
    Ok(serde_json::json!({"success": true}).to_string())
}

// ═══════════════════════════════════════════════════════════════════════
// Resolver Operations
// ═══════════════════════════════════════════════════════════════════════

#[pyfunction]
fn resolve(uri_str: String, repo_base_path: String) -> PyResult<String> {
    let resolver = Resolver::new(Path::new(&repo_base_path));
    let result = resolver
        .resolve(&uri_str, &ResolveOptions::default())
        .map_err(map_error)?;
    match result {
        ResolveResult::Full(manifest) => {
            serde_json::to_string(&manifest).map_err(|e| PyValueError::new_err(e.to_string()))
        }
        ResolveResult::Subtree(value) => Ok(value.to_string()),
    }
}

#[pyfunction]
fn resolve_with_options(
    uri_str: String,
    repo_base_path: String,
    branch: Option<String>,
    commit: Option<String>,
    tag: Option<String>,
    path: Option<String>,
) -> PyResult<String> {
    let resolver = Resolver::new(Path::new(&repo_base_path));
    let options = ResolveOptions {
        branch,
        commit,
        tag,
        path,
    };
    let result = resolver.resolve(&uri_str, &options).map_err(map_error)?;
    match result {
        ResolveResult::Full(manifest) => {
            serde_json::to_string(&manifest).map_err(|e| PyValueError::new_err(e.to_string()))
        }
        ResolveResult::Subtree(value) => Ok(value.to_string()),
    }
}

#[pyfunction]
fn resolve_query(uri_str: String, repo_base_path: String, path: String) -> PyResult<String> {
    let resolver = Resolver::new(Path::new(&repo_base_path));
    let result = resolver.query(&uri_str, &path).map_err(map_error)?;
    Ok(result.to_string())
}

#[pyfunction]
fn list_universes(repo_base_path: String) -> PyResult<String> {
    let resolver = Resolver::new(Path::new(&repo_base_path));
    let universes = resolver.list_universes().map_err(map_error)?;
    serde_json::to_string(&universes).map_err(|e| PyValueError::new_err(e.to_string()))
}

// ═══════════════════════════════════════════════════════════════════════
// Schema Operations
// ═══════════════════════════════════════════════════════════════════════

#[pyfunction]
fn manifest_schema_json() -> PyResult<String> {
    let schema = schema::manifest_schema();
    serde_json::to_string(&schema).map_err(|e| PyValueError::new_err(e.to_string()))
}

#[pyfunction]
fn commit_schema_json() -> PyResult<String> {
    let schema = schema::commit_schema();
    serde_json::to_string(&schema).map_err(|e| PyValueError::new_err(e.to_string()))
}

#[pyfunction]
fn validate_manifest(json_str: String) -> PyResult<String> {
    let manifest: Manifest =
        serde_json::from_str(&json_str).map_err(|e| PyValueError::new_err(e.to_string()))?;
    match schema::validate_manifest(&manifest) {
        Ok(()) => Ok(serde_json::json!({"valid": true}).to_string()),
        Err(errors) => Ok(serde_json::json!({"valid": false, "errors": errors}).to_string()),
    }
}

#[pyfunction]
fn validate_commit(json_str: String) -> PyResult<String> {
    let commit: Commit =
        serde_json::from_str(&json_str).map_err(|e| PyValueError::new_err(e.to_string()))?;
    match schema::validate_commit(&commit) {
        Ok(()) => Ok(serde_json::json!({"valid": true}).to_string()),
        Err(errors) => Ok(serde_json::json!({"valid": false, "errors": errors}).to_string()),
    }
}

// ═══════════════════════════════════════════════════════════════════════
// Merge Engine Operations
// ═══════════════════════════════════════════════════════════════════════
//
// These are stateless functions — the MergeEngine is constructed and
// destroyed within each call, avoiding serialization of non-Send types.

#[pyfunction]
fn merge_merge(
    schema_json: String,
    base: String,
    current: String,
    proposed: String,
) -> PyResult<String> {
    let sdl: nap_core::merge::sdl::SdlDocument =
        serde_json::from_str(&schema_json).map_err(|e| PyValueError::new_err(e.to_string()))?;
    let engine = nap_core::merge::merge_engine::MergeEngine::new(sdl);

    let base_val: serde_json::Value =
        serde_json::from_str(&base).map_err(|e| PyValueError::new_err(e.to_string()))?;
    let current_val: serde_json::Value =
        serde_json::from_str(&current).map_err(|e| PyValueError::new_err(e.to_string()))?;
    let proposed_val: serde_json::Value =
        serde_json::from_str(&proposed).map_err(|e| PyValueError::new_err(e.to_string()))?;

    let result = engine.merge(base_val, current_val, proposed_val);
    let result_json =
        serde_json::to_value(&result).map_err(|e| PyValueError::new_err(e.to_string()))?;
    Ok(result_json.to_string())
}

#[pyfunction]
fn merge_diff(schema_json: String, base: String, candidate: String) -> PyResult<String> {
    let sdl: nap_core::merge::sdl::SdlDocument =
        serde_json::from_str(&schema_json).map_err(|e| PyValueError::new_err(e.to_string()))?;
    let base_val: serde_json::Value =
        serde_json::from_str(&base).map_err(|e| PyValueError::new_err(e.to_string()))?;
    let candidate_val: serde_json::Value =
        serde_json::from_str(&candidate).map_err(|e| PyValueError::new_err(e.to_string()))?;

    let result = nap_core::merge::diff::diff(&base_val, &candidate_val, &sdl);
    let result_json =
        serde_json::to_value(&result).map_err(|e| PyValueError::new_err(e.to_string()))?;
    Ok(result_json.to_string())
}

// ═══════════════════════════════════════════════════════════════════════
// Storage Engine Operations
// ═══════════════════════════════════════════════════════════════════════

#[pyfunction]
fn storage_config() -> PyResult<String> {
    let engine = nap_core::storage::get_engine().map_err(map_storage_error)?;
    let config = engine.config();
    let result = serde_json::json!({
        "backend": config.backend.to_string(),
        "base_dir": config.base_dir.to_string_lossy(),
        "assets_prefix": config.assets_prefix,
        "bucket": config.bucket,
    });
    Ok(result.to_string())
}

/// Ingest raw media bytes into the content-addressed storage engine.
///
/// This function:
/// 1. Releases the Python GIL before entering the async runtime
///    (prevents FastAPI / multi-threaded deadlocks).
/// 2. Delegates all storage logic to the Rust engine — no Python I/O.
/// 3. Returns the `sha256:<hex>` content hash.
///
/// # Arguments
///
/// * `data` — Raw bytes of the media asset (image, audio, mesh, etc.).
/// * `format` — File extension without a leading dot (e.g. `"png"`,
///   `"jpg"`, `"wav"`, `"glb"`).
///
/// # Returns
///
/// The content-addressed hash string `sha256:<hex>`.
#[pyfunction]
fn ingest_media(py: Python<'_>, data: Vec<u8>, format: String) -> PyResult<String> {
    let engine = nap_core::storage::get_engine().map_err(map_storage_error)?;

    // CRITICAL: Release the Python GIL before blocking on Tokio I/O.
    // If we don't, concurrent Python threads (e.g. FastAPI workers) will
    // deadlock trying to acquire the GIL while we hold it in a blocking I/O
    // call.
    let result = py.allow_threads(|| get_runtime().block_on(engine.ingest_media(&data, &format)));

    result.map_err(map_storage_error)
}

// ═══════════════════════════════════════════════════════════════════════
// VCS / Lore Operations
// ═══════════════════════════════════════════════════════════════════════

#[pyfunction]
fn lore_clone(url: String, dest_path: String) -> PyResult<String> {
    LoreBackend::clone_repo(&url, Path::new(&dest_path))
        .map_err(|e| PyValueError::new_err(e.to_string()))?;
    Ok(serde_json::json!({"success": true, "url": url, "path": dest_path}).to_string())
}

// ═══════════════════════════════════════════════════════════════════════
// Version
// ═══════════════════════════════════════════════════════════════════════

#[pyfunction]
fn version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}

// ═══════════════════════════════════════════════════════════════════════
// Module registration
// ═══════════════════════════════════════════════════════════════════════

#[pymodule]
fn _native(module: &Bound<'_, PyModule>) -> PyResult<()> {
    // URI
    module.add_function(wrap_pyfunction!(parse_uri, module)?)?;
    module.add_function(wrap_pyfunction!(uri_new, module)?)?;
    module.add_function(wrap_pyfunction!(uri_identity, module)?)?;
    module.add_function(wrap_pyfunction!(uri_manifest_path, module)?)?;
    module.add_function(wrap_pyfunction!(uri_format, module)?)?;

    // EntityType
    module.add_function(wrap_pyfunction!(entity_type_parse, module)?)?;
    module.add_function(wrap_pyfunction!(entity_type_directory_name, module)?)?;
    module.add_function(wrap_pyfunction!(entity_type_list, module)?)?;

    // Manifest
    module.add_function(wrap_pyfunction!(parse_manifest, module)?)?;
    module.add_function(wrap_pyfunction!(manifest_new, module)?)?;
    module.add_function(wrap_pyfunction!(manifest_to_yaml, module)?)?;
    module.add_function(wrap_pyfunction!(manifest_from_yaml, module)?)?;
    module.add_function(wrap_pyfunction!(manifest_content_hash, module)?)?;
    module.add_function(wrap_pyfunction!(manifest_set_property, module)?)?;
    module.add_function(wrap_pyfunction!(manifest_add_reference, module)?)?;
    module.add_function(wrap_pyfunction!(manifest_set_representation, module)?)?;
    module.add_function(wrap_pyfunction!(manifest_bump_version, module)?)?;

    // ContentHash
    module.add_function(wrap_pyfunction!(content_hash_from_bytes, module)?)?;
    module.add_function(wrap_pyfunction!(content_hash_from_string, module)?)?;
    module.add_function(wrap_pyfunction!(content_hash_parse, module)?)?;
    module.add_function(wrap_pyfunction!(content_hash_verify, module)?)?;
    module.add_function(wrap_pyfunction!(content_hash_hex_digest, module)?)?;

    // Commit / Change
    module.add_function(wrap_pyfunction!(change_set, module)?)?;
    module.add_function(wrap_pyfunction!(change_delete, module)?)?;
    module.add_function(wrap_pyfunction!(change_append, module)?)?;
    module.add_function(wrap_pyfunction!(commit_new, module)?)?;
    module.add_function(wrap_pyfunction!(commit_verify_id, module)?)?;

    // Repository
    module.add_function(wrap_pyfunction!(repo_init, module)?)?;
    module.add_function(wrap_pyfunction!(repo_open, module)?)?;
    module.add_function(wrap_pyfunction!(repo_create_entity, module)?)?;
    module.add_function(wrap_pyfunction!(repo_read_manifest, module)?)?;
    module.add_function(wrap_pyfunction!(repo_read_manifest_at_ref, module)?)?;
    module.add_function(wrap_pyfunction!(repo_write_manifest, module)?)?;
    module.add_function(wrap_pyfunction!(repo_commit_manifest, module)?)?;
    module.add_function(wrap_pyfunction!(repo_delete_entity, module)?)?;
    module.add_function(wrap_pyfunction!(repo_history, module)?)?;
    module.add_function(wrap_pyfunction!(repo_list_entities, module)?)?;
    module.add_function(wrap_pyfunction!(repo_create_branch, module)?)?;
    module.add_function(wrap_pyfunction!(repo_switch_branch, module)?)?;
    module.add_function(wrap_pyfunction!(repo_list_branches, module)?)?;
    module.add_function(wrap_pyfunction!(repo_create_tag, module)?)?;
    module.add_function(wrap_pyfunction!(repo_list_tags, module)?)?;
    module.add_function(wrap_pyfunction!(repo_head_hash, module)?)?;
    module.add_function(wrap_pyfunction!(repo_revert_commit, module)?)?;
    module.add_function(wrap_pyfunction!(repo_add_remote, module)?)?;
    module.add_function(wrap_pyfunction!(repo_remove_remote, module)?)?;
    module.add_function(wrap_pyfunction!(repo_list_remotes, module)?)?;
    module.add_function(wrap_pyfunction!(repo_push, module)?)?;
    module.add_function(wrap_pyfunction!(repo_pull, module)?)?;

    // Resolver
    module.add_function(wrap_pyfunction!(resolve, module)?)?;
    module.add_function(wrap_pyfunction!(resolve_with_options, module)?)?;
    module.add_function(wrap_pyfunction!(resolve_query, module)?)?;
    module.add_function(wrap_pyfunction!(list_universes, module)?)?;

    // Schema
    module.add_function(wrap_pyfunction!(manifest_schema_json, module)?)?;
    module.add_function(wrap_pyfunction!(commit_schema_json, module)?)?;
    module.add_function(wrap_pyfunction!(validate_manifest, module)?)?;
    module.add_function(wrap_pyfunction!(validate_commit, module)?)?;

    // Merge
    module.add_function(wrap_pyfunction!(merge_merge, module)?)?;
    module.add_function(wrap_pyfunction!(merge_diff, module)?)?;

    // Storage
    module.add_function(wrap_pyfunction!(storage_config, module)?)?;
    module.add_function(wrap_pyfunction!(ingest_media, module)?)?;

    // VCS
    module.add_function(wrap_pyfunction!(lore_clone, module)?)?;

    // Version
    module.add_function(wrap_pyfunction!(version, module)?)?;

    Ok(())
}
