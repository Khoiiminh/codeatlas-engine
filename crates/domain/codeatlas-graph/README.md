# codeatlas-graph

**Layer:** Domain

The graph model and pure algorithms over it.

**May depend on:** `codeatlas-core`.

**Must not depend on:** any adapter or entrypoint crate; I/O, async runtimes, `serde_json`, `rusqlite`, `tree-sitter`, process spawning

**Contains:**
- Graph construction from normalized symbols/relationships
- Graph invariants (every edge has provenance/status)
- Bounded traversal: callers, callees, dependencies, dependents

See [ADR-0010](../../../docs/adr/0010-engine-clean-architecture-layering.md) and the [dependency matrix](../../README.md).
