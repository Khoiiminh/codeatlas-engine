# codeatlas-semantic-typescript

**Layer:** Adapter (driven)

Implements `SemanticResolver` by talking to the Node process in `packages/typescript-analyzer` (TypeScript Compiler API).

**May depend on:** `codeatlas-application`, `codeatlas-core`, process/IPC crates.

**Must not depend on:** other adapter crates (except `codeatlas-protocol` where stated); entrypoints

**Contains:**
- Analyzer process lifecycle
- Mapping responses to domain semantic records
- Timeouts and failures become explicit unresolved evidence

See [ADR-0010](../../../docs/adr/0010-engine-clean-architecture-layering.md) and the [dependency matrix](../../README.md).
