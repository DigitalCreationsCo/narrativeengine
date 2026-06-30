use napi::Error;
use napi::bindgen_prelude::Buffer;
use napi_derive::napi;
use std::path::Path;

use nap_core::{
    commit::{Change, Commit},
    content::ContentHash,
    manifest::{Manifest, Representation},
    repository::Repository,
    resolver::{ResolveOptions, ResolveResult, Resolver},
    schema,
    types::EntityType,
    uri::NapUri,
    vcs_lore::LoreBackend,
};

// ── Helpers ──────────────────────────────────────────────────────────

fn map_error(e: nap_core::NapError) -> Error {
    Error::from_reason(e.to_string())
}

fn parse_et(s: &str) -> Result<EntityType, Error> {
    s.parse()
        .map_err(|e: nap_core::NapError| Error::from_reason(e.to_string()))
}

fn open_repo(base_path: &str, universe: &str) -> Result<Repository, Error> {
    let repo_path = Path::new(base_path).join(universe);
    Repository::open(&repo_path, Box::new(LoreBackend::from_env())).map_err(map_error)
}

// ═══════════════════════════════════════════════════════════════════════
// URI Operations
// ═══════════════════════════════════════════════════════════════════════

#[napi(js_name = "parseUri")]
pub fn parse_uri(uri_str: String) -> napi::Result<String> {
    let uri: NapUri = uri_str
        .parse()
        .map_err(|e: nap_core::NapError| Error::from_reason(e.to_string()))?;
    serde_json::to_string(&uri).map_err(|e| Error::from_reason(e.to_string()))
}

#[napi(js_name = "uriNew")]
pub fn uri_new(
    universe: String,
    entity_type: String,
    entity_id: String,
    fragment: Option<String>,
) -> napi::Result<String> {
    let et = parse_et(&entity_type)?;
    let uri = match fragment {
        Some(f) => NapUri::with_fragment(universe, et, entity_id, f),
        None => NapUri::new(universe, et, entity_id),
    };
    serde_json::to_string(&uri).map_err(|e| Error::from_reason(e.to_string()))
}

#[napi(js_name = "uriIdentity")]
pub fn uri_identity(uri_str: String) -> napi::Result<String> {
    let uri: NapUri = uri_str.parse().map_err(map_error)?;
    Ok(uri.identity())
}

#[napi(js_name = "uriManifestPath")]
pub fn uri_manifest_path(uri_str: String) -> napi::Result<String> {
    let uri: NapUri = uri_str.parse().map_err(map_error)?;
    Ok(uri.manifest_path())
}

#[napi(js_name = "uriFormat")]
pub fn uri_format(
    universe: String,
    entity_type: String,
    entity_id: String,
    fragment: Option<String>,
) -> napi::Result<String> {
    let et = parse_et(&entity_type)?;
    let uri = match fragment {
        Some(f) => NapUri::with_fragment(universe, et, entity_id, f),
        None => NapUri::new(universe, et, entity_id),
    };
    Ok(uri.to_string())
}

// ═══════════════════════════════════════════════════════════════════════
// EntityType Operations
// ═══════════════════════════════════════════════════════════════════════

#[napi(js_name = "entityTypeParse")]
pub fn entity_type_parse(s: String) -> napi::Result<String> {
    let et: EntityType = s.parse().map_err(map_error)?;
    serde_json::to_string(&et).map_err(|e| Error::from_reason(e.to_string()))
}

#[napi(js_name = "entityTypeDirectoryName")]
pub fn entity_type_directory_name(entity_type: String) -> napi::Result<String> {
    let et = parse_et(&entity_type)?;
    Ok(et.directory_name().to_string())
}

#[napi(js_name = "entityTypeList")]
pub fn entity_type_list() -> napi::Result<String> {
    let types: Vec<&str> = nap_core::types::EntityType::subdirectory_types()
        .iter()
        .map(|et| match et {
            EntityType::Character => "character",
            EntityType::Location => "location",
            EntityType::Scene => "scene",
            EntityType::Prop => "prop",
            EntityType::World => "world",
        })
        .collect();
    serde_json::to_string(&types).map_err(|e| Error::from_reason(e.to_string()))
}

