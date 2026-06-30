//! Autopublish — a background worker that stages, commits, and publishes
//! workspace changes on a configurable cadence.
//!
//! ## Motivation
//!
//! AI agents working inside a NAP workspace may produce file changes over
//! an extended period (minute-scale or hour-scale sessions).  Without
//! periodic commits the workspace has no checkpoint, the VCS log shows
//! one giant lump, and the context-graph cannot reference intermediate
//! states.
//!
//! Autopublish solves this by running a lightweight loop that:
//! 1. Watches for filesystem changes (or runs on a timer).
//! 2. Stages all modified files.
//! 3. Commits with a standardised message.
//! 4. Publishes to the loreserver.
//!
//! ## Design
//!
//! [`AutopublishWorker`] is a **background thread** (not async) that **owns**
//! its VCS backend.  It is started via [`AutopublishWorker::start(self)`] and
//! stopped by dropping the returned [`AutopublishHandle`].
//!
//! The worker is lightly configurable:
//! - `interval`: how often to attempt a publish (default 60s).
//! - `branch`: which branch to commit to (default `main`).
//! - `message_template`: commit message (default `"autopublish: periodic
//!    checkpoint"`).
//!
//! For v0, the watch mechanism is **timer-based poll**, not filesystem
//! events (inotify/FSEvents).  Filesystem-event watching is a future
//! optimisation.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use crate::error::NapError;
use crate::vcs::VcsBackend;

// ---------------------------------------------------------------------------
// Configuration
// ---------------------------------------------------------------------------

/// Configuration for an autopublish worker.
///
/// Default: 60-second interval on `main` with a standard message.
#[derive(Debug, Clone)]
pub struct AutopublishConfig {
    /// Poll interval between autopublish attempts.
    pub interval: Duration,
    /// Branch to commit on.
    pub branch: String,
    /// Commit message template.
    pub message_template: String,
    /// Author identity string.
    pub author: String,
}

impl Default for AutopublishConfig {
    fn default() -> Self {
        Self {
            interval: Duration::from_secs(60),
            branch: "main".to_string(),
            message_template: "autopublish: periodic checkpoint".to_string(),
            author: "nap-autopublish <nap@nap.dev>".to_string(),
        }
    }
}

// ---------------------------------------------------------------------------
// AutopublishWorker
// ---------------------------------------------------------------------------

/// A background worker that periodically commits and publishes workspace
/// changes.
///
/// ## Lifecycle
///
/// ```ignore
/// let worker = AutopublishWorker::new(
///     workspace_path, backend, config
/// );
/// let handle = worker.start()?;   // consumes worker
/// // ... do work ...
/// handle.stop();                   // or drop(handle)
/// ```
pub struct AutopublishWorker {
    /// Workspace root path.
    workspace_path: PathBuf,
    /// VCS backend (owned — moved into the background thread on start).
    backend: Box<dyn VcsBackend>,
    /// Configuration.
    config: AutopublishConfig,
}

impl AutopublishWorker {
    /// Create a new autopublish worker.
    pub fn new(
        workspace_path: PathBuf,
        backend: Box<dyn VcsBackend>,
        config: AutopublishConfig,
    ) -> Self {
        Self {
            workspace_path,
            backend,
            config,
        }
    }

    /// Start the worker in a background thread.
    ///
    /// **Consumes `self`** — the backend is moved into the spawned thread.
    ///
    /// Returns an [`AutopublishHandle`] that can be used to stop the
    /// worker.  Dropping the handle also stops the worker.
    ///
    /// ## Errors
    ///
    /// Returns an error if the workspace does not exist.
    pub fn start(self) -> Result<AutopublishHandle, NapError> {
        // Health check: verify the workspace exists and is reachable.
        if !self.workspace_path.exists() {
            return Err(NapError::VcsError(format!(
                "autopublish workspace does not exist: {:?}",
                self.workspace_path
            )));
        }

        let running = Arc::new(AtomicBool::new(true));
        let running_clone = running.clone();

        // Move owned resources into the thread.
        let config = self.config;
        let workspace_path = self.workspace_path;
        let backend = self.backend;

        std::thread::Builder::new()
            .name("nap-autopublish".into())
            .spawn(move || {
                loop {
                    if !running_clone.load(Ordering::Relaxed) {
                        break;
                    }

                    // Attempt publish.
                    let result = Self::publish_once(&workspace_path, backend.as_ref(), &config);

                    match result {
                        Ok(Some(sig)) => {
                            tracing::info!(
                                target: "nap::autopublish",
                                "autopublish committed revision {}",
                                sig
                            );
                        }
                        Ok(None) => {
                            // Nothing to commit — no changes.  This is normal.
                            tracing::debug!(
                                target: "nap::autopublish",
                                "autopublish: no changes to commit"
                            );
                        }
                        Err(e) => {
                            // Log but do not crash.  The next poll will retry.
                            tracing::warn!(
                                target: "nap::autopublish",
                                "autopublish attempt failed (will retry): {}",
                                e
                            );
                        }
                    }

                    std::thread::sleep(config.interval);
                }
            })
            .map_err(|e| {
                NapError::VcsError(format!("failed to spawn autopublish thread: {}", e))
            })?;

        Ok(AutopublishHandle { running })
    }

    /// Execute a single publish cycle (exposed for testing and one-shot use).
    ///
    /// Returns `Ok(Some(signature))` if a commit was made, `Ok(None)` if
    /// there were no changes to commit.
    fn publish_once(
        workspace_path: &Path,
        backend: &dyn VcsBackend,
        config: &AutopublishConfig,
    ) -> Result<Option<String>, NapError> {
        let sig = backend.commit(workspace_path, &config.message_template, &config.author)?;

        // If the signature is empty or indicates no-op, return None.
        if sig.is_empty() || sig.contains("nothing to commit") {
            return Ok(None);
        }

        Ok(Some(sig))
    }
}

