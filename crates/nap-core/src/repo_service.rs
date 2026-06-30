//! RepoService — the stable, high-level boundary between NAP consumer
//! code and the Lore VCS adapter layer.
//!
//! ## Design principle
//!
//! **No call site outside the Lore adapter may shell out to `lore`, link
//! against a Lore client library, or assume git/SVN semantics.**
//!
//! `RepoService` is the **only interface** through which the rest of
//! nap-sdk touches version control.  It wraps:
//!
//! - A [`VcsBackend`] implementation (production: [`LoreBackend`])
//! - A [`PermissionGate`]
//! - A [`ContextDocsManager`]
//! - An optional [`AutopublishWorker`]
//!
//! ## Lifecycle
//!
//! ```ignore
//! // 1. Create a repository (server-side).
//! let service = RepoService::new(backend, workspace_root)?;
//! let repo = service.create_repository("my-workspace", "my-repo", RepoOptions::default())?;
//!
//! // 2. Get or open a workspace (local checkout).
//! let ws = service.open_workspace()?;
//!
//! // 3. Use the workspace.
//! service.write_file("characters/hero.yaml", "...", "alice")?;
//! service.commit("add hero", "alice")?;
//!
//! // 4. Autopublish for long-running agents.
//! let handle = service.start_autopublish(AutopublishConfig::default())?;
//! ```

use std::path::{Path, PathBuf};

use crate::autopublish::{AutopublishConfig, AutopublishHandle, AutopublishWorker};
use crate::context_docs::ContextDocsManager;
use crate::error::NapError;
use crate::permission_gate::PermissionGate;
use crate::vcs::ContextDocument;
use crate::vcs::{CommitInfo, Repository, VcsBackend, Workspace, WorkspaceMode};
use crate::vcs_lore::LoreBackend;

// ---------------------------------------------------------------------------
// RepoOptions
// ---------------------------------------------------------------------------

/// Options for creating a new repository.
#[derive(Debug, Clone)]
pub struct RepoOptions {
    /// Human-readable description.
    pub description: String,
    /// Whether to make the repository public on the lore server.
    pub public: bool,
    /// Initial branch name.
    pub default_branch: String,
}

impl Default for RepoOptions {
    fn default() -> Self {
        Self {
            description: String::new(),
            public: false,
            default_branch: "main".to_string(),
        }
    }
}

// ---------------------------------------------------------------------------
// RepoService
// ---------------------------------------------------------------------------

/// High-level interface for all VCS operations in NAP.
///
/// ## Thread safety
///
/// `RepoService` is `Send + Sync` and safe to share across threads.
/// Internal state (e.g., the permission gate cache, context-doc graph)
/// uses interior mutability.
pub struct RepoService {
    /// VCS backend (production: [`LoreBackend`]).
    backend: Box<dyn VcsBackend>,
    /// Workspace root path on disk.
    workspace_root: PathBuf,
    /// Permission gate.
    pub permission_gate: PermissionGate,
    /// Context-document manager.
    pub context_docs: ContextDocsManager,
}

impl RepoService {
    /// Create a new `RepoService` with the given backend and workspace path.
    ///
    /// The permission gate is loaded from `context/nap-gate.toml` if it
    /// exists, otherwise a permissive gate is used.  Use
    /// [`RepoService::with_gate`] for custom gate behaviour.
    pub fn new(backend: Box<dyn VcsBackend>, workspace_root: &Path) -> Result<Self, NapError> {
        let permission_gate = PermissionGate::load(workspace_root)?;
        let context_docs = ContextDocsManager::new(workspace_root);

        Ok(Self {
            backend,
            workspace_root: workspace_root.to_path_buf(),
            permission_gate,
            context_docs,
        })
    }

    /// Create a `RepoService` with an explicit permission gate.
    pub fn with_gate(
        backend: Box<dyn VcsBackend>,
        workspace_root: &Path,
        permission_gate: PermissionGate,
    ) -> Self {
        let context_docs = ContextDocsManager::new(workspace_root);
        Self {
            backend,
            workspace_root: workspace_root.to_path_buf(),
            permission_gate,
            context_docs,
        }
    }

    /// Create a `RepoService` from environment variables (for the common
    /// local-dev case).
    pub fn from_env(workspace_root: &Path) -> Result<Self, NapError> {
        let backend: Box<dyn VcsBackend> = Box::new(LoreBackend::from_env());
        Self::new(backend, workspace_root)
    }