// ═══════════════════════════════════════════════════════════════════════
// Manifest Operations
// ═══════════════════════════════════════════════════════════════════════

#[napi(js_name = "parseManifest")]
pub fn parse_manifest(yaml_str: String) -> napi::Result<String> {
    let manifest: Manifest =
        serde_yaml::from_str(&yaml_str).map_err(|e| Error::from_reason(e.to_string()))?;
    serde_json::to_string(&manifest).map_err(|e| Error::from_reason(e.to_string()))
}

#[napi(js_name = "manifestNew")]
pub fn manifest_new(
    universe: String,
    entity_type: String,
    entity_id: String,
    name: String,
) -> napi::Result<String> {
    let et = parse_et(&entity_type)?;
    let manifest = Manifest::new(&universe, et, &entity_id, &name);
    serde_json::to_string(&manifest).map_err(|e| Error::from_reason(e.to_string()))
}

#[napi(js_name = "manifestToYaml")]
pub fn manifest_to_yaml(json_str: String) -> napi::Result<String> {
    let manifest: Manifest =
        serde_json::from_str(&json_str).map_err(|e| Error::from_reason(e.to_string()))?;
    manifest.to_yaml().map_err(map_error)
}

#[napi(js_name = "manifestFromYaml")]
pub fn manifest_from_yaml(yaml_str: String) -> napi::Result<String> {
    let manifest = Manifest::from_yaml(&yaml_str).map_err(map_error)?;
    serde_json::to_string(&manifest).map_err(|e| Error::from_reason(e.to_string()))
}

#[napi(js_name = "manifestContentHash")]
pub fn manifest_content_hash(json_str: String) -> napi::Result<String> {
    let manifest: Manifest =
        serde_json::from_str(&json_str).map_err(|e| Error::from_reason(e.to_string()))?;
    let hash = manifest.content_hash().map_err(map_error)?;
    Ok(hash.as_str().to_string())
}

#[napi(js_name = "manifestSetProperty")]
pub fn manifest_set_property(json_str: String, key: String, value: String) -> napi::Result<String> {
    let mut manifest: Manifest =
        serde_json::from_str(&json_str).map_err(|e| Error::from_reason(e.to_string()))?;
    let yaml_value: serde_yaml::Value =
        serde_yaml::from_str(&value).unwrap_or_else(|_| serde_yaml::Value::String(value));
    manifest.set_property(&key, yaml_value);
    serde_json::to_string(&manifest).map_err(|e| Error::from_reason(e.to_string()))
}

#[napi(js_name = "manifestAddReference")]
pub fn manifest_add_reference(
    json_str: String,
    key: String,
    value: String,
) -> napi::Result<String> {
    let mut manifest: Manifest =
        serde_json::from_str(&json_str).map_err(|e| Error::from_reason(e.to_string()))?;
    let yaml_value: serde_yaml::Value =
        serde_yaml::from_str(&value).unwrap_or_else(|_| serde_yaml::Value::String(value));
    manifest.add_reference(&key, yaml_value);
    serde_json::to_string(&manifest).map_err(|e| Error::from_reason(e.to_string()))
}

#[napi(js_name = "manifestSetRepresentation")]
pub fn manifest_set_representation(
    json_str: String,
    key: String,
    hash: String,
    format: String,
    uri: Option<String>,
    tier: Option<String>,
) -> napi::Result<String> {
    let mut manifest: Manifest =
        serde_json::from_str(&json_str).map_err(|e| Error::from_reason(e.to_string()))?;
    let repr = Representation {
        hash,
        format,
        uri,
        tier,
    };
    manifest.set_representation(&key, repr);
    serde_json::to_string(&manifest).map_err(|e| Error::from_reason(e.to_string()))
}

#[napi(js_name = "manifestBumpVersion")]
pub fn manifest_bump_version(json_str: String) -> napi::Result<String> {
    let mut manifest: Manifest =
        serde_json::from_str(&json_str).map_err(|e| Error::from_reason(e.to_string()))?;
    manifest.bump_version();
    serde_json::to_string(&manifest).map_err(|e| Error::from_reason(e.to_string()))
}

// ═══════════════════════════════════════════════════════════════════════
// ContentHash Operations
// ═══════════════════════════════════════════════════════════════════════

