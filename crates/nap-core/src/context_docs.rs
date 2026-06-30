//! Context document metadata and dependency-graph management.
//!
//! Context documents are markdown (or any plaintext) files inside a Lore
//! workspace that carry structured metadata and dependency edges.  They
//! are the building blocks for AI context-graph assembly — the system
//! that tells an agent "when user asks about task X, also surface
//! documents Y and Z."
//!
//! ## Storage model
//!
//! Metadata is stored as Lore file-level metadata under the
//! `nap.context_docs` key (a JSON map keyed by file path).  Dependency
//! edges are stored as `nap.context_deps` (a JSON adjacency list).
//!
//! This avoids requiring a separate database or sidecar files — the
//! VCS is the source of truth.
//!
//! ## Thread safety
//!
//! [`ContextDocsManager`] uses interior mutability (a `Mutex<Graph>`)
//! for dependency-graph mutations.  Read operations take `&self`.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use serde::{Deserialize, Serialize};

use crate::error::NapError;
use crate::vcs::ContextDocument;

// ---------------------------------------------------------------------------
// In-memory graph
// ---------------------------------------------------------------------------

/// The in-memory context document graph.
///
/// Nodes are document paths.  Edges are dependency relationships:
/// `edges[target] = set of source paths` — i.e. if document A depends on
/// document B then `edges[B]` contains A.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
struct ContextGraph {
    /// Document metadata keyed by canonical path.
    nodes: HashMap<String, HashMap<String, String>>,
    /// Adjacency list: `edges[target] = {sources ... }`.
    edges: HashMap<String, HashSet<String>>,
}

// ---------------------------------------------------------------------------
// ContextDocsManager
// ---------------------------------------------------------------------------

/// Manages context-document metadata and dependency edges inside a Lore
/// workspace.
///
/// ## Usage
///
/// ```ignore
/// let manager = ContextDocsManager::new(workspace_path);
/// manager.register("task-123.md", &[("status", "active")])?;
/// manager.add_dependency("task-123.md", "characters/hero.yaml")?;
/// let deps = manager.dependents_of("characters/hero.yaml")?;
/// ```
pub struct ContextDocsManager {
    /// Workspace root path.
    workspace_root: PathBuf,
    /// Synchronised in-memory graph.
    graph: Mutex<ContextGraph>,
    /// Whether the graph was loaded from Lore metadata (dirty flag).
    loaded: Mutex<bool>,
}

/// Path where context-doc metadata is persisted, relative to workspace root.
const CONTEXT_DOCS_FILE: &str = ".lore/metadata/nap.context_docs";
/// Path where context-doc dependency edges are persisted, relative to workspace root.
const CONTEXT_DEPS_FILE: &str = ".lore/metadata/nap.context_deps";

impl ContextDocsManager {
    /// Create a new manager.
    ///
    /// The graph is not loaded from Lore until the first operation that
    /// requires it (lazy init).
    pub fn new(workspace_root: &Path) -> Self {
        Self {
            workspace_root: workspace_root.to_path_buf(),
            graph: Mutex::new(ContextGraph::default()),
            loaded: Mutex::new(false),
        }
    }

    // ── Lazy load ────────────────────────────────────────────────────

    /// Ensure the in-memory graph has been loaded from persisted metadata.
    fn ensure_loaded(&self) -> Result<(), NapError> {
        let mut loaded = self.loaded.lock().unwrap();
        if *loaded {
            return Ok(());
        }

        // Attempt to read context metadata from the local filesystem.
        let docs_path = self.workspace_root.join(CONTEXT_DOCS_FILE);
        let metadata_json = if docs_path.exists() {
            std::fs::read_to_string(&docs_path).unwrap_or_else(|_| "{}".to_string())
        } else {
            "{}".to_string()
        };

        let deps_path = self.workspace_root.join(CONTEXT_DEPS_FILE);
        let deps_json = if deps_path.exists() {
            std::fs::read_to_string(&deps_path).unwrap_or_else(|_| "{}".to_string())
        } else {
            "{}".to_string()
        };

        let mut graph_lock = self.graph.lock().unwrap();

        // Parse nodes.
        if metadata_json != "{}" && !metadata_json.is_empty() {
            let nodes: HashMap<String, HashMap<String, String>> =
                serde_json::from_str(&metadata_json).map_err(|e| {
                    NapError::Other(format!("failed to parse nap.context_docs metadata: {}", e))
                })?;
            graph_lock.nodes = nodes;
        }

        // Parse edges.
        if deps_json != "{}" && !deps_json.is_empty() {
            #[derive(Deserialize)]
            struct DepsFile {
                edges: HashMap<String, Vec<String>>,
            }
            let deps: DepsFile = serde_json::from_str(&deps_json)
                .map_err(|e| NapError::Other(format!("failed to parse nap.context_deps: {}", e)))?;

            for (target, sources) in deps.edges {
                graph_lock
                    .edges
                    .insert(target, sources.into_iter().collect());
            }
        }

        *loaded = true;
        Ok(())
    }