    /// The underlying VCS backend (for advanced use).
    pub fn backend(&self) -> &dyn VcsBackend {
        self.backend.as_ref()
    }

    /// Workspace root path.
    pub fn workspace_root(&self) -> &Path {
        &self.workspace_root
    }

    // ── Repository lifecycle ─────────────────────────────────────────

    /// Create a repository on the lore server and clone it locally.
    ///
    /// This is the primary way to bootstrap a NAP workspace.
    pub fn create_repository(
        &self,
        workspace_id: &str,
        repo_id: &str,
        _opts: RepoOptions,
    ) -> Result<Repository, NapError> {
        let remote_url = format!(
            "{}/{}",
            std::env::var("NAP_LORE_URL_BASE")
                .unwrap_or_else(|_| "lore://localhost:8700".to_string()),
            repo_id
        );

        // Init creates the remote repo + clones locally.
        self.backend.init(&self.workspace_root)?;

        Ok(Repository {
            id: repo_id.to_string(),
            workspace_id: workspace_id.to_string(),
            remote_url,
        })
    }

    /// Open an existing local workspace (assumes `lore clone` already
    /// happened, or the workspace directory already exists).
    pub fn open_workspace(&self) -> Result<Workspace, NapError> {
        if !self.workspace_root.exists() {
            return Err(NapError::VcsError(format!(
                "workspace does not exist: {:?}",
                self.workspace_root
            )));
        }

        let branch = self.backend.current_branch(&self.workspace_root)?;
        let repo_id = self
            .workspace_root
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("unknown")
            .to_string();

        Ok(Workspace {
            repository_id: repo_id,
            path: self.workspace_root.to_string_lossy().to_string(),
            branch,
            mode: WorkspaceMode::Durable,
        })
    }

    // ── Entity CRUD (with permission checks) ─────────────────────────

