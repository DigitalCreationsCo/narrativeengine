//! Universe repository — filesystem layout and manifest CRUD.
//!
//! A NAP repository represents a single fictional universe.
//! Repository structure:
//!
//! ```text
//! starwars/               ← universe root (Git repo)
//! ├── .nap/               ← NAP metadata
//! │   └── config.yaml     ← repository config
//! ├── universe.yaml       ← world manifest (root-level)
//! ├── characters/
//! │   ├── lukeskywalker.yaml
//! │   └── darthvader.yaml
//! ├── locations/
//! │   └── tatooine.yaml
//! ├── scenes/
//! │   └── cantina-scene.yaml
//! └── props/
//! ```

use std::path::{Path, PathBuf};

use tracing::{debug, info};

use crate::commit::{Change, Commit};
use crate::error::NapError;
use crate::manifest::Manifest;
use crate::types::EntityType;
use crate::uri::NapUri;
use crate::vcs::VcsBackend;

/// NAP metadata directory name.
const NAP_DIR: &str = ".nap";

/// A NAP universe repository.
pub struct Repository {
    /// Filesystem path to the repository root.
    pub root: PathBuf,
    /// The universe name (derived from directory name).
    pub universe: String,
    /// The VCS backend (Git, Fossil, etc.).
    vcs: Box<dyn VcsBackend>,
}

impl Repository {
    /// Open an existing NAP repository at the given path.
    pub fn open(path: &Path, vcs: Box<dyn VcsBackend>) -> Result<Self, NapError> {
        let nap_dir = path.join(NAP_DIR);
        if !nap_dir.exists() {
            return Err(NapError::RepositoryNotFound(path.display().to_string()));
        }
        let universe = path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("unknown")
            .to_string();

        debug!(
            path = %path.display(),
            universe = %universe,
            "opened NAP repository"
        );

        Ok(Self {
            root: path.to_path_buf(),
            universe,
            vcs,
        })
    }

    /// Initialize a new NAP repository.
    pub fn init(path: &Path, universe: &str, vcs: Box<dyn VcsBackend>) -> Result<Self, NapError> {
        let repo_root = path.join(universe);
        if repo_root.join(NAP_DIR).exists() {
            return Err(NapError::RepositoryAlreadyExists(
                repo_root.display().to_string(),
            ));
        }

        info!(
            path = %repo_root.display(),
            universe = %universe,
            "initializing NAP repository"
        );

        // Create directory structure
        std::fs::create_dir_all(&repo_root)?;
        std::fs::create_dir_all(repo_root.join(NAP_DIR))?;

        // Create entity type subdirectories
        for entity_type in EntityType::subdirectory_types() {
            std::fs::create_dir_all(repo_root.join(entity_type.directory_name()))?;
        }

        // Create .nap/config.yaml
        let config = format!(
            "# NAP Repository Configuration\nuniverse: {universe}\nprotocol_version: \"0.1.0\"\n"
        );
        std::fs::write(repo_root.join(NAP_DIR).join("config.yaml"), config)?;

        // Create universe.yaml (world manifest)
        let world_manifest = Manifest::new(
            universe,
            EntityType::World,
            universe,
            &format!("{universe} Universe"),
        );
        world_manifest.to_file(&repo_root.join("universe.yaml"))?;

        // Initialize VCS
        vcs.init(&repo_root)?;

        // Initial commit
        vcs.commit(
            &repo_root,
            &format!("Initialize {universe} universe"),
            "nap-init",
        )?;

        info!(
            path = %repo_root.display(),
            universe = %universe,
            "NAP repository initialized successfully"
        );

        Ok(Self {
            root: repo_root,
            universe: universe.to_string(),
            vcs,
        })
    }

    /// Get the full filesystem path to an entity's manifest file.
    pub fn manifest_path(&self, entity_type: EntityType, entity_id: &str) -> PathBuf {
        let uri = NapUri::new(&self.universe, entity_type, entity_id);
        self.root.join(uri.manifest_path())
    }

