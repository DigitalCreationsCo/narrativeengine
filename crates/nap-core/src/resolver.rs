//! NAP Resolver — URI → Manifest, with query and version selectors.
//!
//! The resolver is the primary interface for reading NAP resources.
//! It handles:
//! - Full manifest resolution: `nap://starwars/character/lukeskywalker`
//! - Fragment queries: `nap://starwars/character/lukeskywalker#references.appears_in`
//! - Version selectors: branch, commit, tag
//! - Subtree extraction for efficient AI/application access
//!
//! Version and branch are NEVER in the URI. They are orthogonal selectors:
//! ```text
//! URI + Reference + Revision Selector
//! ```

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use tracing::{debug, info};

use crate::error::NapError;
use crate::manifest::Manifest;
use crate::query::ManifestQuery;
use crate::repository::Repository;
use crate::uri::NapUri;
use crate::vcs::VcsBackend;
use crate::vcs_lore::LoreBackend;

/// Options for resolving a NAP URI. All are optional — omitting all
/// resolves the current HEAD of the default branch.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ResolveOptions {
    /// Resolve at a specific branch. e.g., `"canon"`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub branch: Option<String>,

    /// Resolve at a specific commit hash. e.g., `"a72c9f3b"`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub commit: Option<String>,

    /// Resolve at a specific tag. e.g., `"episode-6"`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tag: Option<String>,

    /// Subtree query path (overrides URI fragment). e.g., `"appearances.audienceVotes"`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
}

impl ResolveOptions {
    /// Returns the Git ref to use, or None for HEAD / working tree.
    fn git_ref(&self) -> Option<String> {
        if let Some(ref commit) = self.commit {
            Some(commit.clone())
        } else if let Some(ref tag) = self.tag {
            Some(format!("refs/tags/{tag}"))
        } else {
            self.branch.clone()
        }
    }

    /// Returns the query path (from options or URI fragment).
    fn query_path(&self, uri: &NapUri) -> Option<String> {
        self.path.clone().or_else(|| uri.fragment.clone())
    }
}

/// The result of resolving a NAP URI.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ResolveResult {
    /// Full manifest (no query applied).
    Full(Box<Manifest>),
    /// Subtree result from a query.
    Subtree(serde_json::Value),
}

/// The NAP resolver — resolves URIs to manifests or subtrees.
pub struct Resolver {
    /// Base directory containing universe repositories.
    base_path: PathBuf,
    /// VCS backend factory (creates backend per-repo).
    vcs_factory: fn() -> Box<dyn VcsBackend>,
}

impl Resolver {
    /// Create a resolver that looks for universe repos under `base_path`.
    ///
    /// # Example layout
    /// ```text
    /// base_path/
    /// ├── starwars/    ← universe repo
    /// ├── toystory/    ← universe repo
    /// └── marvel/      ← universe repo
    /// ```
    pub fn new(base_path: &Path) -> Self {
        Self {
            base_path: base_path.to_path_buf(),
            vcs_factory: || Box::new(LoreBackend::from_env()),
        }
    }

    /// Create a resolver with a custom VCS backend factory.
    pub fn with_vcs_factory(base_path: &Path, factory: fn() -> Box<dyn VcsBackend>) -> Self {
        Self {
            base_path: base_path.to_path_buf(),
            vcs_factory: factory,
        }
    }

    /// Open the repository for a given universe.
    fn open_repo(&self, universe: &str) -> Result<Repository, NapError> {
        let repo_path = self.base_path.join(universe);
        Repository::open(&repo_path, (self.vcs_factory)())
    }

    /// Resolve a NAP URI string with options.
    ///
    /// # Examples
    /// ```text
    /// // Full manifest
    /// resolver.resolve("nap://starwars/character/lukeskywalker", &Default::default())
    ///
    /// // With branch
    /// resolver.resolve("nap://starwars/character/lukeskywalker", &ResolveOptions {
    ///     branch: Some("canon".to_string()),
    ///     ..Default::default()
    /// })
    ///
    /// // With fragment query (via URI)
    /// resolver.resolve("nap://starwars/character/lukeskywalker#references.appears_in", &Default::default())
    /// ```
    pub fn resolve(
        &self,
        uri_str: &str,
        options: &ResolveOptions,
    ) -> Result<ResolveResult, NapError> {
        let uri: NapUri = uri_str.parse()?;
        self.resolve_uri(&uri, options)
    }

