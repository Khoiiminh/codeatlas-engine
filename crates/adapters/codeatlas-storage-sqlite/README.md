# codeatlas-storage-sqlite

**Layer:** Adapter (driven)

Implements `SnapshotStore`, `GraphReader` and `ArtifactStore` for local execution with SQLite and the local filesystem.

**May depend on:** `codeatlas-application`, `codeatlas-core`, `rusqlite` (or equivalent).

**Must not depend on:** other adapter crates (except `codeatlas-protocol` where stated); entrypoints

**Contains:**
- Schema migrations
- Atomic snapshot publication
- Port behaviour tests driven by `contracts/fixtures`

See [ADR-0010](../../../docs/adr/0010-engine-clean-architecture-layering.md) and the [dependency matrix](../../README.md).
