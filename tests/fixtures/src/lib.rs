//! FND-04 test support: the snapshot-port behaviour scenarios and a reference implementation.
//!
//! - [`SnapshotPorts`] is the behaviour the scenarios in `contracts/fixtures/ports` describe,
//!   written as a trait so any store (in-memory, SQLite, PostgreSQL) can be run against them.
//! - [`ReferenceStore`] is a deliberately simple in-memory implementation of those rules. It
//!   proves the scenarios are consistent with each other and with the written rules in
//!   `contracts/fixtures/ports/README.md`.
//! - [`run_scenario`] runs one scenario file against any [`SnapshotPorts`] implementation.
//!
//! The real `SnapshotStore` and `GraphReader` ports belong to `codeatlas-application` and arrive
//! with LOC-01; this crate is test tooling and no product crate may depend on it (ADR-0010).

use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::fs;
use std::path::{Path, PathBuf};

use codeatlas_protocol::analysis::{
    Edge, EdgeKind, NodeKind, Snapshot, SnapshotInfo, SnapshotStatus,
};
use codeatlas_protocol::jsonrpc::{GraphQuery, GraphQueryKind, GraphQueryResult};
use serde_json::Value;

/// Default `GraphQuery.depth` and `GraphQuery.limit` (canonical schema).
pub const DEFAULT_DEPTH: u8 = 1;
pub const DEFAULT_LIMIT: u32 = 200;

/// Store errors, spelled as in the scenario files.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StoreError {
    AlreadyExists,
    InvalidStatus,
    NotFound,
}

impl StoreError {
    pub fn as_str(self) -> &'static str {
        match self {
            StoreError::AlreadyExists => "already_exists",
            StoreError::InvalidStatus => "invalid_status",
            StoreError::NotFound => "not_found",
        }
    }
}

/// Graph query errors, spelled as in the scenario files.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum QueryError {
    SnapshotNotFound,
    SnapshotNotComplete,
    NodeNotFound,
}

impl QueryError {
    pub fn as_str(self) -> &'static str {
        match self {
            QueryError::SnapshotNotFound => "snapshot_not_found",
            QueryError::SnapshotNotComplete => "snapshot_not_complete",
            QueryError::NodeNotFound => "node_not_found",
        }
    }
}

/// The SnapshotStore + GraphReader behaviour exercised by the scenarios.
pub trait SnapshotPorts {
    /// Store a finished snapshot. Snapshots are immutable: an existing id is never replaced.
    fn save(&mut self, snapshot: Snapshot) -> Result<(), StoreError>;
    fn get(&self, snapshot_id: &str) -> Result<Snapshot, StoreError>;
    /// Newest first (`createdAt` descending), ties by `snapshotId` ascending.
    fn list(&self, repository: Option<&str>, limit: Option<usize>) -> Vec<SnapshotInfo>;
    /// Newest snapshot of the repository whose status is `complete`.
    fn latest(&self, repository: &str) -> Result<SnapshotInfo, StoreError>;
    fn delete(&mut self, snapshot_id: &str) -> Result<(), StoreError>;
    fn query(&self, snapshot_id: &str, query: &GraphQuery) -> Result<GraphQueryResult, QueryError>;
}

/// In-memory reference implementation of the rules in `contracts/fixtures/ports/README.md`.
#[derive(Default)]
pub struct ReferenceStore {
    snapshots: BTreeMap<String, Snapshot>,
}

fn created_at(info: &SnapshotInfo) -> &str {
    // Scenario data uses one UTC format, so text order is time order. A real adapter compares
    // instants; this guard stops the reference model from silently mis-ordering other formats.
    let t = info.created_at.as_str();
    let b = t.as_bytes();
    assert!(
        b.len() == 20 && b[4] == b'-' && b[10] == b'T' && b[19] == b'Z',
        "reference store expects createdAt like 2026-11-21T10:00:00Z, got {t}"
    );
    t
}

