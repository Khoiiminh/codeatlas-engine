//! ENG-02 end to end: scan a repository with `FsSourceScanner`, read each file with
//! `FsSourceReader` and parse it with `TreeSitterParser`, through the `ParseSources` use case.
//!
//! Every FND-04 fixture is valid code (the TypeScript compiler reports no syntax errors for it),
//! so every file must parse cleanly. A repository with broken files must still produce a full
//! report: syntax errors are diagnostics, not failures.

use std::fs;
use std::path::{Path, PathBuf};

use std::num::NonZeroUsize;

use codeatlas_application::{
    ParseLimits, ParseReport, ParseSources, RunContext, ScanOptions, ScanRepository,
};
use codeatlas_core::Inventory;
use codeatlas_parser_treesitter::TreeSitterParser;
use codeatlas_scanner_fs::{FsSourceReader, FsSourceScanner};
use serde_json::Value;

const NOT_FIXTURES: &[&str] = &["src", "tests", "target"];

/// Several threads, so the end-to-end checks also cover parallel parsing (ADR-0014).
fn run() -> RunContext<'static> {
    RunContext::detached().with_workers(NonZeroUsize::new(4).unwrap())
}

fn scan_and_parse(root: &Path) -> (Inventory, ParseReport) {
    let inventory = ScanRepository::new(&FsSourceScanner)
        .execute(root.to_path_buf(), &ScanOptions::default(), &run())
        .unwrap_or_else(|e| panic!("{}: {e}", root.display()));
    let report = ParseSources::new(&FsSourceReader, &TreeSitterParser)
        .execute(root, &inventory, &ParseLimits::default(), &run())
        .unwrap();
    (inventory, report)
}

fn fixture_roots() -> Vec<(String, PathBuf)> {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    let mut roots: Vec<(String, PathBuf)> = fs::read_dir(dir)
        .unwrap()
        .map(|e| e.unwrap())
        .filter(|e| e.path().is_dir())
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .filter(|n| !NOT_FIXTURES.contains(&n.as_str()))
        .map(|name| {
            let expected: Value = serde_json::from_str(
                &fs::read_to_string(dir.join(&name).join("expected.json")).unwrap(),
            )
            .unwrap();
            let root = dir.join(&name).join(expected["root"].as_str().unwrap());
            (name, root)
        })
        .collect();
    roots.sort();
    assert!(!roots.is_empty(), "no fixtures");
    roots
}

#[test]
fn every_fixture_file_parses_cleanly() {
    let mut files = 0;
    for (name, root) in fixture_roots() {
        let (inventory, report) = scan_and_parse(&root);
        assert!(
            report.failed.is_empty(),
            "[{name}] failed: {:?}",
            report.failed
        );
        let parsed: Vec<&str> = report.parsed.iter().map(|p| p.path().as_str()).collect();
        let scanned: Vec<&str> = inventory.files().iter().map(|f| f.path.as_str()).collect();
        assert_eq!(
            parsed, scanned,
            "[{name}] every scanned file is parsed, in order"
        );
        for p in &report.parsed {
            assert!(
                p.is_clean(),
                "[{name}] {}: {:?}",
                p.path(),
                p.problems().first()
            );
        }
        assert!(report.diagnostics().is_empty(), "[{name}]");
        files += report.parsed.len();
    }
    assert!(files >= 30, "expected the fixture files, parsed {files}");
}

#[test]
fn broken_files_are_reported_and_the_rest_still_parses() {
    let dir = tempfile::tempdir().unwrap();
    let files = [
        ("src/good.ts", "export const good = 1;\n"),
        ("src/broken.ts", "export function f( {\n  return 1;\n"),
        ("src/view.tsx", "export const V = () => <div>{</div>;\n"),
        ("lib/legacy.js", "var x = ;\n"),
        (
            "lib/ok.jsx",
            "export const A = () => <a href=\"#\">ok</a>;\n",
        ),
        ("node_modules/pkg/broken.js", "this is not javascript ((("),
    ];
    for (path, text) in files {
        let full = dir.path().join(path);
        fs::create_dir_all(full.parent().unwrap()).unwrap();
        fs::write(full, text).unwrap();
    }
    let (_, report) = scan_and_parse(dir.path());
    assert!(report.failed.is_empty(), "{:?}", report.failed);
    let summary: Vec<(&str, bool)> = report
        .parsed
        .iter()
        .map(|p| (p.path().as_str(), p.is_clean()))
        .collect();
    assert_eq!(
        summary,
        [
            ("lib/legacy.js", false),
            ("lib/ok.jsx", true),
            ("src/broken.ts", false),
            ("src/good.ts", true),
            ("src/view.tsx", false),
        ]
    );
    let diagnostics = report.diagnostics();
    assert!(diagnostics.iter().all(|d| d.code.as_str() == "CA1001"));
    let paths: std::collections::BTreeSet<&str> = diagnostics
        .iter()
        .map(|d| d.location.as_ref().unwrap().path.as_str())
        .collect();
    assert_eq!(
        paths.into_iter().collect::<Vec<_>>(),
        ["lib/legacy.js", "src/broken.ts", "src/view.tsx"]
    );
}

#[test]
fn a_file_changed_between_scan_and_parse_is_reported_not_parsed() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(dir.path().join("a.ts"), "export const a = 1;\n").unwrap();
    fs::write(dir.path().join("b.ts"), "export const b = 2;\n").unwrap();
    let inventory = ScanRepository::new(&FsSourceScanner)
        .execute(dir.path().to_path_buf(), &ScanOptions::default(), &run())
        .unwrap();
    fs::write(dir.path().join("a.ts"), "export const a = 9;\n").unwrap();
    let report = ParseSources::new(&FsSourceReader, &TreeSitterParser)
        .execute(dir.path(), &inventory, &ParseLimits::default(), &run())
        .unwrap();
    assert_eq!(report.parsed.len(), 1);
    assert_eq!(report.failed.len(), 1);
    assert_eq!(report.failed[0].path.as_str(), "a.ts");
    let diagnostic = &report.diagnostics()[0];
    assert_eq!(diagnostic.code.as_str(), "CA1002");
    assert_eq!(
        diagnostic.message,
        "File could not be parsed: file changed since it was scanned."
    );
}

#[test]
fn fixture_reports_do_not_depend_on_the_thread_count() {
    for (name, root) in fixture_roots() {
        let inventory = ScanRepository::new(&FsSourceScanner)
            .execute(
                root.clone(),
                &ScanOptions::default(),
                &RunContext::detached(),
            )
            .unwrap();
        let parse = |workers: usize| {
            let run = RunContext::detached().with_workers(NonZeroUsize::new(workers).unwrap());
            ParseSources::new(&FsSourceReader, &TreeSitterParser)
                .execute(&root, &inventory, &ParseLimits::default(), &run)
                .unwrap()
        };
        let one = parse(1);
        for workers in [2, 7, 32] {
            assert_eq!(parse(workers), one, "[{name}] {workers} threads");
        }
    }
}