    /// Read a manifest from the repository.
    pub fn read_manifest(
        &self,
        entity_type: EntityType,
        entity_id: &str,
    ) -> Result<Manifest, NapError> {
        let path = self.manifest_path(entity_type, entity_id);
        debug!(
            path = %path.display(),
            entity_type = %entity_type,
            entity_id = %entity_id,
            "reading manifest"
        );
        Manifest::from_file(&path)
    }

    /// Read a manifest at a specific VCS ref (commit, branch, tag).
    pub fn read_manifest_at_ref(
        &self,
        entity_type: EntityType,
        entity_id: &str,
        reference: &str,
    ) -> Result<Manifest, NapError> {
        let uri = NapUri::new(&self.universe, entity_type, entity_id);
        let file_path = uri.manifest_path();

        debug!(
            file_path = %file_path,
            reference = %reference,
            "reading manifest at ref"
        );

        let content = self
            .vcs
            .read_file_at_ref(&self.root, &file_path, Some(reference))?;
        Manifest::from_yaml(&content)
    }

    /// Write a manifest to the repository (does NOT commit).
    pub fn write_manifest(&self, manifest: &Manifest) -> Result<PathBuf, NapError> {
        let uri: NapUri = manifest.id.parse()?;
        let path = self.root.join(uri.manifest_path());

        debug!(
            path = %path.display(),
            manifest_id = %manifest.id,
            "writing manifest"
        );

        manifest.to_file(&path)?;
        Ok(path)
    }

    /// Create a new entity manifest and commit it.
    pub fn create_entity(
        &self,
        entity_type: EntityType,
        entity_id: &str,
        name: &str,
        author: &str,
    ) -> Result<(Manifest, String), NapError> {
        let mut manifest = Manifest::new(&self.universe, entity_type, entity_id, name);

        // Validate against schema before writing
        crate::schema::validate_manifest(&manifest)
            .map_err(|errors| NapError::ManifestValidationError(errors.join("; ")))?;

        // Write the manifest
        self.write_manifest(&manifest)?;

        // Commit via VCS
        let commit_message = format!("Create {entity_type} '{name}' ({entity_id})");
        let commit_hash = self.vcs.commit(&self.root, &commit_message, author)?;

        // Update manifest with head pointer
        manifest.head = Some(commit_hash.clone());
        manifest.bump_version();
        self.write_manifest(&manifest)?;

        info!(
            manifest_id = %manifest.id,
            commit_hash = %commit_hash,
            "created entity"
        );

        Ok((manifest, commit_hash))
    }

    /// Update an existing manifest and commit the changes.
    pub fn commit_manifest(
        &self,
        manifest: &mut Manifest,
        message: &str,
        author: &str,
        changes: Vec<Change>,
    ) -> Result<Commit, NapError> {
        // Validate against schema before writing
        crate::schema::validate_manifest(manifest)
            .map_err(|errors| NapError::ManifestValidationError(errors.join("; ")))?;

        // Bump version
        manifest.bump_version();

        // Write updated manifest (without new head — we don't know it yet)
        self.write_manifest(manifest)?;

        // Compute manifest hash
        let manifest_hash = manifest.content_hash()?.as_str().to_string();

        // VCS commit — produces the new HEAD hash
        let vcs_hash = self.vcs.commit(&self.root, message, author)?;

        // Create NAP commit object with the now-known VCS hash
        let nap_commit = Commit::new(
            manifest.head.clone(),
            author,
            message,
            &manifest_hash,
            changes,
        );

        // Update head pointer and write again (leaves working tree dirty,
        // same pattern as create_entity)
        manifest.head = Some(vcs_hash.clone());
        self.write_manifest(manifest)?;

        debug!(
            manifest_id = %manifest.id,
            version = manifest.version,
            nap_commit_id = %nap_commit.id,
            vcs_hash = %vcs_hash,
            "manifest committed"
        );

        Ok(nap_commit)
    }