#[napi(js_name = "contentHashFromBytes")]
pub fn content_hash_from_bytes(data: Buffer) -> String {
    ContentHash::from_bytes(&data).to_string()
}

#[napi(js_name = "contentHashFromString")]
pub fn content_hash_from_string(s: String) -> String {
    ContentHash::from_str_content(&s).to_string()
}

#[napi(js_name = "contentHashParse")]
pub fn content_hash_parse(s: String) -> napi::Result<String> {
    let hash = ContentHash::parse(&s).map_err(map_error)?;
    Ok(hash.to_string())
}

#[napi(js_name = "contentHashVerify")]
pub fn content_hash_verify(hash_str: String, data: Buffer) -> napi::Result<bool> {
    let hash = ContentHash::parse(&hash_str).map_err(map_error)?;
    match hash.verify(&data) {
        Ok(()) => Ok(true),
        Err(_) => Ok(false),
    }
}

#[napi(js_name = "contentHashHexDigest")]
pub fn content_hash_hex_digest(hash_str: String) -> napi::Result<String> {
    let hash = ContentHash::parse(&hash_str).map_err(map_error)?;
    Ok(hash.hex_digest().to_string())
}

// ═══════════════════════════════════════════════════════════════════════
// Commit / Change Operations
// ═══════════════════════════════════════════════════════════════════════

#[napi(js_name = "changeSet")]
pub fn change_set(
    path: String,
    old_value: Option<String>,
    new_value: String,
) -> napi::Result<String> {
    let change = Change::set(&path, old_value, new_value);
    serde_json::to_string(&change).map_err(|e| Error::from_reason(e.to_string()))
}

#[napi(js_name = "changeDelete")]
pub fn change_delete(path: String, old_value: String) -> napi::Result<String> {
    let change = Change::delete(&path, old_value);
    serde_json::to_string(&change).map_err(|e| Error::from_reason(e.to_string()))
}

#[napi(js_name = "changeAppend")]
pub fn change_append(path: String, new_value: String) -> napi::Result<String> {
    let change = Change::append(&path, new_value);
    serde_json::to_string(&change).map_err(|e| Error::from_reason(e.to_string()))
}

#[napi(js_name = "commitNew")]
pub fn commit_new(
    parent: Option<String>,
    author: String,
    message: String,
    manifest_hash: String,
    changes_json: String,
) -> napi::Result<String> {
    let changes: Vec<Change> =
        serde_json::from_str(&changes_json).map_err(|e| Error::from_reason(e.to_string()))?;
    let commit = Commit::new(parent, &author, &message, &manifest_hash, changes);
    serde_json::to_string(&commit).map_err(|e| Error::from_reason(e.to_string()))
}

#[napi(js_name = "commitVerifyId")]
pub fn commit_verify_id(json_str: String) -> napi::Result<bool> {
    let commit: Commit =
        serde_json::from_str(&json_str).map_err(|e| Error::from_reason(e.to_string()))?;
    Ok(commit.verify_id())
}

// ═══════════════════════════════════════════════════════════════════════
// Repository Operations
// ═══════════════════════════════════════════════════════════════════════

#[napi(js_name = "repoInit")]
pub fn repo_init(base_path: String, universe: String) -> napi::Result<String> {
    let repo = Repository::init(
        Path::new(&base_path),
        &universe,
        Box::new(LoreBackend::from_env()),
    )
    .map_err(map_error)?;
    let result = serde_json::json!({
        "root": repo.root.to_string_lossy(),
        "universe": repo.universe,
    });
    Ok(result.to_string())
}

#[napi(js_name = "repoOpen")]
pub fn repo_open(base_path: String, universe: String) -> napi::Result<String> {
    let repo = open_repo(&base_path, &universe)?;
    let result = serde_json::json!({
        "root": repo.root.to_string_lossy(),
        "universe": repo.universe,
    });
    Ok(result.to_string())
}

