//! Shared test support: finding Node and the analyzer in this checkout.

#![allow(dead_code)]

use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use codeatlas_core::{ContentHash, Language, RepoPath, SourceFile};
use codeatlas_semantic_typescript::{AnalyzerCommand, TypeScriptResolver};

pub fn engine_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../..")
}

/// `CODEATLAS_NODE`, or `node` on `PATH`.
pub fn node_program() -> PathBuf {
    env::var_os("CODEATLAS_NODE").map_or_else(|| PathBuf::from("node"), PathBuf::from)
}

/// Node, if it runs here. CI must have it (ADR-0014 decision 7): there a missing Node fails
/// the test instead of skipping it.
pub fn node() -> Option<PathBuf> {
    let program = node_program();
    let runs = Command::new(&program)
        .arg("--version")
        .output()
        .is_ok_and(|o| o.status.success());
    if runs {
        return Some(program);
    }
    assert!(
        env::var_os("CI").is_none(),
        "CI must be able to run Node ({})",
        program.display()
    );
    eprintln!("Node is not available here; skipping");
    None
}

/// The real analyzer, if Node runs and `npm ci` has installed TypeScript next to it.
pub fn analyzer() -> Option<TypeScriptResolver> {
    let node = node()?;
    let package = engine_root().join("packages/typescript-analyzer");
    if !package
        .join("node_modules/typescript/package.json")
        .is_file()
    {
        assert!(
            env::var_os("CI").is_none(),
            "CI must install the analyzer: npm ci in packages/typescript-analyzer"
        );
        eprintln!("packages/typescript-analyzer has no node_modules (run npm ci); skipping");
        return None;
    }
    Some(TypeScriptResolver::new(AnalyzerCommand::node(
        node,
        package.join("src/main.ts"),
    )))
}

pub fn write(root: &Path, path: &str, content: &str) {
    let full = root.join(path);
    fs::create_dir_all(full.parent().unwrap()).unwrap();
    fs::write(full, content).unwrap();
}

/// Inventory entries for `paths`. The adapter does not check hashes (the use case does), so
/// a placeholder is enough.
pub fn sources(paths: &[&str]) -> Vec<SourceFile> {
    paths
        .iter()
        .map(|p| {
            let path = RepoPath::new(*p).unwrap();
            SourceFile {
                language: Language::detect(&path).unwrap_or(Language::TypeScript),
                path,
                size_bytes: 0,
                content_hash: ContentHash::from_bytes([0; 32]),
            }
        })
        .collect()
}
