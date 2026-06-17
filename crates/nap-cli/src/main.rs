//! NAP CLI — command-line interface for the Narrative Addressing Protocol.
//!
//! Commands:
//!   init     — Initialize a universe repository
//!   create   — Create an entity manifest
//!   resolve  — Resolve a NAP URI (with optional fragment query)
//!   query    — Query a subtree from a manifest
//!   commit   — Commit changes to a manifest
//!   history  — View commit history for an entity
//!   list     — List entities or universes
//!   branch   — Create or list branches
//!   tag      — Create or list tags
//!   pull     — Clone or pull a universe from a remote
//!   push     — Push a universe to a remote
//!   remote   — Manage git remotes on a universe
//!   sign     — Sign a manifest (stub for v0)
//!   verify   — Verify a manifest signature (stub for v0)

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use nap_core::{
    commit::Change,
    manifest::Representation,
    repository::Repository,
    resolver::{ResolveOptions, ResolveResult, Resolver},
    types::EntityType,
    uri::NapUri,
    vcs_git::GitBackend,
};
use std::io::IsTerminal;
use std::path::{Path, PathBuf};

/// Expand a path, resolving a leading `~` to the user's home directory.
fn expand_path(path: &Path) -> PathBuf {
    let s = path.to_string_lossy();
    if s.starts_with('~')
        && let Ok(home) = std::env::var("HOME")
    {
        let rest = s.strip_prefix('~').unwrap_or("");
        return PathBuf::from(format!("{home}{rest}"));
    }
    path.to_path_buf()
}

/// NAP — Narrative Addressing Protocol CLI
///
/// Identity, addressing, resolution, and attribution for entertainment media.
#[derive(Parser, Debug)]
#[command(name = "nap", version, about, long_about = None)]
struct Cli {
    /// Base directory for universe repositories.
    /// Defaults to $NAP_DIR, or ~/.nap if unset.
    #[arg(long, short = 'd', global = true)]
    base_dir: Option<PathBuf>,

    /// Enable verbose debug logging.
    #[arg(long, short = 'v', global = true)]
    verbose: bool,

    #[command(subcommand)]
    command: Commands,
}

/// Return `true` if `s` looks like a URL rather than a universe name.
///
/// Universe names are simple identifiers (`[a-zA-Z0-9_-]+`).
/// Everything else (contains `@`, `://`, `/`, `.git`, etc.) is a URL.
fn looks_like_url(s: &str) -> bool {
    !s.chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
}

/// Subcommands for `nap remote`.
#[derive(Subcommand, Debug)]
enum RemoteCmd {
    /// Add a remote to a universe repository.
    Add {
        /// Universe name.
        universe: String,
        /// Remote name (e.g., "origin").
        name: String,
        /// Remote URL.
        url: String,
    },
    /// List remotes on a universe repository.
    Ls {
        /// Universe name.
        universe: String,
    },
    /// Remove a remote from a universe repository.
    Rm {
        /// Universe name.
        universe: String,
        /// Remote name to remove.
        name: String,
    },
}

#[derive(Subcommand, Debug)]
enum Commands {
    /// Initialize a new universe repository.
    Init {
        /// Universe name (e.g., "starwars", "toystory").
        universe: String,

        /// Remote URL to add as origin after init.
        #[arg(long)]
        remote: Option<String>,
    },

    /// Create a new entity manifest.
    Create {
        /// Entity type: character, location, scene, prop, world.
        entity_type: String,

        /// Entity ID (slug). e.g., "lukeskywalker".
        entity_id: String,

        /// Universe name.
        #[arg(long, short = 'u')]
        universe: String,

        /// Human-readable name.
        #[arg(long, short = 'n')]
        name: String,

        /// Author identifier.
        #[arg(long, short = 'a', default_value = "nap-cli")]
        author: String,
    },

