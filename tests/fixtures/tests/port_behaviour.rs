//! FND-04: snapshot-port behaviour fixtures (`contracts/fixtures/ports`).
//!
//! 1. Every seed is a valid canonical `Snapshot` (schema, Rust round trip, consistent stats).
//! 2. Every scenario is valid against `scenario.schema.json`, which reuses the canonical
//!    `GraphQuery` and `GraphQueryResult` definitions from the contract bundle.
//! 3. Every scenario passes against the in-memory reference store, so the scenarios agree with
//!    the written rules and with each other.
//! 4. The scenarios cover every store operation, query kind and error code.

use std::collections::BTreeSet;

use codeatlas_fixture_tests::{
    ReferenceStore, engine_root, json_files, load_seeds, ports_dir, read_json, run_scenario,
};
use codeatlas_protocol::analysis::{NodeKind, Resolution, Snapshot};
use jsonschema::{Registry, Validator};
use serde_json::{Value, json};

const BUNDLE_ID: &str = "urn:codeatlas:contracts:0.1.0";

fn bundle() -> Value {
    read_json(&engine_root().join("contracts/schemas/codeatlas.schema.json"))
}

fn errors(validator: &Validator, value: &Value) -> Vec<String> {
    validator
        .iter_errors(value)
        .map(|e| format!("{} at {}", e, e.instance_path()))
        .collect()
}

fn scenario_files() -> Vec<std::path::PathBuf> {
    let mut files = json_files(&ports_dir().join("snapshot-store"));
    files.extend(json_files(&ports_dir().join("graph-reader")));
    files
}

#[test]
fn seeds_are_valid_canonical_snapshots() {
    let mut root = bundle();
    root["$ref"] = json!("#/$defs/Snapshot");
    let validator = jsonschema::options()
        .should_validate_formats(true)
        .build(&root)
        .expect("contract bundle compiles");

    let seeds = load_seeds();
    assert!(
        seeds.len() >= 5,
        "expected the seed set, found {}",
        seeds.len()
    );
    for path in json_files(&ports_dir().join("seeds")) {
        let raw = read_json(&path);
        let name = path.file_name().unwrap().to_string_lossy().into_owned();
        let problems = errors(&validator, &raw);
        assert!(problems.is_empty(), "{name}: {problems:#?}");

        let typed: Snapshot = serde_json::from_value(raw.clone()).expect("deserialize");
        assert_eq!(
            serde_json::to_value(&typed).unwrap(),
            raw,
            "{name}: Rust round trip changed it"
        );

        let stats = &typed.info.stats;
        let files = typed
            .nodes
            .iter()
            .filter(|n| n.kind == NodeKind::File)
            .count();
        let unresolved = typed
            .edges
            .iter()
            .filter(|e| e.resolution == Resolution::Unresolved)
            .count();
        assert_eq!(
            (
                stats.files,
                stats.nodes,
                stats.edges,
                stats.unresolved_edges,
                stats.diagnostics
            ),
            (
                files as u64,
                typed.nodes.len() as u64,
                typed.edges.len() as u64,
                unresolved as u64,
                typed.diagnostics.len() as u64
            ),
            "{name}: stats do not match the content"
        );

        let ids: BTreeSet<&str> = typed.nodes.iter().map(|n| n.id.as_str()).collect();
        assert_eq!(ids.len(), typed.nodes.len(), "{name}: duplicate node id");
        let edge_ids: BTreeSet<&str> = typed.edges.iter().map(|e| e.id.as_str()).collect();
        assert_eq!(
            edge_ids.len(),
            typed.edges.len(),
            "{name}: duplicate edge id"
        );
        for edge in &typed.edges {
            assert!(
                ids.contains(edge.source.as_str()),
                "{name}: {} has an unknown source",
                edge.id
            );
            if let Some(target) = &edge.target {
                assert!(
                    ids.contains(target.as_str()),
                    "{name}: {} has an unknown target",
                    edge.id
                );
            }
        }
    }
}