impl SnapshotPorts for ReferenceStore {
    fn save(&mut self, snapshot: Snapshot) -> Result<(), StoreError> {
        if snapshot.info.status == SnapshotStatus::InProgress {
            return Err(StoreError::InvalidStatus);
        }
        if self.snapshots.contains_key(&snapshot.info.snapshot_id) {
            return Err(StoreError::AlreadyExists);
        }
        self.snapshots
            .insert(snapshot.info.snapshot_id.clone(), snapshot);
        Ok(())
    }

    fn get(&self, snapshot_id: &str) -> Result<Snapshot, StoreError> {
        self.snapshots
            .get(snapshot_id)
            .cloned()
            .ok_or(StoreError::NotFound)
    }

    fn list(&self, repository: Option<&str>, limit: Option<usize>) -> Vec<SnapshotInfo> {
        let mut infos: Vec<SnapshotInfo> = self
            .snapshots
            .values()
            .map(|s| s.info.clone())
            .filter(|i| repository.is_none_or(|r| i.repository.name == r))
            .collect();
        infos.sort_by(|a, b| {
            created_at(b)
                .cmp(created_at(a))
                .then_with(|| a.snapshot_id.cmp(&b.snapshot_id))
        });
        if let Some(limit) = limit {
            infos.truncate(limit);
        }
        infos
    }

    fn latest(&self, repository: &str) -> Result<SnapshotInfo, StoreError> {
        self.list(Some(repository), None)
            .into_iter()
            .find(|i| i.status == SnapshotStatus::Complete)
            .ok_or(StoreError::NotFound)
    }

    fn delete(&mut self, snapshot_id: &str) -> Result<(), StoreError> {
        self.snapshots
            .remove(snapshot_id)
            .map(|_| ())
            .ok_or(StoreError::NotFound)
    }

    fn query(&self, snapshot_id: &str, query: &GraphQuery) -> Result<GraphQueryResult, QueryError> {
        let snapshot = self
            .snapshots
            .get(snapshot_id)
            .ok_or(QueryError::SnapshotNotFound)?;
        if snapshot.info.status != SnapshotStatus::Complete {
            return Err(QueryError::SnapshotNotComplete);
        }
        let limit = query.limit.unwrap_or(DEFAULT_LIMIT) as usize;

        if query.kind == GraphQueryKind::FindSymbols {
            let text = query.text.as_deref().unwrap_or_default().to_lowercase();
            let mut hits: Vec<_> = snapshot
                .nodes
                .iter()
                .filter(|n| !matches!(n.kind, NodeKind::File | NodeKind::ExternalModule))
                .filter(|n| n.name.to_lowercase().contains(&text))
                .cloned()
                .collect();
            hits.sort_by(|a, b| a.id.cmp(&b.id));
            let truncated = hits.len() > limit;
            hits.truncate(limit);
            return Ok(GraphQueryResult {
                nodes: hits,
                edges: Vec::new(),
                truncated,
            });
        }

        let start = query.node_id.as_deref().unwrap_or_default();
        if !snapshot.nodes.iter().any(|n| n.id == start) {
            return Err(QueryError::NodeNotFound);
        }
        let (outgoing, incoming, kinds): (bool, bool, Option<Vec<EdgeKind>>) = match query.kind {
            GraphQueryKind::Callers => (false, true, Some(vec![EdgeKind::Calls])),
            GraphQueryKind::Callees => (true, false, Some(vec![EdgeKind::Calls])),
            GraphQueryKind::Dependencies => (true, false, Some(vec![EdgeKind::Imports])),
            GraphQueryKind::Dependents => (false, true, Some(vec![EdgeKind::Imports])),
            GraphQueryKind::Neighbors => (true, true, query.edge_kinds.clone()),
            GraphQueryKind::FindSymbols => unreachable!(),
        };
        let depth = query.depth.unwrap_or(DEFAULT_DEPTH);

        // Breadth-first: expand every node closer than `depth` and collect the matching edges
        // anchored on it. The far end of a resolved edge is a reached node.
        let mut distance: BTreeMap<&str, u8> = BTreeMap::from([(start, 0)]);
        let mut queue = VecDeque::from([start]);
        let mut collected: BTreeMap<&str, &Edge> = BTreeMap::new();
        while let Some(anchor) = queue.pop_front() {
            let d = distance[anchor];
            if d >= depth {
                continue;
            }
            for edge in &snapshot.edges {
                if kinds.as_ref().is_some_and(|k| !k.contains(&edge.kind)) {
                    continue;
                }
                let far = if outgoing && edge.source == anchor {
                    Some(edge.target.as_deref())
                } else if incoming && edge.target.as_deref() == Some(anchor) {
                    Some(Some(edge.source.as_str()))
                } else {
                    None
                };
                let Some(far) = far else { continue };
                collected.insert(edge.id.as_str(), edge);
                if let Some(far) = far
                    && !distance.contains_key(far)
                {
                    distance.insert(far, d + 1);
                    queue.push_back(far);
                }
            }
        }

        let reached: Vec<&str> = distance.keys().copied().filter(|id| *id != start).collect();
        let truncated = reached.len() > limit;
        let kept: BTreeSet<&str> = reached.into_iter().take(limit).collect();
        let allowed = |id: &str| id == start || kept.contains(id);
        let nodes = snapshot
            .nodes
            .iter()
            .filter(|n| kept.contains(n.id.as_str()))
            .cloned();
        let mut nodes: Vec<_> = nodes.collect();
        nodes.sort_by(|a, b| a.id.cmp(&b.id));
        let edges = collected
            .into_values()
            .filter(|e| allowed(&e.source) && e.target.as_deref().is_none_or(allowed))
            .cloned()
            .collect();
        Ok(GraphQueryResult {
            nodes,
            edges,
            truncated,
        })
    }
}