    /// Resolve a NAP URI to its manifest or a subtree.
    ///
    /// Fragment queries are supported via the URI:
    ///   nap resolve nap://starwars/character/lukeskywalker#references.appears_in
    Resolve {
        /// NAP URI. e.g., "nap://starwars/character/lukeskywalker"
        uri: String,

        /// Resolve at a specific branch.
        #[arg(long)]
        branch: Option<String>,

        /// Resolve at a specific commit hash.
        #[arg(long)]
        commit: Option<String>,

        /// Resolve at a specific tag.
        #[arg(long)]
        tag: Option<String>,

        /// Output format: yaml, json.
        #[arg(long, short = 'f', default_value = "yaml")]
        format: String,
    },

    /// Query a subtree from a manifest.
    Query {
        /// NAP URI.
        uri: String,

        /// Dot-notation path. e.g., "appearances.audienceVotes".
        path: String,

        /// Output format: yaml, json.
        #[arg(long, short = 'f', default_value = "json")]
        format: String,
    },

    /// Commit changes to a universe repository.
    Commit {
        /// Universe name.
        universe: String,

        /// Commit message.
        #[arg(long, short = 'm')]
        message: String,

        /// Author identifier.
        #[arg(long, short = 'a', default_value = "nap-cli")]
        author: String,
    },

    /// View commit history for an entity.
    History {
        /// NAP URI.
        uri: String,

        /// Maximum number of commits to show.
        #[arg(long, short = 'n', default_value = "20")]
        limit: usize,
    },

    /// List universes or entities within a universe.
    List {
        /// Universe name. Omit to list all universes.
        universe: Option<String>,

        /// Entity type to list (if universe is specified).
        #[arg(long, short = 't')]
        entity_type: Option<String>,
    },

    /// Create or list branches.
    Branch {
        /// Universe name.
        universe: String,

        /// Branch name to create. Omit to list all branches.
        name: Option<String>,
    },

    /// Create or list tags.
    Tag {
        /// Universe name.
        universe: String,

        /// Tag name to create. Omit to list all tags.
        name: Option<String>,
    },

    /// Set a property on an entity manifest.
    Set {
        /// NAP URI.
        uri: String,

        /// Property key (dot-notation).
        key: String,

        /// Property value.
        value: String,

        /// Commit message.
        #[arg(long, short = 'm', default_value = "set property")]
        message: String,

        /// Author identifier.
        #[arg(long, short = 'a', default_value = "nap-cli")]
        author: String,
    },

    /// Add a representation to an entity manifest.
    AddRepr {
        /// NAP URI.
        uri: String,

        /// Representation key. e.g., "reference_image".
        key: String,

        /// File path to the asset.
        file: PathBuf,

        /// Asset format. e.g., "png", "glb".
        #[arg(long)]
        format: String,

        /// Commit message.
        #[arg(long, short = 'm', default_value = "add representation")]
        message: String,

        /// Author identifier.
        #[arg(long, short = 'a', default_value = "nap-cli")]
        author: String,
    },

    /// Revert a commit by hash (undoes all changes in that commit).
    Revert {
        /// Universe name.
        universe: String,

        /// Commit hash to revert.
        #[arg(long, short = 'c')]
        commit: String,

        /// Author identifier.
        #[arg(long, short = 'a', default_value = "nap-cli")]
        author: String,
    },

    /// Clone or pull a universe from a remote.
    ///
    /// If the argument is a URL, the repo is cloned (name is read from the
    /// repo's own config).  If it's a universe name, the repo must already
    /// exist locally and will be updated via `git pull`.
    Pull {
        /// URL (clone) or universe name (pull existing).
        url_or_name: String,
    },

    /// Push the current branch to its configured upstream remote.
    Push {
        /// Universe name.
        universe: String,

        /// Remote name (default: tracking branch's remote, or "origin").
        #[arg(long, default_value = "origin")]
        remote: String,

        /// Branch to push (default: current branch).
        #[arg(long)]
        branch: Option<String>,
    },

    /// Manage git remotes on a universe.
    #[command(subcommand)]
    Remote(RemoteCmd),

    /// Sign a manifest (stub for v0).
    Sign {
        /// NAP URI.
        uri: String,
    },

    /// Verify a manifest signature (stub for v0).
    Verify {
        /// NAP URI.
        uri: String,
    },
}