    /// Get the commit history for a specific entity.
    pub fn history(
        &self,
        entity_type: EntityType,
        entity_id: &str,
        limit: usize,
    ) -> Result<Vec<crate::vcs::CommitInfo>, NapError> {
        let uri = NapUri::new(&self.universe, entity_type, entity_id);
        let file_path = uri.manifest_path();
        self.vcs.log(&self.root, Some(&file_path), limit)
    }

    /// List all entity IDs of a given type in the repository.
    pub fn list_entities(&self, entity_type: EntityType) -> Result<Vec<String>, NapError> {
        let dir = self.root.join(entity_type.directory_name());
        if !dir.exists() {
            return Ok(vec![]);
        }

        let mut entities = Vec::new();
        for entry in std::fs::read_dir(&dir)? {
            let entry = entry?;
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) == Some("yaml")
                && let Some(stem) = path.file_stem().and_then(|s| s.to_str())
            {
                entities.push(stem.to_string());
            }
        }
        entities.sort();
        Ok(entities)
    }

    /// Delete an entity manifest and commit the deletion.
    pub fn delete_entity(
        &self,
        entity_type: EntityType,
        entity_id: &str,
        author: &str,
    ) -> Result<String, NapError> {
        let path = self.manifest_path(entity_type, entity_id);
        if !path.exists() {
            return Err(NapError::ManifestNotFound(path.display().to_string()));
        }

        std::fs::remove_file(&path)?;

        let message = format!("Delete {entity_type} '{entity_id}'");
        let hash = self.vcs.commit(&self.root, &message, author)?;
        info!(entity_type = %entity_type, entity_id = %entity_id, "deleted entity");
        Ok(hash)
    }

    /// Create a branch in the underlying VCS.
    pub fn create_branch(&self, name: &str) -> Result<(), NapError> {
        self.vcs.create_branch(&self.root, name)
    }

    /// Switch to a branch.
    pub fn switch_branch(&self, name: &str) -> Result<(), NapError> {
        self.vcs.switch_branch(&self.root, name)
    }

    /// Create a tag.
    pub fn create_tag(&self, name: &str) -> Result<(), NapError> {
        self.vcs.create_tag(&self.root, name)
    }

    /// List branches.
    pub fn list_branches(&self) -> Result<Vec<String>, NapError> {
        self.vcs.list_branches(&self.root)
    }

    /// List tags.
    pub fn list_tags(&self) -> Result<Vec<String>, NapError> {
        self.vcs.list_tags(&self.root)
    }

    /// Revert a commit by creating a new VCS commit that undoes the specified one.
    ///
    /// The revert is a universe-level operation (not entity-scoped) — a single
    /// Git commit can touch multiple files across multiple entity types.  After
    /// reverting, working-tree files are restored to their pre-commit content
    /// and a new revert commit is created in VCS history.
    pub fn revert_commit(&self, commit_hash: &str, author: &str) -> Result<String, NapError> {
        let new_hash = self.vcs.revert(&self.root, commit_hash)?;

        // Re-read all entity manifests and update their `head` pointer
        // so manifests are consistent with the new VCS state.
        for et in EntityType::subdirectory_types() {
            if let Ok(ids) = self.list_entities(*et) {
                for id in &ids {
                    if let Ok(mut manifest) = self.read_manifest(*et, id) {
                        manifest.head = Some(new_hash.clone());
                        self.write_manifest(&manifest).ok();
                    }
                }
            }
        }

        info!(
            commit = %commit_hash,
            revert = %new_hash,
            author = %author,
            "commit reverted"
        );

        Ok(new_hash)
    }

    /// Get current HEAD hash.
    pub fn head_hash(&self) -> Result<String, NapError> {
        self.vcs.head_hash(&self.root)
    }

    // ── Remote operations ─────────────────────────────────────────

    /// Add a remote to the repository.
    pub fn add_remote(&self, name: &str, url: &str) -> Result<(), NapError> {
        self.vcs.add_remote(&self.root, name, url)
    }

    /// Remove a remote from the repository.
    pub fn remove_remote(&self, name: &str) -> Result<(), NapError> {
        self.vcs.remove_remote(&self.root, name)
    }

    /// List remotes as `(name, url)` pairs.
    pub fn list_remotes(&self) -> Result<Vec<(String, String)>, NapError> {
        self.vcs.list_remotes(&self.root)
    }

    /// Push the current branch to a remote.
    pub fn push(&self, remote: Option<&str>, branch: Option<&str>) -> Result<(), NapError> {
        self.vcs.push(&self.root, remote, branch)
    }

    /// Pull the current branch from a remote.
    pub fn pull(&self, remote: Option<&str>, branch: Option<&str>) -> Result<(), NapError> {
        self.vcs.pull(&self.root, remote, branch)
    }

    /// Access the VCS backend (for the resolver to read files at specific refs).
    pub fn vcs(&self) -> &dyn VcsBackend {
        self.vcs.as_ref()
    }
}

