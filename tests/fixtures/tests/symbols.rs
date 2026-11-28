//! ENG-03 end to end: scan each FND-04 fixture, parse every file, and compare the extracted
//! symbols with the `symbols` list in its `expected.json`: the same ids, kinds, names, files,
//! name lines, parents and export flags, and nothing more or less.

use std::fs;
use std::path::{Path, PathBuf};

use codeatlas_application::{ParseLimits, ParseSources, RunContext, ScanOptions, ScanRepository};
use codeatlas_parser_treesitter::TreeSitterParser;
use codeatlas_scanner_fs::{FsSourceReader, FsSourceScanner};
use serde_json::{Value, json};

const NOT_FIXTURES: &[&str] = &["src", "tests", "target"];

fn fixtures() -> Vec<(String, Value, PathBuf)> {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    let mut found: Vec<(String, Value, PathBuf)> = fs::read_dir(dir)
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
            (name, expected, root)
        })
        .collect();
    found.sort_by(|a, b| a.0.cmp(&b.0));
    assert!(!found.is_empty(), "no fixtures");
    found
}

/// One symbol in the golden file's shape, with the line one-based.
fn golden_line(symbol: &codeatlas_core::Symbol) -> Value {
    let mut entry = json!({
        "id": symbol.id().as_str(),
        "kind": symbol.kind().as_str(),
        "name": symbol.name(),
        "path": symbol.path().as_str(),
        "line": symbol.primary().name_range.start().line + 1,
        "exported": symbol.exported(),
    });
    if let Some(parent) = symbol.parent() {
        entry["parent"] = json!(parent.as_str());
    }
    entry
}

fn sorted(mut entries: Vec<Value>) -> Vec<Value> {
    entries.sort_by_key(|e| e["id"].as_str().unwrap().to_string());
    entries
}

#[test]
fn every_fixture_symbol_list_is_reproduced_exactly() {
    let mut compared = 0;
    for (name, expected, root) in fixtures() {
        let inventory = ScanRepository::new(&FsSourceScanner)
            .execute(
                root.clone(),
                &ScanOptions::default(),
                &RunContext::detached(),
            )
            .unwrap();
        let report = ParseSources::new(&FsSourceReader, &TreeSitterParser)
            .execute(
                &root,
                &inventory,
                &ParseLimits::default(),
                &RunContext::detached(),
            )
            .unwrap();
        assert!(report.failed.is_empty(), "[{name}] {:?}", report.failed);
        let got = sorted(
            report
                .parsed
                .iter()
                .flat_map(|p| p.symbols())
                .map(golden_line)
                .collect(),
        );
        let want = sorted(expected["symbols"].as_array().unwrap().clone());
        let missing: Vec<&Value> = want.iter().filter(|w| !got.contains(w)).collect();
        let extra: Vec<&Value> = got.iter().filter(|g| !want.contains(g)).collect();
        assert!(
            missing.is_empty() && extra.is_empty(),
            "[{name}]\n  missing: {missing:#?}\n  extra: {extra:#?}"
        );
        for parsed in &report.parsed {
            assert!(
                parsed.symbols_truncated().is_none(),
                "[{name}] {}",
                parsed.path()
            );
        }
        compared += got.len();
    }
    assert!(
        compared >= 90,
        "expected the fixture symbols, compared {compared}"
    );
}
