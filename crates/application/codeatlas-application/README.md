# codeatlas-application

**Layer:** Application

Use cases that orchestrate the domain, and the **port traits** they need from the outside world. This is the single API that every driving adapter (CLI, stdio runtime, future MCP server) calls.

**May depend on:** `codeatlas-core`, `codeatlas-graph`, `codeatlas-impact`.

**Must not depend on:** any adapter or entrypoint crate; concrete infrastructure (`rusqlite`, `tree-sitter`, filesystem walking, JSON-RPC)

**Contains:**
- Use cases: `AnalyzeRepository` (full + incremental), `QueryGraph`, `ComputeImpact`, `ManageSnapshots`
- Driven ports: `SourceScanner`, `LanguageParser`, `SemanticResolver`, `SnapshotStore`, `GraphReader`, `ArtifactStore`, `ProgressReporter`, `CancellationToken`, `Clock`
- Application request/response types (not the wire contracts)
- In-memory port fakes for unit tests (behind a `test-support` feature)

See [ADR-0010](../../../docs/adr/0010-engine-clean-architecture-layering.md) and the [dependency matrix](../../README.md).
