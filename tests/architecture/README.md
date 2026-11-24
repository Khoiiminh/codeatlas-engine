# Architecture tests

Required CI check `architecture` (FND-05, [ADR-0010](../../docs/adr/0010-engine-clean-architecture-layering.md)).

`codeatlas-architecture-tests` is a workspace member that reads the resolved crate graph with `cargo metadata` and fails if:

- a crate depends on a workspace crate the [dependency matrix](../../crates/README.md) does not allow (any dependency kind);
- a crate under `crates/` is missing from the matrix, or sits in a folder that does not match its layer;
- a `domain/*` or `application/*` crate reaches a forbidden external crate (`rusqlite`, `tree-sitter*`, `serde_json`, `tokio*`, other async runtimes, IPC crates) through normal or build dependencies, directly or transitively.

An inward-to-outward edge (for example `codeatlas-core` → `codeatlas-storage-sqlite`) also creates a crate cycle, which Cargo itself rejects before the test runs.

```sh
cargo test -p codeatlas-architecture-tests
```

The matrix is copied in [`src/lib.rs`](src/lib.rs) (`MATRIX`, `FORBIDDEN_IN_INNER_LAYERS`). Change it together with `crates/README.md`; a new adapter-to-adapter edge also needs an ADR-0010 amendment. Unit tests in `src/lib.rs` prove each rule rejects a synthetic violation.