/// Determine output format for a command.
///
/// Priority:
///   1. `--format` flag (honored when terminal; default remains as-set)
///   2. `NAP_OUTPUT` env var (explicit override regardless of terminal state)
///   3. Auto-detection: if stdout is not a terminal (piped), use JSON
fn resolve_output_format(requested: &str) -> String {
    // Env var takes highest priority
    if let Ok(env_val) = std::env::var("NAP_OUTPUT") {
        let val = env_val.trim().to_lowercase();
        if val == "json" || val == "yaml" {
            return val;
        }
    }

    // Auto-detect piped output
    if !std::io::stdout().is_terminal() {
        return "json".to_string();
    }

    // Default to what was requested (honor --format flag)
    requested.to_string()
}

/// Emit a human-friendly or machine-readable message depending on stdout.
fn emit(msg: impl AsRef<str>) {
    let msg = msg.as_ref();
    if std::io::stdout().is_terminal() {
        println!("{msg}");
    } else {
        // Piped/agent output → JSON structured log
        let entry = serde_json::json!({
            "level": "info",
            "message": msg,
        });
        println!(
            "{}",
            serde_json::to_string(&entry).unwrap_or_else(|_| msg.to_string())
        );
    }
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    let is_piped = !std::io::stdout().is_terminal();

    // Initialize tracing
    let filter = if cli.verbose {
        "nap_core=trace,nap_cli=trace"
    } else {
        "nap_core=info,nap_cli=info"
    };
    tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_target(false)
        .init();

    // Resolve base directory: -d flag > $NAP_DIR > ~/.nap
    let base_dir = cli
        .base_dir
        .or_else(|| std::env::var("NAP_DIR").ok().map(PathBuf::from))
        .unwrap_or_else(|| PathBuf::from("~/.nap"));
    let base_dir = expand_path(&base_dir);

    // Ensure the base directory exists (e.g. ~/.nap/)
    std::fs::create_dir_all(&base_dir)
        .with_context(|| format!("failed to create base directory '{}'", base_dir.display()))?;

    let result = match cli.command {
        Commands::Init { universe, remote } => cmd_init(&base_dir, &universe, remote.as_deref()),
        Commands::Create {
            entity_type,
            entity_id,
            universe,
            name,
            author,
        } => cmd_create(
            &base_dir,
            &universe,
            &entity_type,
            &entity_id,
            &name,
            &author,
        ),
        Commands::Resolve {
            uri,
            branch,
            commit,
            tag,
            format,
        } => cmd_resolve(&base_dir, &uri, branch, commit, tag, &format),
        Commands::Query { uri, path, format } => cmd_query(&base_dir, &uri, &path, &format),
        Commands::Commit {
            universe,
            message,
            author,
        } => cmd_commit(&base_dir, &universe, &message, &author),
        Commands::History { uri, limit } => cmd_history(&base_dir, &uri, limit),
        Commands::List {
            universe,
            entity_type,
        } => cmd_list(&base_dir, universe.as_deref(), entity_type.as_deref()),
        Commands::Branch { universe, name } => cmd_branch(&base_dir, &universe, name.as_deref()),
        Commands::Tag { universe, name } => cmd_tag(&base_dir, &universe, name.as_deref()),
        Commands::Set {
            uri,
            key,
            value,
            message,
            author,
        } => cmd_set(&base_dir, &uri, &key, &value, &message, &author),
        Commands::AddRepr {
            uri,
            key,
            file,
            format,
            message,
            author,
        } => cmd_add_repr(&base_dir, &uri, &key, &file, &format, &message, &author),
        Commands::Revert {
            universe,
            commit,
            author,
        } => cmd_revert(&base_dir, &universe, &commit, &author),
        Commands::Pull { url_or_name } => cmd_pull(&base_dir, &url_or_name),
        Commands::Push {
            universe,
            remote,
            branch,
        } => cmd_push(&base_dir, &universe, &remote, branch.as_deref()),
        Commands::Remote(cmd) => cmd_remote(&base_dir, cmd),
        Commands::Sign { uri } => cmd_sign(&uri),
        Commands::Verify { uri } => cmd_verify(&uri),
    };

    if let Err(err) = result {
        if is_piped {
            let error_json = serde_json::json!({
                "level": "error",
                "error": err.to_string(),
                "code": "CLI_ERROR",
            });
            eprintln!("{}", serde_json::to_string(&error_json).unwrap());
        }
        return Err(err);
    }
    Ok(())
}