#[napi(js_name = "repoCreateEntity")]
pub fn repo_create_entity(
    base_path: String,
    universe: String,
    entity_type: String,
    entity_id: String,
    name: String,
    author: String,
) -> napi::Result<String> {
    let et = parse_et(&entity_type)?;
    let repo = open_repo(&base_path, &universe)?;
    let (manifest, commit_hash) = repo
        .create_entity(et, &entity_id, &name, &author)
        .map_err(map_error)?;
    let result = serde_json::json!({
        "manifest": serde_json::to_value(&manifest).map_err(|e| Error::from_reason(e.to_string()))?,
        "commit_hash": commit_hash,
    });
    Ok(result.to_string())
}

#[napi(js_name = "repoReadManifest")]
pub fn repo_read_manifest(
    base_path: String,
    universe: String,
    entity_type: String,
    entity_id: String,
) -> napi::Result<String> {
    let et = parse_et(&entity_type)?;
    let repo = open_repo(&base_path, &universe)?;
    let manifest = repo.read_manifest(et, &entity_id).map_err(map_error)?;
    serde_json::to_string(&manifest).map_err(|e| Error::from_reason(e.to_string()))
}

#[napi(js_name = "repoReadManifestAtRef")]
pub fn repo_read_manifest_at_ref(
    base_path: String,
    universe: String,
    entity_type: String,
    entity_id: String,
    reference: String,
) -> napi::Result<String> {
    let et = parse_et(&entity_type)?;
    let repo = open_repo(&base_path, &universe)?;
    let manifest = repo
        .read_manifest_at_ref(et, &entity_id, &reference)
        .map_err(map_error)?;
    serde_json::to_string(&manifest).map_err(|e| Error::from_reason(e.to_string()))
}

#[napi(js_name = "repoWriteManifest")]
pub fn repo_write_manifest(
    base_path: String,
    universe: String,
    manifest_json: String,
) -> napi::Result<String> {
    let manifest: Manifest =
        serde_json::from_str(&manifest_json).map_err(|e| Error::from_reason(e.to_string()))?;
    let repo = open_repo(&base_path, &universe)?;
    let path = repo.write_manifest(&manifest).map_err(map_error)?;
    Ok(path.to_string_lossy().to_string())
}

#[napi(js_name = "repoCommitManifest")]
pub fn repo_commit_manifest(
    base_path: String,
    universe: String,
    entity_type: String,
    entity_id: String,
    message: String,
    author: String,
    changes_json: String,
) -> napi::Result<String> {
    let et = parse_et(&entity_type)?;
    let repo = open_repo(&base_path, &universe)?;
    let mut manifest = repo.read_manifest(et, &entity_id).map_err(map_error)?;
    let changes: Vec<Change> =
        serde_json::from_str(&changes_json).map_err(|e| Error::from_reason(e.to_string()))?;
    let commit = repo
        .commit_manifest(&mut manifest, &message, &author, changes)
        .map_err(map_error)?;
    let result = serde_json::json!({
        "commit": serde_json::to_value(&commit).map_err(|e| Error::from_reason(e.to_string()))?,
        "version": manifest.version,
    });
    Ok(result.to_string())
}

#[napi(js_name = "repoDeleteEntity")]
pub fn repo_delete_entity(
    base_path: String,
    universe: String,
    entity_type: String,
    entity_id: String,
    author: String,
) -> napi::Result<String> {
    let et = parse_et(&entity_type)?;
    let repo = open_repo(&base_path, &universe)?;
    let hash = repo
        .delete_entity(et, &entity_id, &author)
        .map_err(map_error)?;
    Ok(hash)
}

#[napi(js_name = "repoHistory")]
pub fn repo_history(
    base_path: String,
    universe: String,
    entity_type: String,
    entity_id: String,
    limit: i64,
) -> napi::Result<String> {
    let et = parse_et(&entity_type)?;
    let repo = open_repo(&base_path, &universe)?;
    let history = repo
        .history(et, &entity_id, limit as usize)
        .map_err(map_error)?;
    serde_json::to_string(&history).map_err(|e| Error::from_reason(e.to_string()))
}

#[napi(js_name = "repoListEntities")]
pub fn repo_list_entities(
    base_path: String,
    universe: String,
    entity_type: String,
) -> napi::Result<String> {
    let et = parse_et(&entity_type)?;
    let repo = open_repo(&base_path, &universe)?;
    let entities = repo.list_entities(et).map_err(map_error)?;
    serde_json::to_string(&entities).map_err(|e| Error::from_reason(e.to_string()))
}

