# codeatlas-semantic-typescript

**Layer:** Adapter (driven)

Implements `SemanticResolver` by running the Node program in `packages/typescript-analyzer` (TypeScript Compiler API), once per call ([ADR-0015](../../../docs/adr/0015-typescript-semantic-analysis.md)).

**May depend on:** `codeatlas-application`, `codeatlas-core`, `serde` and `serde_json` (the analyzer protocol), process crates.

**Must not depend on:** other adapter crates (except `codeatlas-protocol` where stated); entrypoints

**Contains:**
- `TypeScriptResolver`, configured with an `AnalyzerCommand` (for example `node <engine>/packages/typescript-analyzer/src/main.ts`) and `AnalyzerSettings` (stall timeout, the analysis thread's stack and heap)
- The analyzer process lifecycle: the request on stdin, one JSON message per line on stdout, stderr kept for error messages
- Mapping messages to domain `SemanticFacts` (`src/wire.rs`, protocol version 1)
- Failures:
  - an analyzer that cannot start, or speaks another protocol, is `SemanticError::Unavailable`;
  - one that crashes, stops, sends an invalid message or stalls is killed, and the files it did not report fail with the reason;
  - cancellation kills it within the poll interval (50 ms).

Tests: `tests/process.rs` uses small fake analyzers (crashes, invalid output, stalls, cancellation). `tests/analyzer.rs` runs the real analyzer on positions, a 3,000-file import chain, configuration problems and unreadable files. Both need Node, and the real analyzer needs `npm ci` in `packages/typescript-analyzer`.

See [ADR-0010](../../../docs/adr/0010-engine-clean-architecture-layering.md) and the [dependency matrix](../../README.md).
