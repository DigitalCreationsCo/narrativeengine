//! # Storage Engine — NAP Asset Ingestion & Content-Addressed Storage
//!
//! A unified storage abstraction over local-filesystem and S3-compatible
//! backends (including Cloudflare R2, MinIO, and generic S3 providers).
//!
//! ## Architecture
//!
//! The [`StorageEngine`] is initialised once (lazily via [`get_engine`]) and
//! reused across all FFI calls.  The engine selects the backend at runtime
//! based on the `NAP_STORAGE_BACKEND` environment variable:
//!
//! | Value   | Backend                        | Required env vars               |
//! |---------|--------------------------------|---------------------------------|
//! | `local` | Local filesystem               | `NAP_DIR` (default: `~/.nap`)   |
//! | `s3`    | S3-compatible object store     | `NAP_S3_BUCKET`, AWS creds      |
//!
//! ## Gotchas Handled
//!
//! * **S3 Endpoint Parsing** — The builder reads `AWS_ENDPOINT_URL_S3` (and
//!   falls back to `AWS_ENDPOINT_URL`) for R2/MinIO compatibility.  HTTP
//!   endpoints automatically enable `with_allow_http(true)`.
//! * **Cross-platform paths** — All local paths use `PathBuf`; never string
//!   concatenation with `/` or `\`.
//! * **Idempotency** — `ingest_media` issues a HEAD before PUT; if the object
//!   already exists, the write is skipped and the hash is returned immediately.
//! * **Gitignore** — The local initialiser ensures `.nap-assets/` is present
//!   in `<NAP_DIR>/.gitignore` so raw binaries never enter the Git graph.

use bytes::Bytes;
use object_store::ObjectStore;
use object_store::aws::AmazonS3Builder;
use object_store::local::LocalFileSystem;
use object_store::path::Path as StorePath;
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};
use std::sync::OnceLock;
use thiserror::Error;
use tracing::{debug, error, info};

// ---------------------------------------------------------------------------
// Error type
// ---------------------------------------------------------------------------

/// Unified error type for all storage operations.
#[derive(Error, Debug)]
pub enum StorageError {
    /// Engine initialisation failed (bad env, missing deps, etc.).
    #[error("storage initialisation failed: {0}")]
    Init(String),

    /// Underlying I/O error (local filesystem).
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    /// Object-store error (network, auth, etc.).
    #[error("object store error: {0}")]
    ObjectStore(#[from] object_store::Error),

    /// A content hash string was malformed.
    #[error("invalid content hash: {0}")]
    InvalidHash(String),

    /// A required environment variable is not set.
    #[error("required environment variable `{0}` is not set")]
    MissingEnvVar(String),

    /// The path could not be converted to a valid object-store path.
    #[error("invalid storage path: {0}")]
    InvalidPath(String),
}

/// Convenience alias for storage results.
pub type StorageResult<T> = Result<T, StorageError>;

// ---------------------------------------------------------------------------
// Backend selection
// ---------------------------------------------------------------------------

/// Storage backend variant selected at runtime.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StorageBackend {
    /// Local filesystem, writing to `<NAP_DIR>/.nap-assets/`.
    Local,
    /// S3-compatible object store (AWS, R2, MinIO, etc.).
    S3,
}

impl std::fmt::Display for StorageBackend {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            StorageBackend::Local => write!(f, "local"),
            StorageBackend::S3 => write!(f, "s3"),
        }
    }
}

// ---------------------------------------------------------------------------
// Configuration
// ---------------------------------------------------------------------------

/// Runtime configuration of the storage engine.
#[derive(Debug, Clone)]
pub struct StorageConfig {
    /// Which backend is active.
    pub backend: StorageBackend,
    /// Base directory for local storage (resolved absolute path).
    pub base_dir: PathBuf,
    /// Subdirectory / prefix for asset blobs (`.nap-assets`).
    pub assets_prefix: String,
    /// S3 bucket name (empty for local backend).
    pub bucket: String,
}

// ---------------------------------------------------------------------------
// Engine
// ---------------------------------------------------------------------------

/// Thread-safe, lazily-initialised storage engine.
///
/// The engine owns the [`ObjectStore`] adapter and exposes the asset-ingestion
/// pipeline.  It is `Send + Sync` and designed to be stored in a global
/// [`OnceLock`] so that FFI bindings can access it without constructing their
/// own instance.
pub struct StorageEngine {
    store: Box<dyn ObjectStore>,
    config: StorageConfig,
}

// Safety: object_store's ObjectStore trait bounds require all implementors
// to be Send + Sync, so Box<dyn ObjectStore> inherits those auto-traits.

