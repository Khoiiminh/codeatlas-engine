//! Shared by the ENG-04 tests: the analyzer of this checkout, and one full analysis.

#![allow(dead_code)]

use std::env;
use std::num::NonZeroUsize;
use std::path::{Path, PathBuf};
use std::process::Command;

use codeatlas_application::{
    ParseLimits, ParseSources, ResolveSemantics, RunContext, ScanOptions, ScanRepository,
    SemanticReport,
};
use codeatlas_parser_treesitter::TreeSitterParser;
use codeatlas_scanner_fs::{FsSourceReader, FsSourceScanner};
use codeatlas_semantic_typescript::{AnalyzerCommand, TypeScriptResolver};

pub fn engine_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

/// The analyzer of this checkout, or `None` when it cannot run here. CI must be able to run it
/// (ADR-0014 decision 7), so there a missing Node or `npm ci` fails the test instead.
pub fn analyzer() -> Option<TypeScriptResolver> {
    let node = env::var_os("CODEATLAS_NODE").map_or_else(|| PathBuf::from("node"), PathBuf::from);
    let package = engine_root().join("packages/typescript-analyzer");
    let runs = Command::new(&node)
        .arg("--version")
        .output()
        .is_ok_and(|o| o.status.success());
    let installed = package
        .join("node_modules/typescript/package.json")
        .is_file();
    if !(runs && installed) {
        assert!(
            env::var_os("CI").is_none(),
            "CI must run the TypeScript analyzer: install Node and run npm ci in packages/typescript-analyzer"
        );
        eprintln!(
            "the TypeScript analyzer cannot run here (Node, or npm ci in packages/typescript-analyzer); skipping"
        );
        return None;
    }
    Some(TypeScriptResolver::new(AnalyzerCommand::node(
        node,
        package.join("src/main.ts"),
    )))
}

/// Scan, parse and resolve `root`, on four threads.
pub fn analyse(root: &Path, resolver: &TypeScriptResolver) -> SemanticReport {
    let run = RunContext::detached().with_workers(NonZeroUsize::new(4).unwrap());
    let inventory = ScanRepository::new(&FsSourceScanner)
        .execute(root.to_path_buf(), &ScanOptions::default(), &run)
        .unwrap();
    let parsed = ParseSources::new(&FsSourceReader, &TreeSitterParser)
        .execute(root, &inventory, &ParseLimits::default(), &run)
        .unwrap();
    assert!(parsed.failed.is_empty(), "{:?}", parsed.failed);
    ResolveSemantics::new(resolver)
        .execute(root, &inventory, &parsed, &run)
        .unwrap()
}