#[napi(js_name = "repoCreateBranch")]
pub fn repo_create_branch(
    base_path: String,
    universe: String,
    name: String,
) -> napi::Result<String> {
    let repo = open_repo(&base_path, &universe)?;
    repo.create_branch(&name).map_err(map_error)?;
    Ok(serde_json::json!({"success": true, "branch": name}).to_string())
}

#[napi(js_name = "repoSwitchBranch")]
pub fn repo_switch_branch(
    base_path: String,
    universe: String,
    name: String,
) -> napi::Result<String> {
    let repo = open_repo(&base_path, &universe)?;
    repo.switch_branch(&name).map_err(map_error)?;
    Ok(serde_json::json!({"success": true, "branch": name}).to_string())
}

#[napi(js_name = "repoListBranches")]
pub fn repo_list_branches(base_path: String, universe: String) -> napi::Result<String> {
    let repo = open_repo(&base_path, &universe)?;
    let branches = repo.list_branches().map_err(map_error)?;
    serde_json::to_string(&branches).map_err(|e| Error::from_reason(e.to_string()))
}

#[napi(js_name = "repoCreateTag")]
pub fn repo_create_tag(base_path: String, universe: String, name: String) -> napi::Result<String> {
    let repo = open_repo(&base_path, &universe)?;
    repo.create_tag(&name).map_err(map_error)?;
    Ok(serde_json::json!({"success": true, "tag": name}).to_string())
}

#[napi(js_name = "repoListTags")]
pub fn repo_list_tags(base_path: String, universe: String) -> napi::Result<String> {
    let repo = open_repo(&base_path, &universe)?;
    let tags = repo.list_tags().map_err(map_error)?;
    serde_json::to_string(&tags).map_err(|e| Error::from_reason(e.to_string()))
}

#[napi(js_name = "repoHeadHash")]
pub fn repo_head_hash(base_path: String, universe: String) -> napi::Result<String> {
    let repo = open_repo(&base_path, &universe)?;
    let hash = repo.head_hash().map_err(map_error)?;
    Ok(hash)
}

#[napi(js_name = "repoRevertCommit")]
pub fn repo_revert_commit(
    base_path: String,
    universe: String,
    commit_hash: String,
    author: String,
) -> napi::Result<String> {
    let repo = open_repo(&base_path, &universe)?;
    let new_hash = repo
        .revert_commit(&commit_hash, &author)
        .map_err(map_error)?;
    Ok(new_hash)
}

#[napi(js_name = "repoAddRemote")]
pub fn repo_add_remote(
    base_path: String,
    universe: String,
    name: String,
    url: String,
) -> napi::Result<String> {
    let repo = open_repo(&base_path, &universe)?;
    repo.add_remote(&name, &url).map_err(map_error)?;
    Ok(serde_json::json!({"success": true, "remote": name, "url": url}).to_string())
}

#[napi(js_name = "repoRemoveRemote")]
pub fn repo_remove_remote(
    base_path: String,
    universe: String,
    name: String,
) -> napi::Result<String> {
    let repo = open_repo(&base_path, &universe)?;
    repo.remove_remote(&name).map_err(map_error)?;
    Ok(serde_json::json!({"success": true, "remote": name}).to_string())
}

#[napi(js_name = "repoListRemotes")]
pub fn repo_list_remotes(base_path: String, universe: String) -> napi::Result<String> {
    let repo = open_repo(&base_path, &universe)?;
    let remotes = repo.list_remotes().map_err(map_error)?;
    serde_json::to_string(&remotes).map_err(|e| Error::from_reason(e.to_string()))
}

#[napi(js_name = "repoPush")]
pub fn repo_push(
    base_path: String,
    universe: String,
    remote: Option<String>,
    branch: Option<String>,
) -> napi::Result<String> {
    let repo = open_repo(&base_path, &universe)?;
    repo.push(remote.as_deref(), branch.as_deref())
        .map_err(map_error)?;
    Ok(serde_json::json!({"success": true}).to_string())
}