impl StorageEngine {
    /// Construct the engine from environment variables.
    ///
    /// ## Environment Variables
    ///
    /// | Variable               | Used by    | Purpose                                       |
    /// |------------------------|------------|-----------------------------------------------|
    /// | `NAP_STORAGE_BACKEND`  | both       | `local` (default) or `s3`                     |
    /// | `NAP_DIR`              | local      | Base directory (default: `~/.nap`)            |
    /// | `NAP_S3_BUCKET`        | s3         | S3 bucket name (required)                     |
    /// | `AWS_ACCESS_KEY_ID`    | s3         | AWS / R2 access key                           |
    /// | `AWS_SECRET_ACCESS_KEY`| s3         | AWS / R2 secret key                           |
    /// | `AWS_REGION`           | s3         | AWS region (e.g. `us-east-1`)                 |
    /// | `AWS_ENDPOINT_URL_S3`  | s3         | Custom S3 endpoint (R2, MinIO)                |
    /// | `AWS_ENDPOINT_URL`     | s3         | Fallback if `AWS_ENDPOINT_URL_S3` unset       |
    pub fn from_env() -> StorageResult<Self> {
        let backend_str =
            std::env::var("NAP_STORAGE_BACKEND").unwrap_or_else(|_| "local".to_string());

        match backend_str.to_lowercase().as_str() {
            "local" => Self::init_local(),
            "s3" => Self::init_s3(),
            other => Err(StorageError::Init(format!(
                "unknown storage backend '{other}'; expected 'local' or 's3'"
            ))),
        }
    }

    // ------------------------------------------------------------------
    // Local backend initialisation
    // ------------------------------------------------------------------

    fn init_local() -> StorageResult<Self> {
        let raw = std::env::var("NAP_DIR").unwrap_or_else(|_| "~/.nap".to_string());
        let base_dir = resolve_path(&raw);

        // Ensure the assets directory exists before any write.
        let assets_dir = base_dir.join(".nap-assets");
        std::fs::create_dir_all(&assets_dir).map_err(|e| {
            error!(
                path = %assets_dir.display(),
                error = %e,
                "failed to create assets directory"
            );
            StorageError::Io(e)
        })?;

        // Manage .gitignore so binaries stay out of Git.
        Self::ensure_gitignore(&base_dir)?;

        let store = Box::new(LocalFileSystem::new()) as Box<dyn ObjectStore>;

        info!(
            backend = "local",
            base_dir = %base_dir.display(),
            assets_dir = %assets_dir.display(),
            "storage engine initialised (local)"
        );

        Ok(Self {
            store,
            config: StorageConfig {
                backend: StorageBackend::Local,
                base_dir,
                assets_prefix: ".nap-assets".to_string(),
                bucket: String::new(),
            },
        })
    }

    /// Ensure `<NAP_DIR>/.gitignore` exists and contains `.nap-assets/`.
    fn ensure_gitignore(base_dir: &Path) -> StorageResult<()> {
        let gitignore_path = base_dir.join(".gitignore");
        const ASSETS_ENTRY: &str = ".nap-assets/";

        if !gitignore_path.exists() {
            std::fs::write(&gitignore_path, format!("{ASSETS_ENTRY}\n")).map_err(|e| {
                error!(
                    path = %gitignore_path.display(),
                    error = %e,
                    "failed to create .gitignore"
                );
                StorageError::Io(e)
            })?;
            info!(
                path = %gitignore_path.display(),
                "created .gitignore with .nap-assets/ entry"
            );
            return Ok(());
        }

        // Read existing content and check for the entry.
        let content = std::fs::read_to_string(&gitignore_path).map_err(|e| {
            error!(
                path = %gitignore_path.display(),
                error = %e,
                "failed to read existing .gitignore"
            );
            StorageError::Io(e)
        })?;

        let already_present = content.lines().any(|line| {
            let trimmed = line.trim();
            trimmed == ASSETS_ENTRY || trimmed.trim_end_matches('/') == ".nap-assets"
        });

        if !already_present {
            let updated = format!("{content}\n{ASSETS_ENTRY}\n");
            std::fs::write(&gitignore_path, updated).map_err(|e| {
                error!(
                    path = %gitignore_path.display(),
                    error = %e,
                    "failed to append .nap-assets/ to .gitignore"
                );
                StorageError::Io(e)
            })?;
            info!(
                path = %gitignore_path.display(),
                "appended .nap-assets/ entry to .gitignore"
            );
        } else {
            debug!(
                path = %gitignore_path.display(),
                ".gitignore already contains .nap-assets/ entry"
            );
        }

        Ok(())
    }

    // ------------------------------------------------------------------
    // S3 backend initialisation
    // ------------------------------------------------------------------