fn open_repo(base_dir: &Path, universe: &str) -> Result<Repository> {
    let repo_path = base_dir.join(universe);
    Repository::open(&repo_path, Box::new(GitBackend::new()))
        .context(format!("failed to open universe '{universe}'"))
}

fn cmd_init(base_dir: &Path, universe: &str, remote: Option<&str>) -> Result<()> {
    let repo = Repository::init(base_dir, universe, Box::new(GitBackend::new()))
        .context(format!("failed to initialize universe '{universe}'"))?;
    emit(format!(
        "✓ Initialized universe '{universe}' at {}/{universe}",
        base_dir.display()
    ));

    if let Some(url) = remote {
        repo.add_remote("origin", url)
            .context(format!("failed to add remote origin '{url}'"))?;
        emit(format!("  Added remote 'origin' → {url}"));
    }

    Ok(())
}

fn cmd_create(
    base_dir: &Path,
    universe: &str,
    entity_type_str: &str,
    entity_id: &str,
    name: &str,
    author: &str,
) -> Result<()> {
    let entity_type: EntityType = entity_type_str
        .parse()
        .context(format!("unknown entity type '{entity_type_str}'"))?;
    let repo = open_repo(base_dir, universe)?;
    let (manifest, hash) = repo
        .create_entity(entity_type, entity_id, name, author)
        .context("failed to create entity")?;
    emit(format!(
        "✓ Created {} '{}' ({})",
        entity_type, name, entity_id
    ));
    emit(format!("  URI:    {}", manifest.id));
    emit(format!("  Commit: {}", &hash[..12]));
    Ok(())
}

fn cmd_resolve(
    base_dir: &Path,
    uri_str: &str,
    branch: Option<String>,
    commit: Option<String>,
    tag: Option<String>,
    format: &str,
) -> Result<()> {
    let resolver = Resolver::new(base_dir);
    let options = ResolveOptions {
        branch,
        commit,
        tag,
        path: None,
    };
    let result = resolver
        .resolve(uri_str, &options)
        .context(format!("failed to resolve '{uri_str}'"))?;

    let fmt = resolve_output_format(format);
    match result {
        ResolveResult::Full(manifest) => match fmt.as_str() {
            "json" => println!("{}", serde_json::to_string_pretty(&manifest)?),
            _ => println!("{}", serde_yaml::to_string(&manifest)?),
        },
        ResolveResult::Subtree(value) => match fmt.as_str() {
            "yaml" => {
                let yaml: serde_yaml::Value = serde_json::from_value(value)?;
                println!("{}", serde_yaml::to_string(&yaml)?);
            }
            _ => println!("{}", serde_json::to_string_pretty(&value)?),
        },
    }
    Ok(())
}

fn cmd_query(base_dir: &Path, uri_str: &str, path: &str, format: &str) -> Result<()> {
    let resolver = Resolver::new(base_dir);
    let result = resolver
        .query(uri_str, path)
        .context(format!("failed to query '{uri_str}#{path}'"))?;

    let fmt = resolve_output_format(format);
    match fmt.as_str() {
        "yaml" => {
            let yaml: serde_yaml::Value = serde_json::from_value(result)?;
            println!("{}", serde_yaml::to_string(&yaml)?);
        }
        _ => println!("{}", serde_json::to_string_pretty(&result)?),
    }
    Ok(())
}

fn cmd_commit(base_dir: &Path, universe: &str, message: &str, author: &str) -> Result<()> {
    let repo_path = base_dir.join(universe);
    let vcs = GitBackend::new();
    let hash = nap_core::vcs::VcsBackend::commit(&vcs, &repo_path, message, author)
        .context("failed to commit")?;
    emit(format!("✓ Committed: {} ({})", message, &hash[..12]));
    Ok(())
}