#[test]
fn scenarios_match_their_schema() {
    let registry = Registry::new()
        .add(BUNDLE_ID, bundle())
        .expect("register contract bundle")
        .prepare()
        .expect("prepare registry");
    let schema = read_json(&ports_dir().join("scenario.schema.json"));
    let validator = jsonschema::options()
        .with_registry(&registry)
        .should_validate_formats(true)
        .build(&schema)
        .expect("scenario schema compiles");

    let seeds = load_seeds();
    for path in scenario_files() {
        let scenario = read_json(&path);
        let name = path.display().to_string();
        let problems = errors(&validator, &scenario);
        assert!(problems.is_empty(), "{name}: {problems:#?}");

        let folder = path
            .parent()
            .unwrap()
            .file_name()
            .unwrap()
            .to_string_lossy();
        let port = scenario["port"].as_str().unwrap();
        let expected_port = if folder == "snapshot-store" {
            "SnapshotStore"
        } else {
            "GraphReader"
        };
        assert_eq!(
            port, expected_port,
            "{name}: port does not match its folder"
        );

        let mut used: Vec<&str> = scenario["given"]
            .as_array()
            .unwrap()
            .iter()
            .filter_map(Value::as_str)
            .collect();
        used.extend(
            scenario["steps"]
                .as_array()
                .unwrap()
                .iter()
                .filter_map(|s| {
                    s.get("seed")
                        .or_else(|| s["expect"].get("snapshot"))
                        .and_then(Value::as_str)
                }),
        );
        for seed in used {
            assert!(seeds.contains_key(seed), "{name}: unknown seed {seed}");
        }
    }
}

#[test]
fn reference_store_passes_every_scenario() {
    let seeds = load_seeds();
    let files = scenario_files();
    assert!(
        files.len() >= 10,
        "expected the scenario set, found {}",
        files.len()
    );
    let mut failures = Vec::new();
    for path in files {
        let mut store = ReferenceStore::default();
        if let Err(problem) = run_scenario(&mut store, &read_json(&path), &seeds) {
            failures.push(format!("{}: {problem}", path.display()));
        }
    }
    assert!(failures.is_empty(), "{failures:#?}");
}

#[test]
fn scenarios_cover_every_operation_query_kind_and_error() {
    let mut ops = BTreeSet::new();
    let mut kinds = BTreeSet::new();
    let mut errs = BTreeSet::new();
    let mut truncated = false;
    for path in scenario_files() {
        for step in read_json(&path)["steps"].as_array().unwrap() {
            ops.insert(step["op"].as_str().unwrap().to_string());
            if let Some(kind) = step.pointer("/query/kind").and_then(Value::as_str) {
                kinds.insert(kind.to_string());
            }
            if let Some(e) = step["expect"].get("error").and_then(Value::as_str) {
                errs.insert(e.to_string());
            }
            truncated |= step.pointer("/expect/result/truncated") == Some(&json!(true));
        }
    }
    let set = |items: &[&str]| items.iter().map(|s| s.to_string()).collect::<BTreeSet<_>>();
    assert_eq!(
        ops,
        set(&["delete", "get", "latest", "list", "query", "save"])
    );
    assert_eq!(
        kinds,
        set(&[
            "callees",
            "callers",
            "dependencies",
            "dependents",
            "find_symbols",
            "neighbors"
        ])
    );
    assert_eq!(
        errs,
        set(&[
            "already_exists",
            "invalid_status",
            "node_not_found",
            "not_found",
            "snapshot_not_complete",
            "snapshot_not_found"
        ])
    );
    assert!(truncated, "no scenario exercises truncation");
}

/// A store that breaks one rule must fail at least one scenario; otherwise the scenarios would
/// not be testing that rule.
#[test]
fn a_store_that_breaks_immutability_is_caught() {
    use codeatlas_fixture_tests::{QueryError, SnapshotPorts, StoreError};
    use codeatlas_protocol::analysis::SnapshotInfo;
    use codeatlas_protocol::jsonrpc::{GraphQuery, GraphQueryResult};

    /// Replaces snapshots on save instead of rejecting duplicates.
    #[derive(Default)]
    struct Overwrites(ReferenceStore);
    impl SnapshotPorts for Overwrites {
        fn save(&mut self, snapshot: Snapshot) -> Result<(), StoreError> {
            let _ = self.0.delete(&snapshot.info.snapshot_id);
            self.0.save(snapshot)
        }
        fn get(&self, id: &str) -> Result<Snapshot, StoreError> {
            self.0.get(id)
        }
        fn list(&self, repository: Option<&str>, limit: Option<usize>) -> Vec<SnapshotInfo> {
            self.0.list(repository, limit)
        }
        fn latest(&self, repository: &str) -> Result<SnapshotInfo, StoreError> {
            self.0.latest(repository)
        }
        fn delete(&mut self, id: &str) -> Result<(), StoreError> {
            self.0.delete(id)
        }
        fn query(&self, id: &str, q: &GraphQuery) -> Result<GraphQueryResult, QueryError> {
            self.0.query(id, q)
        }
    }

    let seeds = load_seeds();
    let caught = scenario_files()
        .iter()
        .any(|path| run_scenario(&mut Overwrites::default(), &read_json(path), &seeds).is_err());
    assert!(
        caught,
        "no scenario detects a store that overwrites snapshots"
    );
}
