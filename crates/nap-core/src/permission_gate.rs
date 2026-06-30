//! Application-layer ACL for directory and file paths in a Lore workspace.
//!
//! ## Why app-layer and not VCS-layer
//!
//! Lore's stock server has no native path-level access control.  Instead,
//! NAP enforces ACLs at the application layer by intercepting file read/
//! write calls after they pass through the Lore VCS.  This keeps the
//! loreserver simple and allows NAP to implement rich rules (prefix
//! patterns, deny-overrides, role expansion) without server changes.
//!
//! ## Design
//!
//! [`PermissionGate`] is constructed with a set of [`Permission`] rules.
//! Every request to read or write a path is checked against the ruleset:
//!
//! 1. If an explicit `Deny` matches the path, the request is **rejected**.
//! 2. If a `Write` (or `Read`) matches, the request is **allowed**.
//! 3. No match → **denied by default** (fail-closed).
//!
//! Rules are stored in `context/nap-gate.toml` inside the workspace and
//! loaded on [`PermissionGate::load`].
//!
//! ## Future
//!
//! In a future iteration this may read from `lore file metadata` or from
//! a dedicated ACL API, but for v0 the file-based approach gives us a
//! portable, inspectable ACL that works without server changes.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::error::NapError;
use crate::vcs::{AccessLevel, Permission};

// ---------------------------------------------------------------------------
// Gate configuration (serialised to context/nap-gate.toml)
// ---------------------------------------------------------------------------

/// On-disk format for the permission gate config.
#[derive(Debug, Clone, Serialize, Deserialize)]
struct GateConfig {
    /// Default access level when no rules match.
    #[serde(default = "default_deny")]
    default: String,
    /// Ordered list of access rules.
    #[serde(default)]
    rules: Vec<GateRule>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct GateRule {
    /// Glob or prefix pattern (e.g. `agents/**` or `context/`).
    path: String,
    /// Principal (user or role) this rule applies to.
    principal: String,
    /// Access level: "read", "write", or "deny".
    access: String,
}

fn default_deny() -> String {
    "deny".to_string()
}

// ---------------------------------------------------------------------------
// PermissionGate
// ---------------------------------------------------------------------------

/// Application-layer access-control gate for Lore workspace paths.
///
/// ## Usage
///
/// ```ignore
/// let gate = PermissionGate::load(workspace_path).await?;
/// gate.check_read("/context/secret.md", "alice")?;     // Err if denied
/// gate.check_write("/entities/characters/bob.yaml", "alice")?; // Err if denied
/// ```
///
/// ## Thread safety
///
/// `PermissionGate` is immutable after construction and safe to share
/// across threads (immutable `&self` check methods).
#[derive(Debug)]
pub struct PermissionGate {
    /// Workspace root path (resolves relative rules).
    workspace_root: PathBuf,
    /// Parsed rules keyed by (canonicalised prefix, principal).
    /// The vec is searched in order; first match wins.
    rules: Vec<(PathBuf, String, AccessLevel)>,
    /// Cached access-level decisions for hot paths.
    /// Stores the granted `AccessLevel` for a (path, principal) pair.
    cache: std::sync::Mutex<HashMap<(PathBuf, String), AccessLevel>>,
    /// Default access when no rule matches.
    default: AccessLevel,
}

impl PermissionGate {
    /// Load the permission gate from `context/nap-gate.toml` inside the
    /// workspace.  If the file does not exist, returns a permissive gate
    /// (default: write access for all principals).  Use
    /// [`PermissionGate::strict`] to enforce a closed-by-default gate
    /// when no config is present.
    pub fn load(workspace_root: &Path) -> Result<Self, NapError> {
        let config_path = workspace_root.join("context").join("nap-gate.toml");
        if !config_path.exists() {
            // No gate config → permissive (backwards-compatible).
            return Ok(Self {
                workspace_root: workspace_root.to_path_buf(),
                rules: Vec::new(),
                cache: std::sync::Mutex::new(HashMap::new()),
                default: AccessLevel::Write,
            });
        }

        let contents = std::fs::read_to_string(&config_path).map_err(|e| {
            NapError::Other(format!(
                "failed to read gate config at {:?}: {}",
                config_path, e
            ))
        })?;

        let config: GateConfig = toml::from_str(&contents).map_err(|e| {
            NapError::Other(format!(
                "failed to parse gate config at {:?}: {}",
                config_path, e
            ))
        })?;

        let default = match config.default.as_str() {
            "read" => AccessLevel::Read,
            "write" => AccessLevel::Write,
            "deny" => AccessLevel::None,
            other => {
                return Err(NapError::Other(format!(
                    "unknown default access level '{}' in gate config at {:?}",
                    other, config_path
                )));
            }
        };

        let mut rules: Vec<(PathBuf, String, AccessLevel)> = Vec::new();
        for rule in &config.rules {
            let access = match rule.access.as_str() {
                "read" => AccessLevel::Read,
                "write" => AccessLevel::Write,
                "deny" | "none" => AccessLevel::None,
                other => {
                    return Err(NapError::Other(format!(
                        "unknown access level '{}' in rule for path '{}'",
                        other, rule.path
                    )));
                }
            };
            rules.push((PathBuf::from(&rule.path), rule.principal.clone(), access));
        }

        Ok(Self {
            workspace_root: workspace_root.to_path_buf(),
            rules,
            cache: std::sync::Mutex::new(HashMap::new()),
            default,
        })
    }