// ── In-memory mock VcsBackend for testing ──────────────────────────────

#[cfg(test)]
mod mock_backend {
    use std::collections::HashMap;
    use std::path::Path;
    use std::sync::Mutex;
    use std::sync::atomic::{AtomicU64, Ordering};

    use crate::error::NapError;
    use crate::vcs::{CommitInfo, VcsBackend};

    /// A VcsBackend that simulates VCS operations in memory without shelling
    /// out.  Used by the Repository unit tests so they don't require a real
    /// `lore` binary.
    pub(crate) struct MockBackend {
        /// Incrementing counter for commit hashes.
        counter: AtomicU64,
        /// Stored commit metadata keyed by "hash".
        commits: Mutex<Vec<CommitInfo>>,
        /// Remote tracking.
        remotes: Mutex<HashMap<String, String>>,
        /// Branches.
        branches: Mutex<Vec<String>>,
        /// Current branch.
        current_branch: Mutex<String>,
        /// Tags.
        tags: Mutex<Vec<String>>,
    }

    impl MockBackend {
        pub fn new() -> Self {
            Self {
                counter: AtomicU64::new(1),
                commits: Mutex::new(Vec::new()),
                remotes: Mutex::new(HashMap::new()),
                branches: Mutex::new(vec!["main".to_string()]),
                current_branch: Mutex::new("main".to_string()),
                tags: Mutex::new(Vec::new()),
            }
        }
    }

    impl VcsBackend for MockBackend {
        fn init(&self, _path: &Path) -> Result<(), NapError> {
            Ok(())
        }

        fn commit(&self, _path: &Path, message: &str, author: &str) -> Result<String, NapError> {
            let n = self.counter.fetch_add(1, Ordering::SeqCst);
            // Use a valid 40-char hex hash so manifest schema validation passes.
            let hash = format!("{:040x}", n);
            let mut commits = self.commits.lock().unwrap();
            let parent = commits.last().map(|c| c.id.clone());
            let info = CommitInfo {
                id: hash.clone(),
                parent,
                author: author.to_string(),
                message: message.to_string(),
                timestamp: chrono::Utc::now().to_rfc3339(),
            };
            commits.push(info);
            Ok(hash)
        }

        fn read_file_at_ref(
            &self,
            repo_path: &Path,
            file_path: &str,
            _reference: Option<&str>,
        ) -> Result<String, NapError> {
            let full_path = repo_path.join(file_path);
            std::fs::read_to_string(&full_path)
                .map_err(|e| NapError::Other(format!("mock: read failed: {}", e)))
        }

        fn log(
            &self,
            _path: &Path,
            _file: Option<&str>,
            _limit: usize,
        ) -> Result<Vec<CommitInfo>, NapError> {
            let commits = self.commits.lock().unwrap();
            Ok(commits.clone())
        }

        fn create_branch(&self, _path: &Path, name: &str) -> Result<(), NapError> {
            self.branches.lock().unwrap().push(name.to_string());
            Ok(())
        }

        fn switch_branch(&self, _path: &Path, name: &str) -> Result<(), NapError> {
            *self.current_branch.lock().unwrap() = name.to_string();
            Ok(())
        }