/// Root of the engine repository.
pub fn engine_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

/// `contracts/fixtures/ports`.
pub fn ports_dir() -> PathBuf {
    engine_root().join("contracts/fixtures/ports")
}

pub fn read_json(path: &Path) -> Value {
    let text = fs::read_to_string(path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
    serde_json::from_str(&text).unwrap_or_else(|e| panic!("{}: {e}", path.display()))
}

/// All `*.json` files in a folder, sorted by name.
pub fn json_files(dir: &Path) -> Vec<PathBuf> {
    let mut files: Vec<PathBuf> = fs::read_dir(dir)
        .unwrap_or_else(|e| panic!("{}: {e}", dir.display()))
        .map(|e| e.expect("directory entry").path())
        .filter(|p| p.extension().is_some_and(|x| x == "json"))
        .collect();
    files.sort();
    files
}

/// Every seed in `contracts/fixtures/ports/seeds`, by name.
pub fn load_seeds() -> BTreeMap<String, Snapshot> {
    json_files(&ports_dir().join("seeds"))
        .into_iter()
        .map(|path| {
            let name = path
                .file_stem()
                .expect("file name")
                .to_string_lossy()
                .into_owned();
            let snapshot: Snapshot = serde_json::from_value(read_json(&path))
                .unwrap_or_else(|e| panic!("seed {name} is not a Snapshot: {e}"));
            (name, snapshot)
        })
        .collect()
}

/// Snapshot as JSON with nodes, edges and diagnostics in a canonical order. Stores need not
/// keep insertion order for these collections (README rule 2).
pub fn canonical(snapshot: &Snapshot) -> Value {
    let mut value = serde_json::to_value(snapshot).expect("serialize snapshot");
    for key in ["nodes", "edges", "diagnostics"] {
        if let Some(Value::Array(items)) = value.get_mut(key) {
            items.sort_by_key(|v| v.to_string());
        }
    }
    value
}

/// Runs one scenario against a fresh port. Returns the first mismatch, naming the step.
pub fn run_scenario(
    port: &mut dyn SnapshotPorts,
    scenario: &Value,
    seeds: &BTreeMap<String, Snapshot>,
) -> Result<(), String> {
    let seed = |name: &str| {
        seeds
            .get(name)
            .cloned()
            .ok_or_else(|| format!("unknown seed {name}"))
    };
    let s = |v: &Value, key: &str| -> Result<String, String> {
        v.get(key)
            .and_then(Value::as_str)
            .map(str::to_string)
            .ok_or_else(|| format!("missing string '{key}'"))
    };

    for name in scenario["given"].as_array().into_iter().flatten() {
        let name = name.as_str().unwrap_or_default();
        port.save(seed(name)?)
            .map_err(|e| format!("given seed {name}: save failed with {}", e.as_str()))?;
    }

    for (i, step) in scenario["steps"]
        .as_array()
        .into_iter()
        .flatten()
        .enumerate()
    {
        let n = i + 1;
        let op = s(step, "op")?;
        let expect = &step["expect"];
        let expected_error = expect.get("error").and_then(Value::as_str);
        let fail = |got: String| Err(format!("step {n} ({op}): expected {expect}, got {got}"));

        match op.as_str() {
            "save" => {
                let got = port.save(seed(&s(step, "seed")?)?);
                match (got, expected_error) {
                    (Ok(()), None) => {}
                    (Err(e), Some(want)) if e.as_str() == want => {}
                    (got, _) => return fail(format!("{got:?}")),
                }
            }
            "get" => match (port.get(&s(step, "snapshotId")?), expected_error) {
                (Ok(got), None) => {
                    let want = seed(&s(expect, "snapshot")?)?;
                    if canonical(&got) != canonical(&want) {
                        return fail(format!("a different snapshot: {}", canonical(&got)));
                    }
                }
                (Err(e), Some(want)) if e.as_str() == want => {}
                (got, _) => return fail(format!("{:?}", got.map(|s| s.info.snapshot_id))),
            },
            "list" => {
                let params = &step["params"];
                let repository = params.get("repository").and_then(Value::as_str);
                let limit = params
                    .get("limit")
                    .and_then(Value::as_u64)
                    .map(|l| l as usize);
                let got = port.list(repository, limit);
                let ids: Vec<&str> = got.iter().map(|i| i.snapshot_id.as_str()).collect();
                let want: Vec<&str> = expect["snapshotIds"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .filter_map(Value::as_str)
                    .collect();
                if ids != want {
                    return fail(format!("{ids:?}"));
                }
                for info in &got {
                    let saved = seeds.values().find(|s| s.info == *info);
                    if saved.is_none() {
                        return fail(format!(
                            "an info that matches no seed: {}",
                            info.snapshot_id
                        ));
                    }
                }
            }
            "latest" => match (port.latest(&s(step, "repository")?), expected_error) {
                (Ok(info), None) if info.snapshot_id == s(expect, "snapshotId")? => {}
                (Err(e), Some(want)) if e.as_str() == want => {}
                (got, _) => return fail(format!("{:?}", got.map(|i| i.snapshot_id))),
            },
            "delete" => match (port.delete(&s(step, "snapshotId")?), expected_error) {
                (Ok(()), None) => {}
                (Err(e), Some(want)) if e.as_str() == want => {}
                (got, _) => return fail(format!("{got:?}")),
            },
            "query" => {
                let query: GraphQuery = serde_json::from_value(step["query"].clone())
                    .map_err(|e| format!("step {n}: query is not a GraphQuery: {e}"))?;
                match (port.query(&s(step, "snapshotId")?, &query), expected_error) {
                    (Ok(got), None) => {
                        let got = serde_json::to_value(&got).expect("serialize result");
                        if got != expect["result"] {
                            return fail(got.to_string());
                        }
                    }
                    (Err(e), Some(want)) if e.as_str() == want => {}
                    (Ok(got), _) => return fail(serde_json::to_string(&got).unwrap_or_default()),
                    (Err(e), _) => return fail(e.as_str().to_string()),
                }
            }
            other => return Err(format!("step {n}: unknown op '{other}'")),
        }
    }
    Ok(())
}