fn cmd_history(base_dir: &Path, uri_str: &str, limit: usize) -> Result<()> {
    let uri: NapUri = uri_str.parse().context("invalid URI")?;
    let repo = open_repo(base_dir, &uri.universe)?;
    let history = repo
        .history(uri.entity_type, &uri.entity_id, limit)
        .context("failed to get history")?;

    if history.is_empty() {
        emit(format!("No history found for {uri_str}"));
        return Ok(());
    }

    if std::io::stdout().is_terminal() {
        for entry in &history {
            let short_hash = if entry.id.len() > 12 {
                &entry.id[..12]
            } else {
                &entry.id
            };
            println!(
                "{} {} — {} ({})",
                short_hash, entry.timestamp, entry.message, entry.author
            );
        }
    } else {
        // Piped: emit full JSON array
        println!("{}", serde_json::to_string_pretty(&history)?);
    }
    Ok(())
}

fn cmd_list(base_dir: &Path, universe: Option<&str>, entity_type: Option<&str>) -> Result<()> {
    let is_piped = !std::io::stdout().is_terminal();

    match universe {
        None => {
            let resolver = Resolver::new(base_dir);
            let universes = resolver
                .list_universes()
                .context("failed to list universes")?;
            if is_piped {
                println!("{}", serde_json::to_string_pretty(&universes)?);
            } else if universes.is_empty() {
                println!("No universes found in {}", base_dir.display());
            } else {
                println!("Universes:");
                for u in &universes {
                    println!("  nap://{u}/");
                }
            }
        }
        Some(universe) => {
            let repo = open_repo(base_dir, universe)?;
            let is_piped = !std::io::stdout().is_terminal();
            match entity_type {
                Some(et_str) => {
                    let et: EntityType = et_str.parse().context("unknown entity type")?;
                    let entities = repo.list_entities(et).context("failed to list entities")?;
                    if is_piped {
                        println!("{}", serde_json::to_string_pretty(&entities)?);
                    } else {
                        println!("{} in {universe}:", et_str);
                        for e in &entities {
                            println!("  nap://{universe}/{et}/{e}");
                        }
                    }
                }
                None => {
                    // Collect all entities grouped by type → JSON for piped, human for terminal
                    let mut all: Vec<serde_json::Value> = Vec::new();
                    for et in EntityType::subdirectory_types() {
                        let entities = repo.list_entities(*et).unwrap_or_default();
                        if is_piped && !entities.is_empty() {
                            for e in &entities {
                                all.push(serde_json::json!({
                                    "type": et.to_string(),
                                    "id": e,
                                    "uri": format!("nap://{universe}/{et}/{e}"),
                                }));
                            }
                        } else if !entities.is_empty() {
                            println!("{}:", et);
                            for e in &entities {
                                println!("  nap://{universe}/{et}/{e}");
                            }
                        }
                    }
                    if is_piped {
                        println!("{}", serde_json::to_string_pretty(&all)?);
                    }
                }
            }
        }
    }
    Ok(())
}

fn cmd_branch(base_dir: &Path, universe: &str, name: Option<&str>) -> Result<()> {
    let repo = open_repo(base_dir, universe)?;
    match name {
        Some(branch_name) => {
            repo.create_branch(branch_name)
                .context(format!("failed to create branch '{branch_name}'"))?;
            emit(format!("✓ Created branch '{branch_name}' in {universe}"));
        }
        None => {
            let branches = repo.list_branches().context("failed to list branches")?;
            if !std::io::stdout().is_terminal() {
                println!("{}", serde_json::to_string_pretty(&branches)?);
            } else {
                println!("Branches in {universe}:");
                for b in &branches {
                    println!("  {b}");
                }
            }
        }
    }
    Ok(())
}