        fn create_tag(&self, _path: &Path, name: &str) -> Result<(), NapError> {
            let mut tags = self.tags.lock().unwrap();
            if !tags.contains(&name.to_string()) {
                tags.push(name.to_string());
            }
            Ok(())
        }

        fn current_branch(&self, _path: &Path) -> Result<String, NapError> {
            Ok(self.current_branch.lock().unwrap().clone())
        }

        fn head_hash(&self, _path: &Path) -> Result<String, NapError> {
            let commits = self.commits.lock().unwrap();
            commits
                .last()
                .map(|c| c.id.clone())
                .ok_or_else(|| NapError::VcsError("no commits".to_string()))
        }

        fn revert(&self, _repo_path: &Path, commit_hash: &str) -> Result<String, NapError> {
            let n = self.counter.fetch_add(1, Ordering::SeqCst);
            let hash = format!("{:040x}", n);
            let info = CommitInfo {
                id: hash.clone(),
                parent: Some(commit_hash.to_string()),
                author: "revert".to_string(),
                message: format!("revert {}", commit_hash),
                timestamp: chrono::Utc::now().to_rfc3339(),
            };
            self.commits.lock().unwrap().push(info);
            Ok(hash)
        }

        fn list_branches(&self, _path: &Path) -> Result<Vec<String>, NapError> {
            Ok(self.branches.lock().unwrap().clone())
        }

        fn list_tags(&self, _path: &Path) -> Result<Vec<String>, NapError> {
            Ok(self.tags.lock().unwrap().clone())
        }

        fn add_remote(&self, _path: &Path, name: &str, url: &str) -> Result<(), NapError> {
            self.remotes
                .lock()
                .unwrap()
                .insert(name.to_string(), url.to_string());
            Ok(())
        }

        fn remove_remote(&self, _path: &Path, name: &str) -> Result<(), NapError> {
            self.remotes.lock().unwrap().remove(name);
            Ok(())
        }

        fn list_remotes(&self, _path: &Path) -> Result<Vec<(String, String)>, NapError> {
            let remotes = self.remotes.lock().unwrap();
            Ok(remotes
                .iter()
                .map(|(k, v)| (k.clone(), v.clone()))
                .collect())
        }

        fn push(
            &self,
            _path: &Path,
            _remote: Option<&str>,
            _branch: Option<&str>,
        ) -> Result<(), NapError> {
            Ok(())
        }