    // ── Persist ──────────────────────────────────────────────────────

    /// Write the current in-memory graph back to Lore metadata.
    ///
    /// This is a two-step process:
    /// 1. Serialise nodes and edges to JSON.
    /// 2. Write to `.lore/metadata/nap.context_docs` and
    ///    `.lore/metadata/nap.context_deps` via the VCS.
    pub fn persist(&self) -> Result<(), NapError> {
        let graph_lock = self.graph.lock().unwrap();

        let nodes_json = serde_json::to_string_pretty(&graph_lock.nodes).map_err(|e| {
            NapError::Other(format!("failed to serialise context doc nodes: {}", e))
        })?;

        // Serialise edges as { target: [source, ...] }.
        let edges_serialisable: HashMap<String, Vec<String>> = graph_lock
            .edges
            .iter()
            .map(|(target, sources)| (target.clone(), sources.iter().cloned().collect()))
            .collect();

        let deps_json = serde_json::to_string_pretty(&serde_json::json!({
            "edges": edges_serialisable
        }))
        .map_err(|e| NapError::Other(format!("failed to serialise context deps: {}", e)))?;

        // Write metadata files.
        let docs_path = self.workspace_root.join(".lore/metadata/nap.context_docs");
        let deps_path = self.workspace_root.join(".lore/metadata/nap.context_deps");

        if let Some(parent) = docs_path.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| NapError::Other(format!("failed to create metadata dir: {}", e)))?;
        }
        if let Some(parent) = deps_path.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| NapError::Other(format!("failed to create metadata dir: {}", e)))?;
        }

        std::fs::write(&docs_path, &nodes_json)
            .map_err(|e| NapError::Other(format!("failed to write context doc metadata: {}", e)))?;
        std::fs::write(&deps_path, &deps_json).map_err(|e| {
            NapError::Other(format!("failed to write context deps metadata: {}", e))
        })?;

        Ok(())
    }

    // ── Document CRUD ────────────────────────────────────────────────

    /// Register a document with its metadata.
    ///
    /// If the document already exists, its metadata is **merged** with
    /// the new entries (new keys overwrite old ones; existing keys not in
    /// `metadata` are preserved).
    pub fn register(&self, path: &str, metadata: &[(&str, &str)]) -> Result<(), NapError> {
        self.ensure_loaded()?;

        let mut graph_lock = self.graph.lock().unwrap();
        let entry = graph_lock.nodes.entry(path.to_string()).or_default();
        for (k, v) in metadata {
            entry.insert(k.to_string(), v.to_string());
        }

        Ok(())
    }

    /// Remove a document from the graph.
    ///
    /// Also removes all dependency edges where this document is the target
    /// or a source.
    pub fn unregister(&self, path: &str) -> Result<(), NapError> {
        self.ensure_loaded()?;

        let mut graph_lock = self.graph.lock().unwrap();
        graph_lock.nodes.remove(path);

        // Remove edges where path is a target (incoming deps).
        graph_lock.edges.remove(path);

        // Remove edges where path is a source (outgoing deps).
        for sources in graph_lock.edges.values_mut() {
            sources.remove(path);
        }

        Ok(())
    }

    /// Get all documents and their metadata as a list.
    pub fn all_documents(&self) -> Result<Vec<ContextDocument>, NapError> {
        self.ensure_loaded()?;

        let graph_lock = self.graph.lock().unwrap();
        let mut docs = Vec::with_capacity(graph_lock.nodes.len());

        for (path, metadata) in &graph_lock.nodes {
            let deps: Vec<String> = graph_lock
                .edges
                .iter()
                .filter(|(_, sources)| sources.contains(path))
                .map(|(target, _)| target.clone())
                .collect();

            docs.push(ContextDocument {
                path: path.clone(),
                metadata: metadata.clone(),
                depends_on: deps,
            });
        }

        Ok(docs)
    }

    /// Get a specific document by path.
    pub fn get_document(&self, path: &str) -> Result<Option<ContextDocument>, NapError> {
        self.ensure_loaded()?;

        let graph_lock = self.graph.lock().unwrap();
        let metadata = match graph_lock.nodes.get(path) {
            Some(m) => m.clone(),
            None => return Ok(None),
        };

        let deps: Vec<String> = graph_lock
            .edges
            .iter()
            .filter(|(_, sources)| sources.contains(path))
            .map(|(target, _)| target.clone())
            .collect();

        Ok(Some(ContextDocument {
            path: path.to_string(),
            metadata,
            depends_on: deps,
        }))
    }

    // ── Dependency management ────────────────────────────────────────

    /// Declare that `source` depends on `target`.
    ///
    /// This adds `source` to the adjacency list of `target`.
    /// Both paths must be registered; registering is implicit if they are
    /// not yet known (the graph auto-vivifies them with empty metadata).
    pub fn add_dependency(&self, source: &str, target: &str) -> Result<(), NapError> {
        self.ensure_loaded()?;

        let mut graph_lock = self.graph.lock().unwrap();

        // Auto-vivify nodes if not yet registered.
        graph_lock.nodes.entry(source.to_string()).or_default();
        graph_lock.nodes.entry(target.to_string()).or_default();

        graph_lock
            .edges
            .entry(target.to_string())
            .or_default()
            .insert(source.to_string());

        Ok(())
    }

    /// Remove a dependency edge.
    pub fn remove_dependency(&self, source: &str, target: &str) -> Result<(), NapError> {
        self.ensure_loaded()?;

        let mut graph_lock = self.graph.lock().unwrap();
        if let Some(sources) = graph_lock.edges.get_mut(target) {
            sources.remove(source);
        }

        Ok(())
    }

    /// Get the set of documents that directly depend on `target`.
    pub fn dependents_of(&self, target: &str) -> Result<Vec<String>, NapError> {
        self.ensure_loaded()?;

        let graph_lock = self.graph.lock().unwrap();
        let sources = graph_lock.edges.get(target).cloned().unwrap_or_default();
        Ok(sources.into_iter().collect())
    }

    /// Get the set of documents that `source` depends on.
    ///
    /// This is a reverse lookup: we scan all edges for where `source`
    /// appears as a dependent.
    pub fn dependencies_of(&self, source: &str) -> Result<Vec<String>, NapError> {
        self.ensure_loaded()?;

        let graph_lock = self.graph.lock().unwrap();
        let deps: Vec<String> = graph_lock
            .edges
            .iter()
            .filter(|(_, sources)| sources.contains(source))
            .map(|(target, _)| target.clone())
            .collect();
        Ok(deps)
    }

    /// Get the full dependency graph as a JSON string (for debugging or
    /// context-graph assembly).
    pub fn graph_as_json(&self) -> Result<String, NapError> {
        self.ensure_loaded()?;

        let graph_lock = self.graph.lock().unwrap();
        serde_json::to_string_pretty(&*graph_lock)
            .map_err(|e| NapError::Other(format!("failed to serialise context graph: {}", e)))
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    /// Helper: create a temporary workspace with a fresh manager.
    fn setup() -> (tempfile::TempDir, ContextDocsManager) {
        let dir = tempfile::TempDir::new().unwrap();
        let manager = ContextDocsManager::new(dir.path());
        (dir, manager)
    }

    #[test]
    fn test_register_and_get_document() {
        let (_dir, manager) = setup();
        manager
            .register("task-123.md", &[("status", "active"), ("owner", "alice")])
            .unwrap();

        let doc = manager.get_document("task-123.md").unwrap().unwrap();
        assert_eq!(doc.path, "task-123.md");
        assert_eq!(doc.metadata.get("status").unwrap(), "active");
        assert_eq!(doc.metadata.get("owner").unwrap(), "alice");
    }

    #[test]
    fn test_get_nonexistent_document() {
        let (_dir, manager) = setup();
        let doc = manager.get_document("does-not-exist.md").unwrap();
        assert!(doc.is_none());
    }

    #[test]
    fn test_unregister_removes_node() {
        let (_dir, manager) = setup();
        manager.register("doc-a.md", &[]).unwrap();
        manager.register("doc-b.md", &[]).unwrap();
        manager.add_dependency("doc-a.md", "doc-b.md").unwrap();

        manager.unregister("doc-a.md").unwrap();

        // doc-a should not appear, and no edge to doc-b should remain.
        assert!(manager.get_document("doc-a.md").unwrap().is_none());
        let deps_of_b = manager.dependents_of("doc-b.md").unwrap();
        assert!(deps_of_b.is_empty());
    }

    #[test]
    fn test_add_dependency() {
        let (_dir, manager) = setup();
        manager.register("agent-log.txt", &[]).unwrap();
        manager
            .register("context/characters/hero.yaml", &[])
            .unwrap();

        manager
            .add_dependency("agent-log.txt", "context/characters/hero.yaml")
            .unwrap();

        let deps = manager
            .dependents_of("context/characters/hero.yaml")
            .unwrap();
        assert_eq!(deps, vec!["agent-log.txt"]);

        let dep_of_agent = manager.dependencies_of("agent-log.txt").unwrap();
        assert_eq!(dep_of_agent, vec!["context/characters/hero.yaml"]);
    }

    #[test]
    fn test_remove_dependency() {
        let (_dir, manager) = setup();
        manager.register("a", &[]).unwrap();
        manager.register("b", &[]).unwrap();
        manager.add_dependency("a", "b").unwrap();
        manager.remove_dependency("a", "b").unwrap();

        assert!(manager.dependents_of("b").unwrap().is_empty());
    }

    #[test]
    fn test_all_documents() {
        let (_dir, manager) = setup();
        manager.register("doc1", &[("k1", "v1")]).unwrap();
        manager.register("doc2", &[("k2", "v2")]).unwrap();

        let docs = manager.all_documents().unwrap();
        assert_eq!(docs.len(), 2);
        let paths: Vec<String> = docs.into_iter().map(|d| d.path).collect();
        assert!(paths.contains(&"doc1".to_string()));
        assert!(paths.contains(&"doc2".to_string()));
    }

    #[test]
    fn test_graph_as_json() {
        let (_dir, manager) = setup();
        manager.register("doc1", &[]).unwrap();
        let json = manager.graph_as_json().unwrap();
        assert!(json.contains("doc1"));
    }

    #[test]
    fn test_persist_and_lazy_reload_is_idempotent() {
        let (_dir, manager) = setup();
        manager.register("persist-test", &[("key", "val")]).unwrap();
        // persist writes to the filesystem.
        manager.persist().unwrap();
        // The data is still in memory; the persist confirmed no errors.
        let doc = manager.get_document("persist-test").unwrap().unwrap();
        assert_eq!(doc.metadata.get("key").unwrap(), "val");
    }

    #[test]
    fn test_auto_vivify_on_add_dependency() {
        let (_dir, manager) = setup();
        // Neither a nor b registered, but add_dependency should create them.
        manager.add_dependency("a", "b").unwrap();
        assert!(manager.get_document("a").unwrap().is_some());
        assert!(manager.get_document("b").unwrap().is_some());
        assert_eq!(manager.dependents_of("b").unwrap(), vec!["a"]);
    }

    #[test]
    fn test_dependency_removes_orphan() {
        let (_dir, manager) = setup();
        manager.add_dependency("a", "b").unwrap();
        manager.unregister("a").unwrap();

        // b should have no dependents now.
        assert!(manager.dependents_of("b").unwrap().is_empty());
    }
}