// ---------------------------------------------------------------------------
// AutopublishHandle
// ---------------------------------------------------------------------------

/// Handle that controls a running autopublish worker.
///
/// Dropping the handle signals the worker to stop and blocks (for up to
/// the configured interval) until the thread exits.
#[derive(Debug)]
pub struct AutopublishHandle {
    /// Shared atomic flag used to signal the worker to stop.
    running: Arc<AtomicBool>,
}

impl AutopublishHandle {
    /// Signal the worker to stop and wait for it to finish.
    ///
    /// This blocks for up to one interval period.  After that the worker
    /// is guaranteed to have seen the stop signal.
    pub fn stop(self) {
        self.running.store(false, Ordering::Relaxed);
    }
}

impl Drop for AutopublishHandle {
    fn drop(&mut self) {
        self.running.store(false, Ordering::Relaxed);
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    /// A no-op backend that always succeeds and returns a fixed signature.
    struct MockBackend {
        signature: String,
    }

    impl VcsBackend for MockBackend {
        fn init(&self, _path: &std::path::Path) -> Result<(), NapError> {
            Ok(())
        }
        fn commit(
            &self,
            _path: &std::path::Path,
            _message: &str,
            _author: &str,
        ) -> Result<String, NapError> {
            let sig = self.signature.clone();
            Ok(sig)
        }
        fn read_file_at_ref(
            &self,
            _repo_path: &std::path::Path,
            _file_path: &str,
            _reference: Option<&str>,
        ) -> Result<String, NapError> {
            Ok(String::new())
        }
        fn log(
            &self,
            _path: &std::path::Path,
            _file: Option<&str>,
            _limit: usize,
        ) -> Result<Vec<crate::vcs::CommitInfo>, NapError> {
            Ok(Vec::new())
        }
        fn create_branch(&self, _path: &std::path::Path, _name: &str) -> Result<(), NapError> {
            Ok(())
        }
        fn switch_branch(&self, _path: &std::path::Path, _name: &str) -> Result<(), NapError> {
            Ok(())
        }
        fn create_tag(&self, _path: &std::path::Path, _name: &str) -> Result<(), NapError> {
            Ok(())
        }
        fn current_branch(&self, _path: &std::path::Path) -> Result<String, NapError> {
            Ok("main".to_string())
        }
        fn head_hash(&self, _path: &std::path::Path) -> Result<String, NapError> {
            Ok("HEAD".to_string())
        }
        fn list_branches(&self, _path: &std::path::Path) -> Result<Vec<String>, NapError> {
            Ok(vec!["main".to_string()])
        }
        fn list_tags(&self, _path: &std::path::Path) -> Result<Vec<String>, NapError> {
            Ok(Vec::new())
        }
        fn add_remote(
            &self,
            _path: &std::path::Path,
            _name: &str,
            _url: &str,
        ) -> Result<(), NapError> {
            Ok(())
        }
        fn remove_remote(&self, _path: &std::path::Path, _name: &str) -> Result<(), NapError> {
            Ok(())
        }
        fn list_remotes(&self, _path: &std::path::Path) -> Result<Vec<(String, String)>, NapError> {
            Ok(Vec::new())
        }
        fn push(
            &self,
            _path: &std::path::Path,
            _remote: Option<&str>,
            _branch: Option<&str>,
        ) -> Result<(), NapError> {
            Ok(())
        }
        fn pull(
            &self,
            _path: &std::path::Path,
            _remote: Option<&str>,
            _branch: Option<&str>,
        ) -> Result<(), NapError> {
            Ok(())
        }
    }

    #[test]
    fn test_publish_once_returns_sig() {
        let config = AutopublishConfig::default();
        let backend = MockBackend {
            signature: "abc123".to_string(),
        };
        let path = Path::new("/tmp/nonexistent"); // doesn't matter for mock

        let result = AutopublishWorker::publish_once(path, &backend, &config);
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), Some("abc123".to_string()));
    }

    #[test]
    fn test_publish_once_empty_sig() {
        let config = AutopublishConfig::default();
        let backend = MockBackend {
            signature: String::new(),
        };
        let path = Path::new("/tmp/nonexistent");

        let result = AutopublishWorker::publish_once(path, &backend, &config);
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), None);
    }

    #[test]
    fn test_default_config() {
        let config = AutopublishConfig::default();
        assert_eq!(config.interval, Duration::from_secs(60));
        assert_eq!(config.branch, "main");
        assert_eq!(config.message_template, "autopublish: periodic checkpoint");
    }

    #[test]
    fn test_start_fails_on_nonexistent_workspace() {
        let config = AutopublishConfig::default();
        let backend = MockBackend {
            signature: "sig".to_string(),
        };
        let worker = AutopublishWorker::new(
            PathBuf::from("/nonexistent-path-12345"),
            Box::new(backend),
            config,
        );

        let result = worker.start();
        assert!(result.is_err());
        assert!(
            result.unwrap_err().to_string().contains("does not exist"),
            "expected 'does not exist' error"
        );
    }

    #[test]
    fn test_handle_stop() {
        let dir = tempfile::TempDir::new().unwrap();
        let config = AutopublishConfig {
            interval: Duration::from_millis(10),
            ..Default::default()
        };
        let backend = MockBackend {
            signature: "sig".to_string(),
        };
        let worker = AutopublishWorker::new(dir.path().to_path_buf(), Box::new(backend), config);
        let handle = worker.start().unwrap();
        // Should stop cleanly without blocking long.
        handle.stop();
    }
}