    /// Create a strict gate that denies everything by default, even
    /// when no config file is present.  Useful for agents or untrusted
    /// contexts.
    pub fn strict(workspace_root: &Path) -> Self {
        Self {
            workspace_root: workspace_root.to_path_buf(),
            rules: Vec::new(),
            cache: std::sync::Mutex::new(HashMap::new()),
            default: AccessLevel::None,
        }
    }

    /// Create a fully permissive gate (any principal may read/write any
    /// path).  Useful for local development or trusted automation.
    pub fn permissive(workspace_root: &Path) -> Self {
        Self {
            workspace_root: workspace_root.to_path_buf(),
            rules: Vec::new(),
            cache: std::sync::Mutex::new(HashMap::new()),
            default: AccessLevel::Write,
        }
    }

    /// Build a gate from an explicit list of [`Permission`] entries.
    pub fn from_permissions(
        workspace_root: &Path,
        permissions: &[Permission],
        default: AccessLevel,
    ) -> Self {
        let rules: Vec<(PathBuf, String, AccessLevel)> = permissions
            .iter()
            .map(|p| (PathBuf::from(&p.path_prefix), p.principal.clone(), p.access))
            .collect();

        Self {
            workspace_root: workspace_root.to_path_buf(),
            rules,
            cache: std::sync::Mutex::new(HashMap::new()),
            default,
        }
    }

    /// Check whether `principal` may read `path`.
    pub fn check_read(&self, path: &str, principal: &str) -> Result<(), NapError> {
        self.check(path, principal, AccessLevel::Read)
    }

    /// Check whether `principal` may write `path`.
    pub fn check_write(&self, path: &str, principal: &str) -> Result<(), NapError> {
        self.check(path, principal, AccessLevel::Write)
    }

    /// Core check: does the rule set grant `required` access for `principal`
    /// on `path`?
    fn check(&self, path: &str, principal: &str, required: AccessLevel) -> Result<(), NapError> {
        let cache_key = (PathBuf::from(path), principal.to_string());
        {
            let cache = self.cache.lock().unwrap();
            if let Some(&granted) = cache.get(&cache_key) {
                // Cache hit: check if the cached access level satisfies
                // the required level (Write implies Read).
                if granted == AccessLevel::Write {
                    return Ok(());
                }
                if granted == AccessLevel::Read && required == AccessLevel::Read {
                    return Ok(());
                }
                return Err(NapError::PermissionDenied(format!(
                    "access denied to '{}' for '{}' (cached)",
                    path, principal
                )));
            }
        }

        let result = self.check_uncached(path, principal, required);
        // Cache the granted access level (determined by re-checking with Read).
        let granted = self
            .check_uncached(path, principal, AccessLevel::Read)
            .map(|_| {
                // Check if Write is also granted.
                if self
                    .check_uncached(path, principal, AccessLevel::Write)
                    .is_ok()
                {
                    AccessLevel::Write
                } else {
                    AccessLevel::Read
                }
            })
            .unwrap_or(AccessLevel::None);

        {
            let mut cache = self.cache.lock().unwrap();
            cache.insert(cache_key, granted);
        }

        result
    }