        fn pull(
            &self,
            _path: &Path,
            _remote: Option<&str>,
            _branch: Option<&str>,
        ) -> Result<(), NapError> {
            Ok(())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use mock_backend::MockBackend;
    use tempfile::TempDir;

    fn make_repo(tmp: &TempDir) -> Repository {
        Repository::init(tmp.path(), "testverse", Box::new(MockBackend::new())).unwrap()
    }

    #[test]
    fn test_init_creates_structure() {
        let tmp = TempDir::new().unwrap();
        let repo = make_repo(&tmp);

        assert!(repo.root.join(".nap").exists());
        assert!(repo.root.join("universe.yaml").exists());
        assert!(repo.root.join("characters").exists());
        assert!(repo.root.join("locations").exists());
        assert!(repo.root.join("scenes").exists());
        assert!(repo.root.join("props").exists());
    }

    #[test]
    fn test_create_and_read_entity() {
        let tmp = TempDir::new().unwrap();
        let repo = make_repo(&tmp);

        let (manifest, _hash) = repo
            .create_entity(EntityType::Character, "hero", "The Hero", "test-author")
            .unwrap();

        assert_eq!(manifest.name, "The Hero");
        assert_eq!(manifest.entity_type, EntityType::Character);

        // Read it back
        let read_back = repo.read_manifest(EntityType::Character, "hero").unwrap();
        assert_eq!(read_back.name, "The Hero");
    }

    #[test]
    fn test_list_entities() {
        let tmp = TempDir::new().unwrap();
        let repo = make_repo(&tmp);

        repo.create_entity(EntityType::Character, "alice", "Alice", "author")
            .unwrap();
        repo.create_entity(EntityType::Character, "bob", "Bob", "author")
            .unwrap();

        let chars = repo.list_entities(EntityType::Character).unwrap();
        assert_eq!(chars, vec!["alice", "bob"]);
    }

    #[test]
    fn test_commit_manifest_updates() {
        let tmp = TempDir::new().unwrap();
        let repo = make_repo(&tmp);

        let (mut manifest, _) = repo
            .create_entity(EntityType::Character, "hero", "The Hero", "author")
            .unwrap();

        // Modify and commit
        manifest.set_property("species", serde_yaml::Value::String("elf".to_string()));
        let changes = vec![Change::set("properties.species", None, "elf".to_string())];
        let commit = repo
            .commit_manifest(&mut manifest, "set species to elf", "author", changes)
            .unwrap();

        assert!(!commit.id.is_empty());
        assert_eq!(commit.message, "set species to elf");

        // Verify version incremented
        let read_back = repo.read_manifest(EntityType::Character, "hero").unwrap();
        assert!(read_back.version >= 2);
    }

    #[test]
    fn test_history() {
        let tmp = TempDir::new().unwrap();
        let repo = make_repo(&tmp);

        let (mut manifest, _) = repo
            .create_entity(EntityType::Character, "hero", "The Hero", "author")
            .unwrap();

        manifest.set_property(
            "name",
            serde_yaml::Value::String("Updated Hero".to_string()),
        );
        repo.commit_manifest(&mut manifest, "update name", "author", vec![])
            .unwrap();

        let hist = repo.history(EntityType::Character, "hero", 10).unwrap();
        assert!(hist.len() >= 2);
    }

    #[test]
    fn test_revert_commit() {
        let tmp = TempDir::new().unwrap();
        let repo = make_repo(&tmp);

        // Create entity and note its name
        let (mut manifest, _) = repo
            .create_entity(EntityType::Character, "hero", "The Hero", "author")
            .unwrap();
        assert_eq!(manifest.name, "The Hero");

        // Modify and commit
        manifest.set_property("species", serde_yaml::Value::String("elf".to_string()));
        let changes = vec![Change::set("properties.species", None, "elf".to_string())];
        let _commit = repo
            .commit_manifest(&mut manifest, "set species to elf", "author", changes)
            .unwrap();

        // Verify the property was set
        let read_back = repo.read_manifest(EntityType::Character, "hero").unwrap();
        assert_eq!(
            read_back.properties.get("species").and_then(|v| v.as_str()),
            Some("elf")
        );

        // Get the VCS commit hash from the manifest's head pointer.
        // After commit_manifest, this is the single VCS commit containing
        // the property change (the head pointer update is left dirty).
        let vcs_hash = read_back
            .head
            .as_ref()
            .expect("head should be set after commit");

        // Revert that VCS commit
        let revert_hash = repo.revert_commit(vcs_hash, "author").unwrap();
        assert!(!revert_hash.is_empty());

        // Verify the manifest head was updated to the revert commit
        let after_revert = repo.read_manifest(EntityType::Character, "hero").unwrap();
        assert_eq!(after_revert.head.as_deref(), Some(revert_hash.as_str()));

        // Verify the revert appears in history
        let hist = repo.history(EntityType::Character, "hero", 10).unwrap();
        assert!(hist.iter().any(|c| c.id == revert_hash));
    }

    #[test]
    fn test_remote_operations() {
        let tmp = TempDir::new().unwrap();
        let repo = make_repo(&tmp);

        repo.add_remote("origin", "git@github.com:user/repo.git")
            .unwrap();
        let remotes = repo.list_remotes().unwrap();
        assert_eq!(remotes.len(), 1);
        assert_eq!(remotes[0].0, "origin");

        repo.remove_remote("origin").unwrap();
        let remotes = repo.list_remotes().unwrap();
        assert!(remotes.is_empty());
    }
}
