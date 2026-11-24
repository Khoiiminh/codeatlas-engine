# codeatlas-impact

**Layer:** Domain

Static change-impact rules: given changed spans/symbols and a graph, compute the affected set with evidence classes.

**May depend on:** `codeatlas-core`, `codeatlas-graph`.

**Must not depend on:** any adapter or entrypoint crate; I/O, async runtimes, `serde_json`, `rusqlite`, `tree-sitter`, process spawning; Git libraries (diffs arrive as domain values)

**Contains:**
- Change-set value objects
- Impact propagation rules and bounds
- Impact result model with evidence classification

See [ADR-0010](../../../docs/adr/0010-engine-clean-architecture-layering.md) and the [dependency matrix](../../README.md).
