//! ENG-01 against the FND-04 golden fixtures: scanning each `tests/fixtures/<name>/repo` with
//! the default options must reproduce its `expected.json` exactly. The file list (path and
//! language) must match, and the reported skips must be exactly the folders in `ignoredPaths`.
//! New fixtures are picked up automatically.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use codeatlas_application::{RunContext, ScanOptions, ScanRepository};
use codeatlas_core::{Inventory, SkipReason};
use codeatlas_scanner_fs::FsSourceScanner;
use serde_json::Value;

const NOT_FIXTURES: &[&str] = &["src", "tests", "target"];

fn engine_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../..")
}

fn read_json(path: &Path) -> Value {
    let text = fs::read_to_string(path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
    serde_json::from_str(&text).unwrap_or_else(|e| panic!("{}: {e}", path.display()))
}

/// (name, expected.json, scanned inventory) for every fixture.
fn scanned_fixtures() -> Vec<(String, Value, Inventory)> {
    let dir = engine_root().join("tests/fixtures");
    let mut names: Vec<String> = fs::read_dir(&dir)
        .unwrap()
        .map(|e| e.unwrap())
        .filter(|e| e.path().is_dir())
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .filter(|n| !NOT_FIXTURES.contains(&n.as_str()))
        .collect();
    names.sort();
    assert!(!names.is_empty(), "no fixtures under tests/fixtures");
    names
        .into_iter()
        .map(|name| {
            let expected = read_json(&dir.join(&name).join("expected.json"));
            let root = dir.join(&name).join(expected["root"].as_str().unwrap());
            let inventory = ScanRepository::new(&FsSourceScanner)
                .execute(root, &ScanOptions::default(), &RunContext::detached())
                .unwrap_or_else(|e| panic!("[{name}] scan failed: {e}"));
            (name, expected, inventory)
        })
        .collect()
}

#[test]
fn scanner_reproduces_every_fixture_inventory() {
    for (name, expected, inventory) in scanned_fixtures() {
        let want: Vec<(String, String)> = expected["files"]
            .as_array()
            .unwrap()
            .iter()
            .map(|f| {
                (
                    f["path"].as_str().unwrap().to_string(),
                    f["language"].as_str().unwrap().to_string(),
                )
            })
            .collect();
        let got: Vec<(String, String)> = inventory
            .files()
            .iter()
            .map(|f| (f.path.to_string(), f.language.as_str().to_string()))
            .collect();
        assert_eq!(got, want, "[{name}] files differ from expected.json");

        let mut want_skipped: Vec<String> = expected["ignoredPaths"]
            .as_array()
            .unwrap()
            .iter()
            .map(|p| p.as_str().unwrap().trim_end_matches('/').to_string())
            .collect();
        want_skipped.sort(); // ignoredPaths is a set; the inventory is sorted by path
        let got_skipped: Vec<String> = inventory
            .skipped()
            .iter()
            .map(|s| {
                assert_eq!(
                    s.reason,
                    SkipReason::IgnoredDirectory,
                    "[{name}] unexpected skip of {}",
                    s.path
                );
                s.path.to_string()
            })
            .collect();
        assert_eq!(
            got_skipped, want_skipped,
            "[{name}] skipped folders differ from ignoredPaths"
        );
    }
}

/// The FND-04 seed snapshots carry SHA-256 content hashes computed by a separate (Python)
/// implementation from the SynchroStream fixture. The scanner's hashes must agree.
#[test]
fn content_hashes_agree_with_the_fnd04_seed_snapshots() {
    let fixtures = scanned_fixtures();
    let (_, _, synchrostream) = fixtures
        .iter()
        .find(|(name, _, _)| name == "synchrostream")
        .expect("synchrostream fixture");
    let scanned: BTreeMap<String, String> = synchrostream
        .files()
        .iter()
        .map(|f| (f.path.to_string(), f.content_hash.to_hex()))
        .collect();

    let seeds = engine_root().join("contracts/fixtures/ports/seeds");
    let mut checked = 0;
    for entry in fs::read_dir(&seeds).unwrap() {
        let seed = read_json(&entry.unwrap().path());
        for node in seed["nodes"].as_array().unwrap() {
            if node["kind"] != "file" {
                continue;
            }
            let path = node["location"]["path"].as_str().unwrap();
            let hash = node["contentHash"].as_str().unwrap();
            assert_eq!(scanned.get(path).map(String::as_str), Some(hash), "{path}");
            checked += 1;
        }
    }
    assert!(
        checked >= 5,
        "expected seed file nodes to compare, found {checked}"
    );
}

#[test]
fn scanning_twice_gives_the_same_inventory() {
    let first = scanned_fixtures();
    let second = scanned_fixtures();
    for ((name, _, a), (_, _, b)) in first.iter().zip(&second) {
        assert_eq!(a, b, "[{name}] inventory is not deterministic");
    }
}