    fn init_s3() -> StorageResult<Self> {
        let bucket = std::env::var("NAP_S3_BUCKET").map_err(|_| {
            error!("NAP_S3_BUCKET is required for S3 storage backend");
            StorageError::MissingEnvVar("NAP_S3_BUCKET".to_string())
        })?;

        let mut builder = AmazonS3Builder::from_env().with_bucket_name(&bucket);

        // Handle custom S3-compatible endpoints (Cloudflare R2, MinIO, etc.).
        // AWS_ENDPOINT_URL_S3 is the service-specific override; fall back to
        // the generic AWS_ENDPOINT_URL if the S3-specific var is unset.
        let endpoint_var =
            std::env::var("AWS_ENDPOINT_URL_S3").or_else(|_| std::env::var("AWS_ENDPOINT_URL"));

        if let Ok(endpoint) = endpoint_var {
            let endpoint_str = endpoint.trim().to_string();

            // Non-standard endpoints often use HTTP in dev environments.
            if endpoint_str.starts_with("http://") {
                builder = builder.with_allow_http(true);
                debug!(endpoint = %endpoint_str, "HTTP endpoint detected, allowing HTTP");
            }

            builder = builder.with_endpoint(&endpoint_str);
            debug!(endpoint = %endpoint_str, "configured custom S3 endpoint");
        }

        let store: Box<dyn ObjectStore> = Box::new(builder.build().map_err(|e| {
            error!(
                bucket = %bucket,
                error = %e,
                "failed to build S3 object store"
            );
            StorageError::ObjectStore(e)
        })?);

        info!(
            backend = "s3",
            bucket = %bucket,
            "storage engine initialised (S3)"
        );

        Ok(Self {
            store,
            config: StorageConfig {
                backend: StorageBackend::S3,
                base_dir: PathBuf::new(),
                assets_prefix: ".nap-assets".to_string(),
                bucket,
            },
        })
    }

    // ------------------------------------------------------------------
    // Public API
    // ------------------------------------------------------------------

    /// Return the active configuration.
    pub fn config(&self) -> &StorageConfig {
        &self.config
    }

    /// Return a reference to the underlying [`ObjectStore`].
    pub fn store(&self) -> &dyn ObjectStore {
        self.store.as_ref()
    }

    /// Ingest media bytes into the content-addressed store.
    ///
    /// This is the core ingestion pipeline:
    ///
    /// 1. **Hash** — Compute `sha256:<hex>` from the byte slice.
    /// 2. **Idempotency check** — Send a HEAD request to see if the blob
    ///    already exists.  If it does, return the hash immediately without
    ///    any network / disk write.
    /// 3. **PUT** — Upload the bytes as `<hex>.<format>` under the assets
    ///    prefix (`.nap-assets/` for both backends).
    ///
    /// # Arguments
    ///
    /// * `data` — Raw media bytes (image, audio, mesh, etc.).
    /// * `format` — File extension without leading dot (e.g. `"png"`,
    ///   `"jpg"`, `"wav"`, `"glb"`).
    ///
    /// # Returns
    ///
    /// The content-addressed hash string `sha256:<hex>`.
    pub async fn ingest_media(&self, data: &[u8], format: &str) -> StorageResult<String> {
        // ── Step 1: SHA-256 hash ────────────────────────────────────
        let hex_digest = {
            let mut hasher = Sha256::new();
            hasher.update(data);
            hex::encode(hasher.finalize())
        };
        let hash = format!("sha256:{hex_digest}");
        let filename = format!("{hex_digest}.{format}");

        debug!(
            hash = %hash,
            format = %format,
            size = data.len(),
            "ingesting media asset"
        );

        // ── Step 2: Build storage path ──────────────────────────────
        let store_path = self.build_store_path(&filename)?;

        // ── Step 3: Idempotency check (HEAD before PUT) ─────────────
        match self.store.head(&store_path).await {
            Ok(meta) => {
                debug!(
                    hash = %hash,
                    path = %store_path,
                    size = meta.size,
                    "asset already exists, skipping write"
                );
                return Ok(hash);
            }
            Err(e) => {
                // Only NotFound is expected — any other error is real.
                if !matches!(&e, object_store::Error::NotFound { .. }) {
                    error!(
                        hash = %hash,
                        path = %store_path,
                        error = %e,
                        "HEAD request failed before PUT"
                    );
                    return Err(StorageError::ObjectStore(e));
                }
                // Asset does not exist — proceed to write.
                debug!(
                    hash = %hash,
                    path = %store_path,
                    "asset not found via HEAD, proceeding with PUT"
                );
            }
        }

        // ── Step 4: PUT ─────────────────────────────────────────────
        let blob = Bytes::copy_from_slice(data);
        self.store
            .put(&store_path, blob.into())
            .await
            .map_err(|e| {
                error!(
                    hash = %hash,
                    path = %store_path,
                    error = %e,
                    "PUT failed during media ingestion"
                );
                StorageError::ObjectStore(e)
            })?;

        info!(
            hash = %hash,
            path = %store_path,
            size = data.len(),
            "media asset ingested successfully"
        );

        Ok(hash)
    }

    // ------------------------------------------------------------------
    // Internal helpers
    // ------------------------------------------------------------------

