//! Error types for the Narrative Addressing Protocol.
//!
//! All NAP errors are surfaced through [`NapError`], which captures the exact
//! failure domain (URI parsing, manifest I/O, VCS operations, resolution, etc.)
//! with enough context for callers to produce actionable diagnostics.

use thiserror::Error;

/// Top-level error type for all NAP operations.
#[derive(Error, Debug)]
pub enum NapError {
    // ── URI Errors ──────────────────────────────────────────────────────
    #[error("invalid NAP URI '{uri}': {reason}")]
    InvalidUri { uri: String, reason: String },

    #[error("unknown entity type '{0}'")]
    UnknownEntityType(String),

    // ── Manifest Errors ─────────────────────────────────────────────────
    #[error("manifest not found: {0}")]
    ManifestNotFound(String),

    #[error("manifest parse error for '{path}': {source}")]
    ManifestParseError {
        path: String,
        source: serde_yaml::Error,
    },

    #[error("manifest validation error: {0}")]
    ManifestValidationError(String),

    #[error("manifest write error for '{path}': {source}")]
    ManifestWriteError {
        path: String,
        source: std::io::Error,
    },

    // ── Query Errors ────────────────────────────────────────────────────
    #[error("query path not found: '{path}' in manifest '{manifest_id}'")]
    QueryPathNotFound { path: String, manifest_id: String },

    #[error("invalid query path: '{0}'")]
    InvalidQueryPath(String),

    // ── Repository Errors ───────────────────────────────────────────────
    #[error("repository not found at '{0}'")]
    RepositoryNotFound(String),

    #[error("repository already exists at '{0}'")]
    RepositoryAlreadyExists(String),

    #[error("universe '{0}' not found in repository root")]
    UniverseNotFound(String),

    // ── VCS Errors ──────────────────────────────────────────────────────
    #[error("VCS error: {0}")]
    VcsError(String),

    #[error("ref not found: '{0}'")]
    RefNotFound(String),

    // ── Content Addressing Errors ───────────────────────────────────────
    #[error("content hash mismatch: expected {expected}, got {actual}")]
    ContentHashMismatch { expected: String, actual: String },

    // ── I/O ─────────────────────────────────────────────────────────────
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    // ── Merge Errors ────────────────────────────────────────────────────
    #[error("merge conflict at '{path}': {details}")]
    MergeConflict { path: String, details: String },

    #[error("SDL parse error: {0}")]
    SdlParseError(String),

    #[error("SDL validation error: {reason}")]
    SdlValidationError { reason: String },

    #[error("merge strategy error at '{path}': {reason}")]
    MergeStrategyError { path: String, reason: String },

    #[error("merge validation error: {reason}")]
    MergeValidationError { reason: String },

    // ── Permission ──────────────────────────────────────────────────────
    #[error("permission denied: {0}")]
    PermissionDenied(String),

    // ── Catch-all ───────────────────────────────────────────────────────
    #[error("{0}")]
    Other(String),
}

/// Result type alias using [`NapError`].
pub type NapResult<T> = Result<T, NapError>;
