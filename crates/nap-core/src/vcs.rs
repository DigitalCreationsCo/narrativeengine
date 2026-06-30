//! VCS backend abstraction and Lore VCS type system.
//!
//! NAP's v0 used Git (via `git` CLI).  As of the Lore VCS migration, the
//! default and only production backend is **Lore** — a centralized VCS with
//! global revision numbers, file-level metadata, dependency graphs, and
//! git-style branching.
//!
//! The [`VcsBackend`] trait is the low-level seam between NAP and any VCS.
//! [`LoreBackend`](crate::vcs_lore::LoreBackend) is the only production
//! implementation.  Higher-level workflows (context docs, permissions,
//! autopublish) live in [`RepoService`].
//!
//! ## Architecture
//!
//! ```text
//! Consumer code → RepoService (stable boundary)
//!                     │
//!                     ▼
//!               VcsBackend trait
//!                     │
//!               LoreBackend (adapter)
//!                     │
//!               LoreProcessRunner (CLI executor)
//!                     │
//!               loreserver (authoritative store)
//! ```

use std::path::Path;

use crate::error::NapError;

// ---------------------------------------------------------------------------
// Core VCS types (Lore-native, also serve as the RepoService vocabulary)
// ---------------------------------------------------------------------------

/// A Lore repository identity — analogous to a Git remote, but with a
/// workspace-scoped multi-tenant owner.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct Repository {
    /// Stable internal identifier, set via `lore repository create --id`.
    pub id: String,
    /// Workspace that owns this repository (multi-tenancy boundary).
    pub workspace_id: String,
    /// Lore `lore://` remote URL on the loreserver.
    pub remote_url: String,
}

/// A local working copy of a Lore repository.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct Workspace {
    /// The repository this workspace belongs to.
    pub repository_id: String,
    /// Local filesystem path to the working tree.
    pub path: String,
    /// Current branch.
    pub branch: String,
    /// Whether this workspace is durable, ephemeral, or virtual.
    pub mode: WorkspaceMode,
}

/// How a workspace tracks state locally.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum WorkspaceMode {
    /// Full local working tree with tracking (default for interactive use).
    Durable,
    /// Memory-only tracking — no local repo state left behind (for agents).
    Ephemeral,
    /// Split-write filesystem — like ephemeral but with a writable overlay.
    Virtual,
}

/// A single revision (commit) in the Lore VCS.
///
/// Lore revisions have both a content-hash **signature** (like a Git SHA)
/// and a monotonically incrementing global **number** (like an SVN revision
/// or Perforce changelist).  NAP exposes both.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct Revision {
    /// Lore revision hash signature (content-addressed).
    pub signature: String,
    /// Lore global revision number (monotonic, cross-branch).
    pub number: u64,
    /// Branch this revision was committed on.
    pub branch: String,
    /// Commit message.
    pub message: String,
    /// Author identity string.
    pub author: String,
    /// Parent revision signature, if any.
    pub parent_signature: Option<String>,
}

/// A label (tag) attached to a revision via Lore metadata.
///
/// Lore has no first-class "tag" object — labels are stored as metadata
/// under the reserved key `nap.labels`.  See the [`LabelConvention`]
/// documentation for how tags round-trip through the metadata system.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct Label {
    /// The revision this label points to.
    pub revision_signature: String,
    /// Label/tag names applied to this revision.
    pub names: Vec<String>,
}

/// A directory- or file-level access-control entry.
///
/// Lore's stock server has no native path ACL — this is enforced at the
/// application layer by [`PermissionGate`](crate::permission_gate::PermissionGate).
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct Permission {
    /// Directory or file path prefix this rule applies to.
    pub path_prefix: String,
    /// User or role identifier.
    pub principal: String,
    /// Granted access level.
    pub access: AccessLevel,
}

/// Access level for a permission entry.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum AccessLevel {
    /// Read-only access.
    Read,
    /// Read + write access.
    Write,
    /// No access (explicit deny).
    None,
}

/// A contextual document tracked in the Lore VCS with associated metadata
/// and dependency edges for AI context-graph assembly.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ContextDocument {
    /// Path within the repository (e.g. `/context/task-123.md`).
    pub path: String,
    /// Arbitrary key-value metadata stored via `lore file metadata set`.
    pub metadata: std::collections::HashMap<String, String>,
    /// Other files this document depends on (the AI relevance graph).
    pub depends_on: Vec<String>,
}