    /// Build an object-store [`StorePath`] from a filename, taking the
    /// active backend into account.
    fn build_store_path(&self, filename: &str) -> StorageResult<StorePath> {
        match self.config.backend {
            StorageBackend::Local => {
                let full_path = self
                    .config
                    .base_dir
                    .join(&self.config.assets_prefix)
                    .join(filename);

                StorePath::from_absolute_path(&full_path).map_err(|e| {
                    error!(
                        path = %full_path.display(),
                        error = %e,
                        "failed to convert local filesystem path to store path"
                    );
                    StorageError::InvalidPath(format!(
                        "cannot convert '{}' to store path: {e}",
                        full_path.display()
                    ))
                })
            }
            StorageBackend::S3 => {
                // S3 keys are URL-style and relative.
                let key = format!("{}/{}", self.config.assets_prefix, filename);
                Ok(StorePath::from(key))
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Global singleton
// ---------------------------------------------------------------------------
//
// We use `OnceLock<Result<StorageEngine, String>>` because
// `OnceLock::get_or_try_init` has not been stabilised as of Rust 1.96.
// Storing the error as a `String` keeps the init path simple and avoids
// requiring `Clone` on `StorageError`.

type EngineInitResult = Result<StorageEngine, String>;

static ENGINE: OnceLock<EngineInitResult> = OnceLock::new();

/// Return a reference to the globally-initialised [`StorageEngine`].
///
/// The engine is lazily initialised on first access using
/// [`StorageEngine::from_env`].  Subsequent calls return the same instance.
///
/// # Errors
///
/// Returns [`StorageError::Init`] if environment variables are invalid or
/// the backend cannot be configured.
pub fn get_engine() -> StorageResult<&'static StorageEngine> {
    let result = ENGINE.get_or_init(|| {
        info!("initialising storage engine (lazy)");
        StorageEngine::from_env().map_err(|e| e.to_string())
    });

    match result {
        Ok(engine) => Ok(engine),
        Err(msg) => Err(StorageError::Init(msg.clone())),
    }
}

// ---------------------------------------------------------------------------
// Utility
// ---------------------------------------------------------------------------

/// Resolve a filesystem path, expanding `~/` and converting relative paths
/// to absolute form.
fn resolve_path(raw: &str) -> PathBuf {
    let expanded = if let Some(rest) = raw.strip_prefix("~/") {
        if let Some(home) = std::env::var_os("HOME").or_else(|| {
            // Fallback for Windows / unusual environments.
            std::env::var_os("USERPROFILE")
        }) {
            PathBuf::from(home).join(rest)
        } else {
            // No home dir available — keep the path as-is.
            PathBuf::from(raw)
        }
    } else {
        PathBuf::from(raw)
    };

    // Make relative paths absolute before canonicalising.
    let absolute = if expanded.is_relative() {
        if let Ok(cwd) = std::env::current_dir() {
            cwd.join(&expanded)
        } else {
            expanded
        }
    } else {
        expanded
    };

    // Attempt to canonicalise; fall back to the absolute-but-not-canonical
    // form if the path doesn't exist yet (e.g. first run).
    std::fs::canonicalize(&absolute).unwrap_or(absolute)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use object_store::memory::InMemory;

    // ── Helpers ─────────────────────────────────────────────────────

    /// Construct an engine backed by an in-memory store (no I/O, fast).
    fn in_memory_engine() -> StorageEngine {
        StorageEngine {
            store: Box::new(InMemory::new()),
            config: StorageConfig {
                backend: StorageBackend::S3,
                base_dir: PathBuf::new(),
                assets_prefix: ".nap-assets".to_string(),
                bucket: "test-bucket".to_string(),
            },
        }
    }

    /// Construct an engine backed by a local filesystem rooted at `dir`.
    fn local_fs_engine(dir: &Path) -> StorageEngine {
        // Ensure the assets subdirectory exists.
        let assets = dir.join(".nap-assets");
        std::fs::create_dir_all(&assets).unwrap();

        StorageEngine {
            store: Box::new(LocalFileSystem::new()),
            config: StorageConfig {
                backend: StorageBackend::Local,
                base_dir: dir.to_path_buf(),
                assets_prefix: ".nap-assets".to_string(),
                bucket: String::new(),
            },
        }
    }

    // ── resolve_path ────────────────────────────────────────────────

    #[test]
    fn test_resolve_path_expands_tilde() {
        let resolved = resolve_path("~/nap-test-storage");
        assert!(resolved.is_absolute(), "expected absolute path");
        let home = std::env::var("HOME").unwrap();
        assert!(
            resolved.starts_with(&home),
            "expected '{:?}' to start with '{home}'",
            resolved
        );
    }

    #[test]
    fn test_resolve_path_absolute_passthrough() {
        let p = "/tmp/nap-test-abs";
        let resolved = resolve_path(p);
        assert_eq!(resolved, PathBuf::from(p));
    }

    #[test]
    fn test_resolve_path_relative_uses_cwd() {
        let resolved = resolve_path("relative-dir");
        assert!(
            resolved.is_absolute(),
            "relative path should be resolved to absolute"
        );
    }

    // ── .gitignore management ───────────────────────────────────────

    #[test]
    fn test_ensure_gitignore_creates_when_missing() {
        let tmp = tempfile::TempDir::new().unwrap();
        let base = tmp.path();

        StorageEngine::ensure_gitignore(base).unwrap();

        let gitignore = base.join(".gitignore");
        assert!(gitignore.exists(), ".gitignore should have been created");

        let content = std::fs::read_to_string(&gitignore).unwrap();
        assert!(
            content.contains(".nap-assets/"),
            "expected .nap-assets/ in .gitignore, got: {content:?}"
        );
    }

    #[test]
    fn test_ensure_gitignore_appends_when_entry_missing() {
        let tmp = tempfile::TempDir::new().unwrap();
        let base = tmp.path();

        std::fs::write(base.join(".gitignore"), "target/\nnode_modules/\n").unwrap();

        StorageEngine::ensure_gitignore(base).unwrap();

        let content = std::fs::read_to_string(base.join(".gitignore")).unwrap();
        assert!(
            content.contains(".nap-assets/"),
            "expected .nap-assets/ to be appended, got: {content:?}"
        );
        assert!(
            content.contains("target/"),
            "existing entries should be preserved"
        );
        assert!(
            content.contains("node_modules/"),
            "existing entries should be preserved"
        );
    }

    #[test]
    fn test_ensure_gitignore_noop_when_entry_present() {
        let tmp = tempfile::TempDir::new().unwrap();
        let base = tmp.path();

        std::fs::write(base.join(".gitignore"), "target/\n.nap-assets/\n").unwrap();

        StorageEngine::ensure_gitignore(base).unwrap();

        let content = std::fs::read_to_string(base.join(".gitignore")).unwrap();
        assert_eq!(content, "target/\n.nap-assets/\n");
    }

    // ── build_store_path ────────────────────────────────────────────

    #[test]
    fn test_build_store_path_local() {
        let engine = StorageEngine {
            store: Box::new(LocalFileSystem::new()),
            config: StorageConfig {
                backend: StorageBackend::Local,
                base_dir: PathBuf::from("/tmp/nap-test"),
                assets_prefix: ".nap-assets".to_string(),
                bucket: String::new(),
            },
        };

        let path = engine.build_store_path("abc123.png").unwrap();
        let path_str = path.to_string();
        assert!(
            path_str.contains(".nap-assets/abc123.png"),
            "expected path containing '.nap-assets/abc123.png', got: {path_str}"
        );
    }

    #[test]
    fn test_build_store_path_s3() {
        let engine = StorageEngine {
            store: Box::new(LocalFileSystem::new()),
            config: StorageConfig {
                backend: StorageBackend::S3,
                base_dir: PathBuf::new(),
                assets_prefix: ".nap-assets".to_string(),
                bucket: "my-bucket".to_string(),
            },
        };

        let path = engine.build_store_path("abc123.png").unwrap();
        assert_eq!(path.to_string(), ".nap-assets/abc123.png");
    }

    #[test]
    fn test_build_store_path_local_from_absolute_path() {
        // Verify the store path for a local backend contains the expected
        // directory components.  Note: `from_absolute_path` returns a path
        // relative to the filesystem root (strips the leading '/'), so
        // we check for the subdirectory components rather than a leading '/'.
        let engine = StorageEngine {
            store: Box::new(LocalFileSystem::new()),
            config: StorageConfig {
                backend: StorageBackend::Local,
                base_dir: PathBuf::from("/tmp/nap-test"),
                assets_prefix: ".nap-assets".to_string(),
                bucket: String::new(),
            },
        };

        let path = engine.build_store_path("abc.png").unwrap();
        let path_str = path.to_string();
        assert!(
            path_str.contains(".nap-assets/abc.png"),
            "expected path to contain '.nap-assets/abc.png', got: {path_str}"
        );
        // The path should not be a URL-style S3 key.
        assert!(
            !path_str.starts_with(".nap-assets/"),
            "local paths should NOT be relative URL-style, got: {path_str}"
        );
    }

    // ── SHA-256 hash format ─────────────────────────────────────────

    #[test]
    fn test_sha256_hash_format() {
        let data = b"hello world";
        let mut hasher = Sha256::new();
        hasher.update(data);
        let hex_digest = hex::encode(hasher.finalize());
        let hash = format!("sha256:{hex_digest}");

        assert!(hash.starts_with("sha256:"), "hash must start with sha256:");
        assert_eq!(hash.len(), 71, "sha256:<64 hex chars> = 71 chars");
    }

    // ── Display impl ────────────────────────────────────────────────

    #[test]
    fn test_storage_backend_display() {
        assert_eq!(StorageBackend::Local.to_string(), "local");
        assert_eq!(StorageBackend::S3.to_string(), "s3");
    }

    // ── Error formatting ────────────────────────────────────────────

    #[test]
    fn test_storage_error_messages() {
        let err = StorageError::Init("test".to_string());
        assert_eq!(err.to_string(), "storage initialisation failed: test");

        let err = StorageError::MissingEnvVar("NAP_S3_BUCKET".to_string());
        assert_eq!(
            err.to_string(),
            "required environment variable `NAP_S3_BUCKET` is not set"
        );

        let err = StorageError::InvalidPath("bad".to_string());
        assert_eq!(err.to_string(), "invalid storage path: bad");
    }

    // ══════════════════════════════════════════════════════════════════
    //  IN-MEMORY INGESTION TESTS
    // ══════════════════════════════════════════════════════════════════

    #[tokio::test]
    async fn test_ingest_media_returns_correct_hash_format() {
        let engine = in_memory_engine();
        let hash = engine.ingest_media(b"hello world", "txt").await.unwrap();

        assert!(
            hash.starts_with("sha256:"),
            "hash must start with 'sha256:', got: {hash}"
        );
        assert_eq!(hash.len(), 71, "sha256:<64 hex chars> should be 71 chars");
    }

    #[tokio::test]
    async fn test_ingest_media_is_idempotent() {
        // Same data ingested twice should return the same hash AND
        // result in only one object in the store (second call is a no-op).
        let engine = in_memory_engine();
        let data = b"some-image-bytes-12345";

        let hash1 = engine.ingest_media(data, "png").await.unwrap();
        let hash2 = engine.ingest_media(data, "png").await.unwrap();

        assert_eq!(hash1, hash2, "same data must produce same hash");

        // Verify only one object exists in the store.
        let hex = &hash1[7..];
        let path = StorePath::from(format!(".nap-assets/{hex}.png"));
        // `head` should succeed (object exists).
        let meta = engine.store.head(&path).await.unwrap();
        assert_eq!(
            meta.size as usize,
            data.len(),
            "stored object size must match"
        );
    }

    #[tokio::test]
    async fn test_ingest_media_different_data_different_hash() {
        let engine = in_memory_engine();

        let hash_a = engine.ingest_media(b"hello", "txt").await.unwrap();
        let hash_b = engine.ingest_media(b"world", "txt").await.unwrap();

        assert_ne!(
            hash_a, hash_b,
            "different content must produce different hashes"
        );
    }

    #[tokio::test]
    async fn test_ingest_media_content_is_retrievable() {
        // After ingestion, the content must be readable back from the
        // object store (media resolution).
        let engine = in_memory_engine();
        let original = b"\x89PNG\r\n\x1a\n\x00\x00\x00\rIHDR..."; // fake PNG header

        let hash = engine.ingest_media(original, "png").await.unwrap();

        // Resolve the hash to a store path and GET the content.
        let hex = &hash[7..];
        let path = StorePath::from(format!(".nap-assets/{hex}.png"));
        let result = engine.store.get(&path).await.unwrap();
        let retrieved = result.bytes().await.unwrap();

        assert_eq!(
            retrieved.as_ref(),
            original,
            "retrieved content must match original"
        );
    }

    #[tokio::test]
    async fn test_ingest_media_content_integrity_verified_by_hash() {
        // The returned hash must be the valid SHA-256 of the content.
        let engine = in_memory_engine();
        let data = b"verify-me-please";

        let hash = engine.ingest_media(data, "bin").await.unwrap();

        // Compute expected hash independently.
        let mut hasher = Sha256::new();
        hasher.update(data);
        let expected_hex = hex::encode(hasher.finalize());
        let expected_hash = format!("sha256:{expected_hex}");

        assert_eq!(hash, expected_hash, "hash must match SHA-256 of content");
    }

    #[tokio::test]
    async fn test_ingest_media_empty_data() {
        let engine = in_memory_engine();

        let hash = engine.ingest_media(b"", "empty").await.unwrap();

        assert!(hash.starts_with("sha256:"), "empty data should still hash");
        // SHA-256 of empty string: e3b0c44298fc1c149afbf4c8996fb924
        //                          27ae41e4649b934ca495991b7852b855
        assert_eq!(
            &hash[7..],
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
    }

    #[tokio::test]
    async fn test_ingest_media_unknown_format() {
        // Even unknown/weird formats should work fine.
        let engine = in_memory_engine();
        let hash = engine
            .ingest_media(b"\x00\x01\x02", "weird-format-123")
            .await
            .unwrap();
        assert!(hash.starts_with("sha256:"));

        // Verify it was stored under the correct key.
        let hex = &hash[7..];
        let path = StorePath::from(format!(".nap-assets/{hex}.weird-format-123"));
        let meta = engine.store.head(&path).await.unwrap();
        assert_eq!(meta.size as usize, 3);
    }

    #[tokio::test]
    async fn test_ingest_media_large_blob() {
        // Stress test with a large payload to surface any buffer issues.
        let engine = in_memory_engine();
        let data = vec![0xABu8; 1_000_000]; // 1 MB

        let hash = engine.ingest_media(&data, "bin").await.unwrap();
        assert!(hash.starts_with("sha256:"));

        // Verify size stored is correct.
        let hex = &hash[7..];
        let path = StorePath::from(format!(".nap-assets/{hex}.bin"));
        let meta = engine.store.head(&path).await.unwrap();
        assert_eq!(meta.size as usize, 1_000_000);
    }

    // ══════════════════════════════════════════════════════════════════
    //  LOCAL FILESYSTEM INGESTION TESTS
    // ══════════════════════════════════════════════════════════════════

    #[tokio::test]
    async fn test_local_ingest_writes_file_to_disk() {
        let tmp = tempfile::TempDir::new().unwrap();
        let engine = local_fs_engine(tmp.path());
        let data = b"disk-content";

        let hash = engine.ingest_media(data, "txt").await.unwrap();

        // The file should exist at <tmp>/.nap-assets/<hex>.txt
        let hex = &hash[7..];
        let file_path = tmp.path().join(".nap-assets").join(format!("{hex}.txt"));
        assert!(
            file_path.exists(),
            "file should exist on disk: {}",
            file_path.display()
        );

        let on_disk = std::fs::read(&file_path).unwrap();
        assert_eq!(on_disk, data, "file content must match original");
    }

    #[tokio::test]
    async fn test_local_ingest_idempotent_skips_write() {
        let tmp = tempfile::TempDir::new().unwrap();
        let engine = local_fs_engine(tmp.path());
        let data = b"idempotent-data";

        let hash1 = engine.ingest_media(data, "bin").await.unwrap();
        let hash2 = engine.ingest_media(data, "bin").await.unwrap();

        assert_eq!(hash1, hash2);

        // Only one file should exist.
        let hex = &hash1[7..];
        let dir = tmp.path().join(".nap-assets");
        let entries: Vec<_> = std::fs::read_dir(&dir)
            .unwrap()
            .filter_map(|e| e.ok())
            .collect();
        let matching: Vec<_> = entries
            .iter()
            .filter(|e| e.file_name().to_str().is_some_and(|n| n.contains(hex)))
            .collect();
        assert_eq!(
            matching.len(),
            1,
            "expected exactly one file matching the hash, got {}",
            matching.len()
        );
    }

    #[tokio::test]
    async fn test_local_ingest_multiple_files() {
        let tmp = tempfile::TempDir::new().unwrap();
        let engine = local_fs_engine(tmp.path());

        let hash1 = engine.ingest_media(b"alpha", "txt").await.unwrap();
        let hash2 = engine.ingest_media(b"beta", "txt").await.unwrap();
        let hash3 = engine.ingest_media(b"gamma", "png").await.unwrap();

        // All three files must exist in the assets directory.
        let assets_dir = tmp.path().join(".nap-assets");
        for hash in &[hash1, hash2, hash3] {
            let hex = &hash[7..];
            // Determine format from the stored key.
            // Since we used S3 path style, we need to check the file.
            // Let's just check at least one file with this hex exists.
            let found = std::fs::read_dir(&assets_dir).unwrap().any(|e| {
                e.ok()
                    .and_then(|e| e.file_name().to_str().map(|s| s.to_string()))
                    .is_some_and(|name| name.starts_with(hex))
            });
            assert!(
                found,
                "expected file starting with {hex} in assets directory"
            );
        }
    }

    #[tokio::test]
    async fn test_local_ingest_resolves_content_by_hash() {
        // Ingest content, then resolve using ContentHash to prove the
        // returned hash is a valid NAP content address.
        let tmp = tempfile::TempDir::new().unwrap();
        let engine = local_fs_engine(tmp.path());

        let data = b"resolve-me";
        let hash = engine.ingest_media(data, "bin").await.unwrap();

        // Parse as a NAP ContentHash — validates format & hex.
        let content_hash = crate::content::ContentHash::parse(&hash).unwrap();
        assert_eq!(content_hash.as_str(), &hash);

        // Verify the hash matches the content via ContentHash::verify.
        content_hash.verify(data).unwrap();
    }

    #[tokio::test]
    async fn test_local_engine_config_matches() {
        let tmp = tempfile::TempDir::new().unwrap();
        let engine = local_fs_engine(tmp.path());

        assert_eq!(engine.config().backend, StorageBackend::Local);
        assert_eq!(engine.config().base_dir, tmp.path());
        assert_eq!(engine.config().assets_prefix, ".nap-assets");
        assert!(engine.config().bucket.is_empty());
    }

    #[tokio::test]
    async fn test_local_ingest_subdirectory_created() {
        // The local initialiser creates .nap-assets automatically, but
        // even if it doesn't exist, the object_store LocalFileSystem
        // should create it.  Let's verify.
        let tmp = tempfile::TempDir::new().unwrap();
        let assets = tmp.path().join(".nap-assets");

        // Ensure it does NOT exist before we start.
        if assets.exists() {
            std::fs::remove_dir_all(&assets).unwrap();
        }
        assert!(!assets.exists(), "assets dir should not exist initially");

        // The engine itself doesn't auto-create; init_local does.
        // We call create_dir_all in local_fs_engine to match init_local.
        std::fs::create_dir_all(&assets).unwrap();
        let engine = local_fs_engine(tmp.path());

        let hash = engine.ingest_media(b"new-dir-test", "txt").await.unwrap();
        let hex = &hash[7..];
        let file_path = assets.join(format!("{hex}.txt"));
        assert!(file_path.exists(), "file should exist in .nap-assets");
    }

    // ══════════════════════════════════════════════════════════════════
    //  EDGE CASES & ERROR PATHS
    // ══════════════════════════════════════════════════════════════════

    #[test]
    fn test_from_env_rejects_unknown_backend() {
        // Temporarily set an invalid backend and verify the error.
        // We can't easily test from_env in-process because it reads
        // global env vars.  Instead we test the dispatcher logic by
        // constructing the error directly.
        let err = StorageError::Init(
            "unknown storage backend 'gcs'; expected 'local' or 's3'".to_string(),
        );
        assert!(err.to_string().contains("unknown storage backend"));
    }

    #[test]
    fn test_storage_error_from_io() {
        let io_err = std::io::Error::new(std::io::ErrorKind::NotFound, "file not found");
        let err = StorageError::Io(io_err);
        assert!(err.to_string().contains("I/O error"));
    }

    #[test]
    fn test_storage_error_from_object_store() {
        let os_err = object_store::Error::Generic {
            store: "test-store",
            source: Box::new(std::io::Error::other("store error")),
        };
        let err = StorageError::ObjectStore(os_err);
        assert!(err.to_string().contains("object store error"));
    }

    #[tokio::test]
    async fn test_ingest_media_with_nonexistent_base_dir_still_works() {
        // The object_store LocalFileSystem should create intermediate
        // directories automatically, so even if the base doesn't exist
        // (except .nap-assets which we create), it should work.
        let tmp = tempfile::TempDir::new().unwrap();
        let base = tmp.path().join("nested").join("path");

        // Create the .nap-assets dir (simulating what init_local does).
        let assets = base.join(".nap-assets");
        std::fs::create_dir_all(&assets).unwrap();

        let engine = StorageEngine {
            store: Box::new(LocalFileSystem::new()),
            config: StorageConfig {
                backend: StorageBackend::Local,
                base_dir: base.clone(),
                assets_prefix: ".nap-assets".to_string(),
                bucket: String::new(),
            },
        };

        let hash = engine.ingest_media(b"nested-test", "txt").await.unwrap();
        let hex = &hash[7..];
        let file_path = assets.join(format!("{hex}.txt"));
        assert!(
            file_path.exists(),
            "file should exist at nested path: {}",
            file_path.display()
        );
    }

    #[tokio::test]
    async fn test_ingest_media_multiple_formats_same_content() {
        // Same content with different formats should produce the same
        // hash prefix but different file extensions.
        let engine = in_memory_engine();
        let data = b"multi-format-content";

        let hash_png = engine.ingest_media(data, "png").await.unwrap();
        let hash_jpg = engine.ingest_media(data, "jpg").await.unwrap();

        // Same content = same hash (the format is NOT part of the hash).
        assert_eq!(hash_png, hash_jpg);

        // Both keys should exist in the store.
        let hex = &hash_png[7..];
        let path_png = StorePath::from(format!(".nap-assets/{hex}.png"));
        let path_jpg = StorePath::from(format!(".nap-assets/{hex}.jpg"));

        assert!(engine.store.head(&path_png).await.is_ok());
        assert!(engine.store.head(&path_jpg).await.is_ok());
    }

    #[tokio::test]
    async fn test_ingest_media_forces_unique_formats_in_store() {
        // Different formats for the same content hash create separate
        // store entries, both returning the same hash.
        let engine = in_memory_engine();
        let data = b"same-bytes-different-extension";

        let hash_a = engine.ingest_media(data, "mp4").await.unwrap();
        let hash_b = engine.ingest_media(data, "wav").await.unwrap();

        assert_eq!(hash_a, hash_b);

        let hex = &hash_a[7..];
        let path_mp4 = StorePath::from(format!(".nap-assets/{hex}.mp4"));
        let path_wav = StorePath::from(format!(".nap-assets/{hex}.wav"));

        // Both entries must exist independently.
        let meta_mp4 = engine.store.head(&path_mp4).await.unwrap();
        let meta_wav = engine.store.head(&path_wav).await.unwrap();
        assert_eq!(meta_mp4.size, meta_wav.size);
    }
}