#[napi(js_name = "repoPull")]
pub fn repo_pull(
    base_path: String,
    universe: String,
    remote: Option<String>,
    branch: Option<String>,
) -> napi::Result<String> {
    let repo = open_repo(&base_path, &universe)?;
    repo.pull(remote.as_deref(), branch.as_deref())
        .map_err(map_error)?;
    Ok(serde_json::json!({"success": true}).to_string())
}

// ═══════════════════════════════════════════════════════════════════════
// Resolver Operations
// ═══════════════════════════════════════════════════════════════════════

#[napi(js_name = "resolve")]
pub fn resolve(uri_str: String, repo_base_path: String) -> napi::Result<String> {
    let resolver = Resolver::new(Path::new(&repo_base_path));
    let result = resolver
        .resolve(&uri_str, &ResolveOptions::default())
        .map_err(map_error)?;
    match result {
        ResolveResult::Full(manifest) => {
            serde_json::to_string(&manifest).map_err(|e| Error::from_reason(e.to_string()))
        }
        ResolveResult::Subtree(value) => Ok(value.to_string()),
    }
}

#[napi(js_name = "resolveWithOptions")]
pub fn resolve_with_options(
    uri_str: String,
    repo_base_path: String,
    branch: Option<String>,
    commit: Option<String>,
    tag: Option<String>,
    path: Option<String>,
) -> napi::Result<String> {
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
            serde_json::to_string(&manifest).map_err(|e| Error::from_reason(e.to_string()))
        }
        ResolveResult::Subtree(value) => Ok(value.to_string()),
    }
}

#[napi(js_name = "resolveQuery")]
pub fn resolve_query(
    uri_str: String,
    repo_base_path: String,
    path: String,
) -> napi::Result<String> {
    let resolver = Resolver::new(Path::new(&repo_base_path));
    let result = resolver.query(&uri_str, &path).map_err(map_error)?;
    Ok(result.to_string())
}

#[napi(js_name = "listUniverses")]
pub fn list_universes(repo_base_path: String) -> napi::Result<String> {
    let resolver = Resolver::new(Path::new(&repo_base_path));
    let universes = resolver.list_universes().map_err(map_error)?;
    serde_json::to_string(&universes).map_err(|e| Error::from_reason(e.to_string()))
}

// ═══════════════════════════════════════════════════════════════════════
// Schema Operations
// ═══════════════════════════════════════════════════════════════════════

#[napi(js_name = "manifestSchema")]
pub fn manifest_schema_json() -> napi::Result<String> {
    let schema = schema::manifest_schema();
    serde_json::to_string(&schema).map_err(|e| Error::from_reason(e.to_string()))
}

#[napi(js_name = "commitSchema")]
pub fn commit_schema_json() -> napi::Result<String> {
    let schema = schema::commit_schema();
    serde_json::to_string(&schema).map_err(|e| Error::from_reason(e.to_string()))
}

#[napi(js_name = "validateManifest")]
pub fn validate_manifest(json_str: String) -> napi::Result<String> {
    let manifest: Manifest =
        serde_json::from_str(&json_str).map_err(|e| Error::from_reason(e.to_string()))?;
    match schema::validate_manifest(&manifest) {
        Ok(()) => Ok(serde_json::json!({"valid": true}).to_string()),
        Err(errors) => Ok(serde_json::json!({"valid": false, "errors": errors}).to_string()),
    }
}

#[napi(js_name = "validateCommit")]
pub fn validate_commit(json_str: String) -> napi::Result<String> {
    let commit: Commit =
        serde_json::from_str(&json_str).map_err(|e| Error::from_reason(e.to_string()))?;
    match schema::validate_commit(&commit) {
        Ok(()) => Ok(serde_json::json!({"valid": true}).to_string()),
        Err(errors) => Ok(serde_json::json!({"valid": false, "errors": errors}).to_string()),
    }
}

// ═══════════════════════════════════════════════════════════════════════
// Merge Operations
// ═══════════════════════════════════════════════════════════════════════

