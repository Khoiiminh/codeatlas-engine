//! Architecture boundary checks for the engine workspace (FND-05, ADR-0010).
//!
//! `MATRIX` is a copy of the normative dependency matrix in `crates/README.md`.
//! Change both together, and amend ADR-0010 for any new adapter-to-adapter edge.

use std::collections::{BTreeMap, BTreeSet};

/// Clean Architecture layer, derived from the crate's folder under `crates/`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Layer {
    Domain,
    Application,
    Adapters,
    Entrypoints,
}

impl Layer {
    pub fn from_dir(dir: &str) -> Option<Layer> {
        match dir {
            "domain" => Some(Layer::Domain),
            "application" => Some(Layer::Application),
            "adapters" => Some(Layer::Adapters),
            "entrypoints" => Some(Layer::Entrypoints),
            _ => None,
        }
    }

    fn dir(self) -> &'static str {
        match self {
            Layer::Domain => "domain",
            Layer::Application => "application",
            Layer::Adapters => "adapters",
            Layer::Entrypoints => "entrypoints",
        }
    }
}

/// Workspace crates each crate may depend on (any dependency kind).
/// `"*"` marks the composition root, which may depend on every product crate.
pub const MATRIX: &[(&str, Layer, &[&str])] = &[
    ("codeatlas-core", Layer::Domain, &[]),
    ("codeatlas-graph", Layer::Domain, &["codeatlas-core"]),
    (
        "codeatlas-impact",
        Layer::Domain,
        &["codeatlas-core", "codeatlas-graph"],
    ),
    (
        "codeatlas-application",
        Layer::Application,
        &["codeatlas-core", "codeatlas-graph", "codeatlas-impact"],
    ),
    (
        "codeatlas-protocol",
        Layer::Adapters,
        &["codeatlas-core", "codeatlas-application"],
    ),
    (
        "codeatlas-runtime",
        Layer::Adapters,
        &[
            "codeatlas-core",
            "codeatlas-application",
            "codeatlas-protocol",
        ],
    ),
    (
        "codeatlas-scanner-fs",
        Layer::Adapters,
        &["codeatlas-core", "codeatlas-application"],
    ),
    (
        "codeatlas-parser-treesitter",
        Layer::Adapters,
        &["codeatlas-core", "codeatlas-application"],
    ),
    (
        "codeatlas-semantic-typescript",
        Layer::Adapters,
        &["codeatlas-core", "codeatlas-application"],
    ),
    (
        "codeatlas-storage-sqlite",
        Layer::Adapters,
        &["codeatlas-core", "codeatlas-application"],
    ),
    ("codeatlas-cli", Layer::Entrypoints, &["*"]),
];

/// External crates that must not reach domain or application crates, directly or
/// transitively, through normal or build dependencies. A trailing `*` is a prefix match.
pub const FORBIDDEN_IN_INNER_LAYERS: &[&str] = &[
    "rusqlite",
    "libsqlite3-sys",
    "tree-sitter*",
    "serde_json",
    "tokio*",
    "async-std",
    "smol",
    "interprocess",
    "ipc-channel",
];

/// Workspace members that are tools, not product crates.
pub const TOOLING_CRATES: &[&str] = &["codeatlas-architecture-tests"];

/// A workspace crate as seen by the checker.
#[derive(Clone, Debug)]
pub struct Crate {
    pub name: String,
    /// Crate directory relative to the workspace root, with `/` separators.
    pub rel_dir: String,
    /// Workspace crates it depends on directly (all dependency kinds).
    pub workspace_deps: BTreeSet<String>,
    /// External crate names reachable through normal/build dependencies.
    pub transitive_external: BTreeSet<String>,
}

fn matches(pattern: &str, name: &str) -> bool {
    match pattern.strip_suffix('*') {
        Some(prefix) => name.starts_with(prefix),
        None => pattern == name,
    }
}

