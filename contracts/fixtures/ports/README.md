# Snapshot-port behaviour fixtures

Language-neutral scenarios for the `SnapshotStore` and `GraphReader` ports (FND-04, ADR-0006). The engine's SQLite adapter (LOC-01) and the platform's PostgreSQL adapter (HOST-02) must both pass every scenario. That is how two separately built stores stay behaviourally equivalent without sharing a database schema.

| Path | Contents |
|---|---|
| `seeds/<name>.json` | Canonical `Snapshot` documents (contract 0.1.0). Ids, paths, positions and content hashes come from the SynchroStream source fixture (`tests/fixtures/synchrostream`). |
| `snapshot-store/*.json` | Save, get, list, latest and delete scenarios. |
| `graph-reader/*.json` | Graph query scenarios: canonical `GraphQuery` in, canonical `GraphQueryResult` out. |
| `scenario.schema.json` | Schema for a scenario file. It references the contract bundle `urn:codeatlas:contracts:0.1.0` for `SnapshotId`, `GraphQuery` and `GraphQueryResult`. |

## How to run a scenario

1. Start from an empty store.
2. Save every seed named in `given`. Each save must succeed.
3. Run `steps` in order. For each step, call the operation in `op` and compare the outcome with `expect`:
   - `{"ok": true}`: the operation succeeded;
   - `{"error": "<code>"}`: the operation failed with exactly that error;
   - otherwise, the value to compare (below).

The reference runner is `run_scenario` in `tests/fixtures/src/lib.rs`. An in-memory reference store passes every scenario (`cargo test -p codeatlas-fixture-tests`).

## Rules

**SnapshotStore**
1. `save(snapshot)` stores a finished snapshot. A snapshot whose status is `in_progress` is rejected with `invalid_status`; failed analyses (`failed`) are stored. A snapshot is immutable: saving an id that already exists fails with `already_exists` and changes nothing.
2. `get(snapshotId)` returns exactly what was saved, or `not_found`. Expectation `{"snapshot": "<seed>"}` means equal to that seed. The order of `nodes`, `edges` and `diagnostics` may differ, so compare them as collections; everything else must match exactly, and optional fields stay absent (never `null`).
3. `list({repository?, limit?})` returns `SnapshotInfo` values (each equal to the saved one), filtered by `repository.name`, newest `createdAt` first, ties broken by `snapshotId` ascending, at most `limit`. Compare `createdAt` as instants, not as text. Failed snapshots are listed too.
4. `latest(repository)` returns the newest snapshot of that repository whose status is `complete`, or `not_found`. A failed analysis never replaces the last complete snapshot.
5. `delete(snapshotId)` removes that snapshot only, or fails with `not_found`.

**GraphReader** — `query(snapshotId, GraphQuery)`
6. Errors are checked in this order: `snapshot_not_found` (unknown id); `snapshot_not_complete` (status is not `complete`); `node_not_found` (the start `nodeId` is not in the snapshot; this check does not apply to `find_symbols`).
7. `find_symbols`: nodes whose `name` contains `text`, ignoring case, excluding `file` and `external_module` nodes; sorted by `id`; at most `limit`; `edges` is empty.
8. Traversals start at `nodeId`, with defaults `depth` 1 and `limit` 200:

   | Kind | Follows | Direction |
   |---|---|---|
   | `callers` | `calls` | incoming |
   | `callees` | `calls` | outgoing |
   | `dependencies` | `imports` | outgoing |
   | `dependents` | `imports` | incoming |
   | `neighbors` | `edgeKinds`, or every kind when absent | both |

   `edgeKinds` applies to `neighbors` only.
9. Breadth-first: every node closer than `depth` hops is expanded. The result's edges are all matching edges anchored on an expanded node (the source for outgoing, the target for incoming). The far ends of those edges are the result's nodes, excluding the start node. An unresolved edge has no far end: it is included but adds no node.
10. Truncation: if more than `limit` nodes are reached, keep the first `limit` by `id` and set `truncated` to true. Then keep only the edges whose source and target are the start node or a kept node; an unresolved edge is kept when its source is.
11. Result `nodes` and `edges` are sorted by `id`.

## Changing the scenarios

Scenarios are part of the contract: a rule change is a contract change (`contracts/README.md`), recorded in ADR-0006. Add a scenario before changing a rule, check that the reference store fails it, then change the rule and the reference store together. Both adapters must pass the new scenario before the change is released.

## Distribution

These files are released with the contract package (ADR-0009). Until the platform's PostgreSQL adapter needs them (HOST-02), they are consumed from this repository only, so the vendored `codeatlas-contracts-0.1.0.tgz` does not include them yet.