#[napi(js_name = "mergeMerge")]
pub fn merge_merge(
    schema_json: String,
    base: String,
    current: String,
    proposed: String,
) -> napi::Result<String> {
    let sdl: nap_core::merge::sdl::SdlDocument =
        serde_json::from_str(&schema_json).map_err(|e| Error::from_reason(e.to_string()))?;
    let engine = nap_core::merge::merge_engine::MergeEngine::new(sdl);

    let base_val: serde_json::Value =
        serde_json::from_str(&base).map_err(|e| Error::from_reason(e.to_string()))?;
    let current_val: serde_json::Value =
        serde_json::from_str(&current).map_err(|e| Error::from_reason(e.to_string()))?;
    let proposed_val: serde_json::Value =
        serde_json::from_str(&proposed).map_err(|e| Error::from_reason(e.to_string()))?;

    let result = engine.merge(base_val, current_val, proposed_val);
    let result_json =
        serde_json::to_value(&result).map_err(|e| Error::from_reason(e.to_string()))?;
    Ok(result_json.to_string())
}

#[napi(js_name = "mergeDiff")]
pub fn merge_diff(schema_json: String, base: String, candidate: String) -> napi::Result<String> {
    let sdl: nap_core::merge::sdl::SdlDocument =
        serde_json::from_str(&schema_json).map_err(|e| Error::from_reason(e.to_string()))?;
    let base_val: serde_json::Value =
        serde_json::from_str(&base).map_err(|e| Error::from_reason(e.to_string()))?;
    let candidate_val: serde_json::Value =
        serde_json::from_str(&candidate).map_err(|e| Error::from_reason(e.to_string()))?;

    let result = nap_core::merge::diff::diff(&base_val, &candidate_val, &sdl);
    let result_json =
        serde_json::to_value(&result).map_err(|e| Error::from_reason(e.to_string()))?;
    Ok(result_json.to_string())
}

// ═══════════════════════════════════════════════════════════════════════
// Storage Engine Operations
// ═══════════════════════════════════════════════════════════════════════

#[napi(js_name = "storageConfig")]
pub fn storage_config() -> napi::Result<String> {
    let engine = nap_core::storage::get_engine().map_err(|e| Error::from_reason(e.to_string()))?;
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
/// This is an async function returning a JavaScript Promise.  The storage
/// backend is determined at runtime by the ``NAP_STORAGE_BACKEND``
/// environment variable.
///
/// ## Memory Safety (NAPI Buffer Ownership)
///
/// The incoming [`Buffer`] is **copied** to an owned `Vec<u8>` **before**
/// crossing the await boundary.  Holding a raw pointer to a V8 `ArrayBuffer`
/// across an await point is unsafe because Node's garbage collector may
/// invalidate or relocate the backing store.  By cloning eagerly we avoid
/// use-after-free and data races.
///
/// # Arguments
///
/// * `data` — Raw bytes of the media asset (image, audio, mesh, etc.),
///   passed as a Node.js `Buffer`.
/// * `format` — File extension without a leading dot (e.g. `"png"`,
///   `"jpg"`, `"wav"`, `"glb"`).
///
/// # Returns
///
/// A Promise resolving to the content-addressed hash `sha256:<hex>`.
#[napi]
pub async fn ingest_media(data: Buffer, format: String) -> napi::Result<String> {
    // CRITICAL: Copy the buffer to an owned Vec<u8> before
    // the first await point.  Node's GC can invalidate the underlying
    // ArrayBuffer once we yield control to the Tokio runtime.
    let owned_data = data.to_vec();

    let engine = nap_core::storage::get_engine().map_err(|e| Error::from_reason(e.to_string()))?;

    engine
        .ingest_media(&owned_data, &format)
        .await
        .map_err(|e| Error::from_reason(e.to_string()))
}

// ═══════════════════════════════════════════════════════════════════════
// VCS / Lore Operations
// ═══════════════════════════════════════════════════════════════════════

#[napi(js_name = "loreClone")]
pub fn lore_clone(url: String, dest_path: String) -> napi::Result<String> {
    LoreBackend::clone_repo(&url, Path::new(&dest_path))
        .map_err(|e| Error::from_reason(e.to_string()))?;
    Ok(serde_json::json!({"success": true, "url": url, "path": dest_path}).to_string())
}

// ═══════════════════════════════════════════════════════════════════════
// Version
// ═══════════════════════════════════════════════════════════════════════

#[napi]
pub fn version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}