/// Returns one message per violation; an empty result means the graph is valid.
pub fn check(crates: &[Crate]) -> Vec<String> {
    let rules: BTreeMap<&str, (Layer, &[&str])> =
        MATRIX.iter().map(|(n, l, d)| (*n, (*l, *d))).collect();
    let mut out = Vec::new();

    for c in crates
        .iter()
        .filter(|c| !TOOLING_CRATES.contains(&c.name.as_str()))
    {
        let Some((layer, allowed)) = rules.get(c.name.as_str()) else {
            out.push(format!(
                "{} is not in the dependency matrix; add it to crates/README.md and tests/architecture/src/lib.rs",
                c.name
            ));
            continue;
        };

        let parts: Vec<&str> = c.rel_dir.split('/').collect();
        let dir_layer = match parts.as_slice() {
            ["crates", dir, _] => Layer::from_dir(dir),
            _ => None,
        };
        if dir_layer != Some(*layer) {
            out.push(format!(
                "{} is at {} but belongs in crates/{}/{}",
                c.name,
                c.rel_dir,
                layer.dir(),
                c.name
            ));
        }

        for dep in &c.workspace_deps {
            if TOOLING_CRATES.contains(&dep.as_str()) {
                out.push(format!("{} depends on tooling crate {}", c.name, dep));
            } else if !allowed.contains(&"*") && !allowed.contains(&dep.as_str()) {
                let allowed = if allowed.is_empty() {
                    "none".to_string()
                } else {
                    allowed.join(", ")
                };
                out.push(format!(
                    "{} -> {} is forbidden by the dependency matrix (allowed: {})",
                    c.name, dep, allowed
                ));
            }
        }

        if *layer <= Layer::Application {
            for ext in &c.transitive_external {
                if FORBIDDEN_IN_INNER_LAYERS.iter().any(|p| matches(p, ext)) {
                    out.push(format!(
                        "{} ({} layer) reaches forbidden external crate {}",
                        c.name,
                        layer.dir(),
                        ext
                    ));
                }
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn krate(name: &str, rel_dir: &str, deps: &[&str], ext: &[&str]) -> Crate {
        Crate {
            name: name.into(),
            rel_dir: rel_dir.into(),
            workspace_deps: deps.iter().map(|s| s.to_string()).collect(),
            transitive_external: ext.iter().map(|s| s.to_string()).collect(),
        }
    }

    #[test]
    fn accepts_allowed_edges() {
        let v = check(&[
            krate(
                "codeatlas-graph",
                "crates/domain/codeatlas-graph",
                &["codeatlas-core"],
                &[],
            ),
            krate(
                "codeatlas-runtime",
                "crates/adapters/codeatlas-runtime",
                &["codeatlas-protocol"],
                &["tokio"],
            ),
            krate(
                "codeatlas-cli",
                "crates/entrypoints/codeatlas-cli",
                &["codeatlas-storage-sqlite"],
                &[],
            ),
        ]);
        assert!(v.is_empty(), "{v:?}");
    }

    #[test]
    fn rejects_outward_dependency() {
        let v = check(&[krate(
            "codeatlas-core",
            "crates/domain/codeatlas-core",
            &["codeatlas-storage-sqlite"],
            &[],
        )]);
        assert_eq!(v.len(), 1, "{v:?}");
        assert!(v[0].contains("codeatlas-core -> codeatlas-storage-sqlite"));
    }

    #[test]
    fn rejects_unlisted_adapter_to_adapter_edge() {
        let v = check(&[krate(
            "codeatlas-scanner-fs",
            "crates/adapters/codeatlas-scanner-fs",
            &["codeatlas-storage-sqlite"],
            &[],
        )]);
        assert_eq!(v.len(), 1, "{v:?}");
    }

    #[test]
    fn rejects_forbidden_external_in_inner_layers() {
        let v = check(&[krate(
            "codeatlas-application",
            "crates/application/codeatlas-application",
            &[],
            &["serde", "tree-sitter-typescript"],
        )]);
        assert_eq!(v.len(), 1, "{v:?}");
        assert!(v[0].contains("tree-sitter-typescript"));
    }

    #[test]
    fn rejects_unknown_and_misplaced_crates() {
        let v = check(&[
            krate("codeatlas-mcp", "crates/adapters/codeatlas-mcp", &[], &[]),
            krate(
                "codeatlas-graph",
                "crates/adapters/codeatlas-graph",
                &["codeatlas-core"],
                &[],
            ),
        ]);
        assert_eq!(v.len(), 2, "{v:?}");
    }
}