    fn check_uncached(
        &self,
        path: &str,
        principal: &str,
        required: AccessLevel,
    ) -> Result<(), NapError> {
        // Normalize paths: strip leading `/` so `Path::join` works
        // correctly on all platforms (macOS treats `/public` as root).
        let norm_path = path.strip_prefix('/').unwrap_or(path);
        let norm_prefix = |p: &Path| -> PathBuf {
            let s = p.to_string_lossy();
            let relative = s.strip_prefix('/').unwrap_or(&s);
            self.workspace_root.join(relative)
        };

        let request_path = self.workspace_root.join(norm_path);

        // Collect all matching rules and pick the one with the longest
        // (most specific) prefix.  This way a `Write` rule on
        // `entities/characters` overrides a `Read` rule on `entities`.
        let mut best_match: Option<(usize, &AccessLevel)> = None;
        let mut best_prefix_len: usize = 0;

        for (rule_prefix, rule_principal, rule_access) in &self.rules {
            let prefix = norm_prefix(rule_prefix);

            if !request_path.starts_with(&prefix) {
                continue;
            }

            if rule_principal != "*" && rule_principal != principal {
                continue;
            }

            let prefix_len = prefix.as_os_str().len();
            if best_match.is_none() || prefix_len > best_prefix_len {
                best_match = Some((prefix_len, rule_access));
                best_prefix_len = prefix_len;
            }
        }

        // ── Apply the best (most specific) matching rule ──────────────
        if let Some((_, access)) = best_match {
            match access {
                AccessLevel::None => {
                    return Err(NapError::PermissionDenied(format!(
                        "principal '{}' is denied access to '{}'",
                        principal, path
                    )));
                }
                AccessLevel::Read => {
                    if required == AccessLevel::Write {
                        return Err(NapError::PermissionDenied(format!(
                            "principal '{}' has read-only access to '{}'",
                            principal, path
                        )));
                    }
                    return Ok(());
                }
                AccessLevel::Write => {
                    return Ok(());
                }
            }
        }

        // ── No rule matched → apply default ──────────────────────────
        match self.default {
            AccessLevel::None => Err(NapError::PermissionDenied(format!(
                "principal '{}' is denied access to '{}' (default deny)",
                principal, path
            ))),
            AccessLevel::Read => {
                if required == AccessLevel::Write {
                    return Err(NapError::PermissionDenied(format!(
                        "principal '{}' has read-only access to '{}' (default read)",
                        principal, path
                    )));
                }
                Ok(())
            }
            AccessLevel::Write => Ok(()),
        }
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn test_permissive_gate_allows_all() {
        let dir = tempfile::TempDir::new().unwrap();
        let gate = PermissionGate::permissive(dir.path());
        assert!(gate.check_read("/any/path", "alice").is_ok());
        assert!(gate.check_write("/any/path", "alice").is_ok());
        assert!(gate.check_write("/any/path", "mallory").is_ok());
    }

    #[test]
    fn test_strict_gate_denies_all() {
        let dir = tempfile::TempDir::new().unwrap();
        let gate = PermissionGate::strict(dir.path());
        assert!(gate.check_read("/any/path", "alice").is_err());
        assert!(gate.check_write("/any/path", "alice").is_err());
    }

    #[test]
    fn test_from_permissions() {
        let dir = tempfile::TempDir::new().unwrap();
        let perms = vec![
            Permission {
                path_prefix: "/public".to_string(),
                principal: "*".to_string(),
                access: AccessLevel::Read,
            },
            Permission {
                path_prefix: "/admin".to_string(),
                principal: "alice".to_string(),
                access: AccessLevel::Write,
            },
        ];
        let gate = PermissionGate::from_permissions(dir.path(), &perms, AccessLevel::None);

        // Public is readable by anyone.
        assert!(gate.check_read("/public/readme.md", "bob").is_ok());
        // But not writable.
        assert!(gate.check_write("/public/readme.md", "bob").is_err());
        // Admin is writeable by alice.
        assert!(gate.check_write("/admin/secret.md", "alice").is_ok());
        // Admin is denied for bob.
        assert!(gate.check_write("/admin/secret.md", "bob").is_err());
        // Default deny path.
        assert!(gate.check_read("/other", "alice").is_err());
    }

    #[test]
    fn test_load_from_file() {
        let dir = tempfile::TempDir::new().unwrap();
        let context_dir = dir.path().join("context");
        fs::create_dir_all(&context_dir).unwrap();

        let config = r#"
default = "deny"

[[rules]]
path = "entities"
principal = "*"
access = "read"

[[rules]]
path = "entities/characters"
principal = "alice"
access = "write"
"#;
        fs::write(context_dir.join("nap-gate.toml"), config).unwrap();

        let gate = PermissionGate::load(dir.path()).unwrap();

        // Everyone can read entities.
        assert!(gate.check_read("entities/foo.yaml", "bob").is_ok());
        // But not write them unless they're alice.
        assert!(gate.check_write("entities/foo.yaml", "bob").is_err());
        assert!(gate.check_write("entities/foo.yaml", "alice").is_err()); // only characters/
        assert!(
            gate.check_write("entities/characters/hero.yaml", "alice")
                .is_ok()
        );
        // Default deny for unconfigured paths.
        assert!(gate.check_read("context/secret.md", "alice").is_err());
    }

    #[test]
    fn test_cache_hits() {
        let dir = tempfile::TempDir::new().unwrap();
        let gate = PermissionGate::permissive(dir.path());

        // First check populates the cache.
        assert!(gate.check_read("/file", "alice").is_ok());
        // Second check hits the cache (won't re-evaluate rules).
        assert!(gate.check_read("/file", "alice").is_ok());
    }

    #[test]
    fn test_cache_miss_different_principal() {
        let dir = tempfile::TempDir::new().unwrap();
        let perms = vec![Permission {
            path_prefix: "/".to_string(),
            principal: "alice".to_string(),
            access: AccessLevel::Write,
        }];
        let gate = PermissionGate::from_permissions(dir.path(), &perms, AccessLevel::None);

        assert!(gate.check_read("/file", "alice").is_ok());
        // Different principal → different cache key → re-evaluated.
        assert!(gate.check_read("/file", "bob").is_err());
    }

    #[test]
    fn test_invalid_config_default() {
        let dir = tempfile::TempDir::new().unwrap();
        let context_dir = dir.path().join("context");
        fs::create_dir_all(&context_dir).unwrap();
        fs::write(
            context_dir.join("nap-gate.toml"),
            r#"default = "superadmin""#,
        )
        .unwrap();

        let result = PermissionGate::load(dir.path());
        assert!(result.is_err());
        assert!(
            result.unwrap_err().to_string().contains("unknown default"),
            "expected 'unknown default' error"
        );
    }
}