fn cmd_tag(base_dir: &Path, universe: &str, name: Option<&str>) -> Result<()> {
    let repo = open_repo(base_dir, universe)?;
    match name {
        Some(tag_name) => {
            repo.create_tag(tag_name)
                .context(format!("failed to create tag '{tag_name}'"))?;
            emit(format!("✓ Created tag '{tag_name}' in {universe}"));
        }
        None => {
            let tags = repo.list_tags().context("failed to list tags")?;
            if !std::io::stdout().is_terminal() {
                println!("{}", serde_json::to_string_pretty(&tags)?);
            } else if tags.is_empty() {
                println!("No tags in {universe}");
            } else {
                println!("Tags in {universe}:");
                for t in &tags {
                    println!("  {t}");
                }
            }
        }
    }
    Ok(())
}

fn cmd_pull(base_dir: &Path, url_or_name: &str) -> Result<()> {
    if looks_like_url(url_or_name) {
        // ── Clone from URL ──────────────────────────────────────
        // Clone to a temp directory, read the universe name from the
        // repo's own config, then rename to the final directory.

        let tmp_suffix = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        let tmp_name = format!(".__nap_clone_{tmp_suffix}");
        let tmp_path = base_dir.join(&tmp_name);

        emit(format!("  Cloning from {url_or_name} …"));
        GitBackend::clone_repo(url_or_name, &tmp_path).context("failed to clone repository")?;

        // Read the universe name from .nap/config.yaml
        let config_path = tmp_path.join(".nap").join("config.yaml");
        let name = if config_path.exists() {
            let config_content = std::fs::read_to_string(&config_path)
                .context("cloned repo is missing or corrupt .nap/config.yaml")?;
            // Parse universe name from YAML front matter
            let config_yaml: serde_yaml::Value = serde_yaml::from_str(&config_content)
                .context("invalid .nap/config.yaml in cloned repo")?;
            config_yaml["universe"]
                .as_str()
                .map(|s| s.to_string())
                .ok_or_else(|| anyhow::anyhow!("missing 'universe' key in .nap/config.yaml"))?
        } else {
            anyhow::bail!("not a NAP universe repository: missing .nap/config.yaml");
        };

        // Check if the target directory already exists
        let target = base_dir.join(&name);
        if target.exists() {
            // Clean up the temp clone
            std::fs::remove_dir_all(&tmp_path).context("failed to clean up temp clone")?;
            anyhow::bail!("universe '{name}' already exists at {}", target.display());
        }

        // Rename temp → final
        std::fs::rename(&tmp_path, &target)
            .context(format!("failed to rename {tmp_name} → {name}"))?;

        emit(format!(
            "✓ Cloned universe '{name}' to {}",
            target.display()
        ));
    } else {
        // ── Pull existing repo ───────────────────────────────────
        let repo = open_repo(base_dir, url_or_name)?;
        repo.pull(None, None)
            .context("failed to pull latest changes")?;
        emit(format!("✓ Pulled latest changes for '{url_or_name}'"));
    }

    Ok(())
}

fn cmd_push(base_dir: &Path, universe: &str, remote: &str, branch: Option<&str>) -> Result<()> {
    let repo = open_repo(base_dir, universe)?;
    repo.push(Some(remote), branch)
        .context("failed to push to remote")?;
    match branch {
        Some(b) => emit(format!("✓ Pushed '{universe}' ({b}) → {remote}")),
        None => emit(format!("✓ Pushed '{universe}' → {remote}")),
    }
    Ok(())
}