    /// Resolve a parsed NAP URI with options.
    pub fn resolve_uri(
        &self,
        uri: &NapUri,
        options: &ResolveOptions,
    ) -> Result<ResolveResult, NapError> {
        debug!(
            uri = %uri,
            options = ?options,
            "resolving NAP URI"
        );

        let repo = self.open_repo(&uri.universe)?;
        let git_ref = options.git_ref();
        let query_path = options.query_path(uri);

        // Read the manifest (at ref or from working tree)
        let manifest = match git_ref {
            Some(ref reference) => {
                debug!(reference = %reference, "resolving at specific ref");
                repo.read_manifest_at_ref(uri.entity_type, &uri.entity_id, reference)?
            }
            None => repo.read_manifest(uri.entity_type, &uri.entity_id)?,
        };

        // Apply query if present
        match query_path {
            Some(ref path) => {
                debug!(query_path = %path, "applying subtree query");
                let yaml_value = manifest.to_value()?;
                let result = ManifestQuery::query(&yaml_value, path, &manifest.id)?;

                // Convert YAML value to JSON for consistent API output
                let json_str = serde_yaml::to_string(&result)
                    .map_err(|e| NapError::ManifestValidationError(e.to_string()))?;
                let json_value: serde_json::Value = serde_yaml::from_str(&json_str)
                    .map_err(|e| NapError::ManifestValidationError(e.to_string()))?;

                info!(
                    uri = %uri,
                    query_path = %path,
                    "resolved NAP URI with query"
                );
                Ok(ResolveResult::Subtree(json_value))
            }
            None => {
                info!(uri = %uri, "resolved NAP URI (full manifest)");
                Ok(ResolveResult::Full(Box::new(manifest)))
            }
        }
    }

    /// Convenience: query a specific path on a URI.
    pub fn query(&self, uri_str: &str, path: &str) -> Result<serde_json::Value, NapError> {
        let options = ResolveOptions {
            path: Some(path.to_string()),
            ..Default::default()
        };
        match self.resolve(uri_str, &options)? {
            ResolveResult::Subtree(v) => Ok(v),
            ResolveResult::Full(m) => m.to_json_value(),
        }
    }

    /// List all universe repositories available.
    pub fn list_universes(&self) -> Result<Vec<String>, NapError> {
        let mut universes = Vec::new();
        for entry in std::fs::read_dir(&self.base_path)? {
            let entry = entry?;
            let path = entry.path();
            if path.is_dir()
                && path.join(".nap").exists()
                && let Some(name) = path.file_name().and_then(|n| n.to_str())
            {
                universes.push(name.to_string());
            }
        }
        universes.sort();
        Ok(universes)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::EntityType;
    use crate::vcs_git::GitBackend;
    use tempfile::TempDir;

    fn setup() -> (TempDir, Resolver) {
        let tmp = TempDir::new().unwrap();
        let repo = Repository::init(tmp.path(), "starwars", Box::new(GitBackend::new())).unwrap();

        // Create a character
        let (mut manifest, _) = repo
            .create_entity(
                EntityType::Character,
                "lukeskywalker",
                "Luke Skywalker",
                "test",
            )
            .unwrap();

        // Add properties and commit
        manifest.set_property("species", serde_yaml::Value::String("human".to_string()));
        manifest.set_property(
            "homeworld",
            serde_yaml::Value::String("nap://starwars/location/tatooine".to_string()),
        );
        manifest.add_reference(
            "appears_in",
            serde_yaml::Value::Sequence(vec![serde_yaml::Value::String(
                "nap://starwars/scene/cantina".to_string(),
            )]),
        );

        use crate::commit::Change;
        repo.commit_manifest(
            &mut manifest,
            "add Luke Skywalker details",
            "test",
            vec![Change::set("properties.species", None, "human".to_string())],
        )
        .unwrap();

        let resolver = Resolver::new(tmp.path());
        (tmp, resolver)
    }

    #[test]
    fn test_resolve_full_manifest() {
        let (_tmp, resolver) = setup();
        let result = resolver
            .resolve(
                "nap://starwars/character/lukeskywalker",
                &Default::default(),
            )
            .unwrap();
        match result {
            ResolveResult::Full(m) => {
                assert_eq!(m.name, "Luke Skywalker");
                assert_eq!(m.entity_type, EntityType::Character);
            }
            _ => panic!("expected full manifest"),
        }
    }

    #[test]
    fn test_resolve_with_fragment() {
        let (_tmp, resolver) = setup();
        let result = resolver
            .resolve(
                "nap://starwars/character/lukeskywalker#properties.species",
                &Default::default(),
            )
            .unwrap();
        match result {
            ResolveResult::Subtree(v) => {
                assert_eq!(v.as_str(), Some("human"));
            }
            _ => panic!("expected subtree"),
        }
    }

    #[test]
    fn test_resolve_with_options_path() {
        let (_tmp, resolver) = setup();
        let result = resolver
            .resolve(
                "nap://starwars/character/lukeskywalker",
                &ResolveOptions {
                    path: Some("properties.homeworld".to_string()),
                    ..Default::default()
                },
            )
            .unwrap();
        match result {
            ResolveResult::Subtree(v) => {
                assert_eq!(v.as_str(), Some("nap://starwars/location/tatooine"));
            }
            _ => panic!("expected subtree"),
        }
    }

    #[test]
    fn test_query_convenience() {
        let (_tmp, resolver) = setup();
        let result = resolver
            .query(
                "nap://starwars/character/lukeskywalker",
                "properties.species",
            )
            .unwrap();
        assert_eq!(result.as_str(), Some("human"));
    }

    #[test]
    fn test_list_universes() {
        let (_tmp, resolver) = setup();
        let universes = resolver.list_universes().unwrap();
        assert!(universes.contains(&"starwars".to_string()));
    }

    #[test]
    fn test_resolve_not_found() {
        let (_tmp, resolver) = setup();
        let result = resolver.resolve("nap://starwars/character/nonexistent", &Default::default());
        assert!(result.is_err());
    }
}
