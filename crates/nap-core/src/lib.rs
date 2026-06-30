//! # NAP Core — Narrative Addressing Protocol
//!
//! Four primitives for entertainment infrastructure:
//! - **URI**: Identity — `nap://starwars/character/lukeskywalker`
//! - **Manifest**: Current state — YAML, human/machine/agent-readable
//! - **Commit**: History — snapshot + delta metadata
//! - **Resolver**: Resolution — URI → Manifest, with query/version selectors

pub mod autopublish;
pub mod commit;
pub mod content;
pub mod context_docs;
pub mod error;
pub mod manifest;
pub mod merge;
pub mod permission_gate;
pub mod query;
pub mod repo_service;
pub mod repository;
pub mod resolver;
pub mod schema;
pub mod storage;
pub mod types;
pub mod uri;
pub mod validation;
pub mod vcs;

// Git-backed VCS (deprecated — use vcs_lore for new development).
pub mod vcs_git;

// Lore VCS backend (production).
pub mod vcs_lore;

// Re-exports for ergonomic top-level usage
pub use autopublish::{AutopublishConfig, AutopublishHandle, AutopublishWorker};
pub use commit::{Change, ChangeOp, Commit};
pub use content::ContentHash;
pub use context_docs::ContextDocsManager;
pub use error::NapError;
pub use manifest::{Manifest, Principal, Provenance, Representation};
pub use merge::{
    conflict::{Conflict, ConflictType, MergeResult},
    diff::{Change as DiffChange, ChangeOp as DiffChangeOp, DiffResult, diff, diff_normalized},
    merge_engine::MergeEngine,
    normalization::normalize,
    path::CanonicalPath,
    sdl::{
        IdentityRule, MergeStrategyDef, MergeStrategyType, PropertyDef, PropertyType, SdlDocument,
        SdlError,
    },
    strategies,
};
pub use permission_gate::PermissionGate;
pub use query::ManifestQuery;
pub use repo_service::RepoService;
pub use repository::Repository;
pub use resolver::{ResolveOptions, Resolver};
pub use storage::{StorageBackend, StorageConfig, StorageEngine, StorageError, get_engine};
pub use types::EntityType;
pub use uri::NapUri;
pub use vcs::{AccessLevel, CommitInfo, ContextDocument, Permission, Revision, VcsBackend};
pub use vcs_git::GitBackend;
pub use vcs_lore::LoreBackend;