fn cmd_remote(base_dir: &Path, cmd: RemoteCmd) -> Result<()> {
    match cmd {
        RemoteCmd::Add {
            universe,
            name,
            url,
        } => {
            let repo = open_repo(base_dir, &universe)?;
            repo.add_remote(&name, &url)
                .context(format!("failed to add remote '{name}'"))?;
            emit(format!("✓ Added remote '{name}' → {url} to '{universe}'"));
        }
        RemoteCmd::Ls { universe } => {
            let repo = open_repo(base_dir, &universe)?;
            let remotes = repo.list_remotes().context("failed to list remotes")?;
            if remotes.is_empty() {
                emit(format!("No remotes configured for '{universe}'"));
            } else {
                if std::io::stdout().is_terminal() {
                    println!("Remotes in '{universe}':");
                    for (name, url) in &remotes {
                        println!("  {name}\t{url}");
                    }
                } else {
                    let pairs: Vec<serde_json::Value> = remotes
                        .iter()
                        .map(|(n, u)| serde_json::json!({ "name": n, "url": u }))
                        .collect();
                    println!("{}", serde_json::to_string_pretty(&pairs)?);
                }
            }
        }
        RemoteCmd::Rm { universe, name } => {
            let repo = open_repo(base_dir, &universe)?;
            repo.remove_remote(&name)
                .context(format!("failed to remove remote '{name}'"))?;
            emit(format!("✓ Removed remote '{name}' from '{universe}'"));
        }
    }
    Ok(())
}

fn cmd_set(
    base_dir: &Path,
    uri_str: &str,
    key: &str,
    value: &str,
    message: &str,
    author: &str,
) -> Result<()> {
    let uri: NapUri = uri_str.parse().context("invalid URI")?;
    let repo = open_repo(base_dir, &uri.universe)?;
    let mut manifest = repo
        .read_manifest(uri.entity_type, &uri.entity_id)
        .context("failed to read manifest")?;

    // Parse value — try as YAML for structured values, fallback to string
    let yaml_value: serde_yaml::Value = serde_yaml::from_str(value)
        .unwrap_or_else(|_| serde_yaml::Value::String(value.to_string()));

    manifest.set_property(key, yaml_value);
    let changes = vec![Change::set(
        &format!("properties.{key}"),
        None,
        value.to_string(),
    )];

    repo.commit_manifest(&mut manifest, message, author, changes)
        .context("failed to commit property change")?;

    emit(format!("✓ Set {key} = {value} on {uri_str}"));
    Ok(())
}

fn cmd_add_repr(
    base_dir: &Path,
    uri_str: &str,
    key: &str,
    file: &Path,
    format: &str,
    message: &str,
    author: &str,
) -> Result<()> {
    let uri: NapUri = uri_str.parse().context("invalid URI")?;
    let repo = open_repo(base_dir, &uri.universe)?;
    let mut manifest = repo
        .read_manifest(uri.entity_type, &uri.entity_id)
        .context("failed to read manifest")?;

    // Compute content hash
    let hash = nap_core::ContentHash::from_file(file)
        .context(format!("failed to hash file '{}'", file.display()))?;

    let repr = Representation {
        hash: hash.as_str().to_string(),
        format: format.to_string(),
        uri: Some(file.display().to_string()),
        tier: None,
    };

    manifest.set_representation(key, repr);
    let changes = vec![Change::set(
        &format!("representations.{key}"),
        None,
        hash.as_str().to_string(),
    )];

    repo.commit_manifest(&mut manifest, message, author, changes)
        .context("failed to commit representation")?;

    emit(format!(
        "✓ Added representation '{key}' ({format}) to {uri_str}"
    ));
    emit(format!("  Hash: {hash}"));
    Ok(())
}

fn cmd_revert(base_dir: &Path, universe: &str, commit: &str, author: &str) -> Result<()> {
    let repo = open_repo(base_dir, universe)?;
    let new_hash = repo
        .revert_commit(commit, author)
        .context(format!("failed to revert commit '{commit}'"))?;
    let short_old = if commit.len() > 12 {
        &commit[..12]
    } else {
        commit
    };
    let short_new = &new_hash[..12.min(new_hash.len())];
    emit(format!(
        "✓ Reverted commit {short_old} — new commit: {short_new}"
    ));
    Ok(())
}

fn cmd_sign(uri_str: &str) -> Result<()> {
    emit(format!("⚠ Sign not implemented in v0. URI: {uri_str}"));
    emit("  Future: Ed25519 signing of manifest content hash.");
    Ok(())
}

fn cmd_verify(uri_str: &str) -> Result<()> {
    emit(format!("⚠ Verify not implemented in v0. URI: {uri_str}"));
    emit("  Future: Ed25519 signature verification.");
    Ok(())
}