    /// Write a file, checking permissions first.
    pub fn write_file(&self, path: &str, content: &str, principal: &str) -> Result<(), NapError> {
        self.permission_gate.check_write(path, principal)?;

        let full_path = self.workspace_root.join(path);
        if let Some(parent) = full_path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| {
                NapError::Other(format!(
                    "failed to create parent directory for '{}': {}",
                    path, e
                ))
            })?;
        }

        std::fs::write(&full_path, content).map_err(NapError::Io)?;

        Ok(())
    }

    /// Read a file, checking permissions first.
    pub fn read_file(&self, path: &str, principal: &str) -> Result<String, NapError> {
        self.permission_gate.check_read(path, principal)?;

        let full_path = self.workspace_root.join(path);
        std::fs::read_to_string(&full_path).map_err(NapError::Io)
    }

    // ── VCS operations ───────────────────────────────────────────────

    /// Commit staged changes.
    pub fn commit(&self, message: &str, author: &str) -> Result<String, NapError> {
        self.backend.commit(&self.workspace_root, message, author)
    }

    /// Get commit history.
    pub fn log(&self, file: Option<&str>, limit: usize) -> Result<Vec<CommitInfo>, NapError> {
        self.backend.log(&self.workspace_root, file, limit)
    }

    /// Create a branch.
    pub fn create_branch(&self, name: &str) -> Result<(), NapError> {
        self.backend.create_branch(&self.workspace_root, name)
    }

    /// Switch to a branch.
    pub fn switch_branch(&self, name: &str) -> Result<(), NapError> {
        self.backend.switch_branch(&self.workspace_root, name)
    }

    /// Get current branch.
    pub fn current_branch(&self) -> Result<String, NapError> {
        self.backend.current_branch(&self.workspace_root)
    }

    /// List branches.
    pub fn list_branches(&self) -> Result<Vec<String>, NapError> {
        self.backend.list_branches(&self.workspace_root)
    }

    /// Create a tag.
    pub fn create_tag(&self, name: &str) -> Result<(), NapError> {
        self.backend.create_tag(&self.workspace_root, name)
    }

    /// List tags.
    pub fn list_tags(&self) -> Result<Vec<String>, NapError> {
        self.backend.list_tags(&self.workspace_root)
    }

    /// Push to the lore server.
    pub fn push(&self, remote: Option<&str>, branch: Option<&str>) -> Result<(), NapError> {
        self.backend.push(&self.workspace_root, remote, branch)
    }

    /// Pull from the lore server.
    pub fn pull(&self, remote: Option<&str>, branch: Option<&str>) -> Result<(), NapError> {
        self.backend.pull(&self.workspace_root, remote, branch)
    }

    // ── Context documents ────────────────────────────────────────────

    /// Register a context document.  See [`ContextDocsManager::register`].
    pub fn register_context_doc(
        &self,
        path: &str,
        metadata: &[(&str, &str)],
    ) -> Result<(), NapError> {
        self.context_docs.register(path, metadata)
    }

    /// Add a dependency between context documents.
    pub fn add_context_dep(&self, source: &str, target: &str) -> Result<(), NapError> {
        self.context_docs.add_dependency(source, target)
    }

    /// Get all context documents.
    pub fn all_context_docs(&self) -> Result<Vec<ContextDocument>, NapError> {
        self.context_docs.all_documents()
    }

    // ── Autopublish ──────────────────────────────────────────────────

    /// Start the autopublish worker.
    ///
    /// The worker takes ownership of a **new** backend instance (created
    /// from env variables) so the shared `RepoService` backend is not
    /// consumed.  If you need a worker with a specific backend, construct
    /// [`AutopublishWorker`] directly.
    pub fn start_autopublish(
        &self,
        config: AutopublishConfig,
    ) -> Result<AutopublishHandle, NapError> {
        let worker_backend: Box<dyn VcsBackend> = Box::new(LoreBackend::from_env());
        let worker = AutopublishWorker::new(self.workspace_root.clone(), worker_backend, config);
        worker.start()
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::vcs::AccessLevel;

    #[test]
    fn test_open_workspace_fails_on_missing() {
        let backend = LoreBackend::new("lore://localhost:8700", "test");
        let service = RepoService::with_gate(
            Box::new(backend),
            Path::new("/nonexistent-12345"),
            PermissionGate::permissive(Path::new("/nonexistent-12345")),
        );
        let result = service.open_workspace();
        assert!(result.is_err());
        assert!(
            result.unwrap_err().to_string().contains("does not exist"),
            "expected 'does not exist'"
        );
    }

    #[test]
    fn test_write_file_checks_permissions() {
        let dir = tempfile::TempDir::new().unwrap();
        let perms = vec![crate::vcs::Permission {
            path_prefix: "/restricted".to_string(),
            principal: "alice".to_string(),
            access: AccessLevel::Write,
        }];
        let gate = PermissionGate::from_permissions(dir.path(), &perms, AccessLevel::None);
        let backend = LoreBackend::new("lore://localhost:8700", "test");

        let service = RepoService::with_gate(Box::new(backend), dir.path(), gate);

        // Alice can write to restricted.
        assert!(
            service
                .write_file("restricted/secret.txt", "data", "alice")
                .is_ok()
        );

        // Bob cannot.
        let result = service.write_file("restricted/secret.txt", "data", "bob");
        assert!(result.is_err());
        assert!(
            result.unwrap_err().to_string().contains("denied"),
            "expected permission denied"
        );
    }

    #[test]
    fn test_read_file_checks_permissions() {
        let dir = tempfile::TempDir::new().unwrap();
        let perms = vec![crate::vcs::Permission {
            path_prefix: "/".to_string(),
            principal: "*".to_string(),
            access: AccessLevel::Read,
        }];
        let gate = PermissionGate::from_permissions(dir.path(), &perms, AccessLevel::None);
        let backend = LoreBackend::new("lore://localhost:8700", "test");

        let service = RepoService::with_gate(Box::new(backend), dir.path(), gate);

        // Write a file (bypassing gate for setup).
        std::fs::write(dir.path().join("readme.md"), "hello").unwrap();

        // Anyone can read.
        assert!(service.read_file("readme.md", "bob").is_ok());

        // But write is denied (read-only default on "/").
        let result = service.write_file("newfile.txt", "data", "bob");
        assert!(result.is_err());
    }

    #[test]
    fn test_context_docs_integration() {
        let dir = tempfile::TempDir::new().unwrap();
        let backend = LoreBackend::new("lore://localhost:8700", "test");
        let service = RepoService::with_gate(
            Box::new(backend),
            dir.path(),
            PermissionGate::permissive(dir.path()),
        );

        service
            .register_context_doc("task.md", &[("status", "active")])
            .unwrap();
        service.add_context_dep("task.md", "spec.md").unwrap();

        let docs = service.all_context_docs().unwrap();
        // task.md should exist (spec.md was auto-vivified by context_docs)
        let task_doc = docs.iter().find(|d| d.path == "task.md");
        assert!(task_doc.is_some(), "task.md should be in context docs");
        assert_eq!(task_doc.unwrap().metadata.get("status").unwrap(), "active");
    }
}