/// Metadata about a single VCS commit, returned by `log()`.
/// Kept for backward compatibility with existing callers.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct CommitInfo {
    /// The commit hash/identifier (Lore revision signature).
    pub id: String,
    /// Parent commit hash (None for root).
    pub parent: Option<String>,
    /// Commit author.
    pub author: String,
    /// Commit message.
    pub message: String,
    /// Commit timestamp (RFC 3339).
    pub timestamp: String,
}

// ---------------------------------------------------------------------------
// VcsBackend trait — low-level VCS abstraction
// ---------------------------------------------------------------------------

/// Low-level abstraction over a version control system.
///
/// Implementors: [`GitBackend`](crate::vcs_git::GitBackend) (deprecated),
/// [`LoreBackend`](crate::vcs_lore::LoreBackend) (production).
///
/// Most consumer code should use [`RepoService`] instead — it adds
/// permissions, context-document management, autopublish, and a
/// workspace-lifecycle API on top of this trait.
pub trait VcsBackend: Send + Sync {
    /// Initialize a new repository at the given path.
    fn init(&self, path: &Path) -> Result<(), NapError>;

    /// Stage all files and create a commit.
    fn commit(&self, path: &Path, message: &str, author: &str) -> Result<String, NapError>;

    /// Read a file's content at a specific ref (branch, tag, or commit hash).
    /// If `reference` is None, reads from the current working tree.
    fn read_file_at_ref(
        &self,
        repo_path: &Path,
        file_path: &str,
        reference: Option<&str>,
    ) -> Result<String, NapError>;

    /// Get the commit log for the repository, optionally filtered to a specific file.
    fn log(
        &self,
        path: &Path,
        file: Option<&str>,
        limit: usize,
    ) -> Result<Vec<CommitInfo>, NapError>;

    /// Create a new branch.
    fn create_branch(&self, path: &Path, name: &str) -> Result<(), NapError>;

    /// Switch to a branch.
    fn switch_branch(&self, path: &Path, name: &str) -> Result<(), NapError>;

    /// Create a tag at the current HEAD.
    fn create_tag(&self, path: &Path, name: &str) -> Result<(), NapError>;

    /// Get the current branch name.
    fn current_branch(&self, path: &Path) -> Result<String, NapError>;

    /// Get the HEAD commit hash.
    fn head_hash(&self, path: &Path) -> Result<String, NapError>;

    /// Revert a commit by creating a new commit that undoes it.
    fn revert(&self, _path: &Path, _commit_hash: &str) -> Result<String, NapError> {
        Err(NapError::VcsError(
            "revert not supported by this VCS backend".to_string(),
        ))
    }

    /// List all branches.
    fn list_branches(&self, path: &Path) -> Result<Vec<String>, NapError>;

    /// List all tags.
    fn list_tags(&self, path: &Path) -> Result<Vec<String>, NapError>;

    // ── Remote operations ────────────────────────────────────────────

    /// Add a remote.
    fn add_remote(&self, path: &Path, name: &str, url: &str) -> Result<(), NapError>;

    /// Remove a remote.
    fn remove_remote(&self, path: &Path, name: &str) -> Result<(), NapError>;

    /// List remotes as `(name, url)` pairs.
    fn list_remotes(&self, path: &Path) -> Result<Vec<(String, String)>, NapError>;

    /// Push the current branch to its upstream / a named remote.
    fn push(&self, path: &Path, remote: Option<&str>, branch: Option<&str>)
    -> Result<(), NapError>;

    /// Pull the current branch from its upstream / a named remote.
    fn pull(&self, path: &Path, remote: Option<&str>, branch: Option<&str>)
    -> Result<(), NapError>;
}

// ---------------------------------------------------------------------------
// CommitInfo convenience — used by the resolver and history views
// ---------------------------------------------------------------------------

impl CommitInfo {
    /// Build a `CommitInfo` from a lore revision's structured output.
    /// The `timestamp` field is best-effort; Lore may not provide it in all
    /// output modes.
    pub fn from_lore_revision(
        signature: &str,
        parent: Option<&str>,
        author: &str,
        message: &str,
        timestamp: &str,
    ) -> Self {
        Self {
            id: signature.to_string(),
            parent: parent.map(|p| p.to_string()),
            author: author.to_string(),
            message: message.to_string(),
            timestamp: if timestamp.is_empty() {
                chrono::Utc::now().to_rfc3339()
            } else {
                timestamp.to_string()
            },
        }
    }
}
