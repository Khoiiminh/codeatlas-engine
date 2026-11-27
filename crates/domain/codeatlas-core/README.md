# codeatlas-core

**Layer:** Domain

The language-neutral analysis model: what a repository, file, symbol, relationship, snapshot and piece of evidence *is*, plus the rules that keep them valid.

**May depend on:** Rust `std` and small utility crates (for example `thiserror`).

**Must not depend on:** any adapter or entrypoint crate; I/O, async runtimes, `serde_json`, `rusqlite`, `tree-sitter`, process spawning

**Contains:**
- Entities and value objects: `RepositoryId`, `SourceFile`, `SourceSpan`, `Symbol`, `Relationship`, `Snapshot`, `Evidence`, `Diagnostic`
- Stable symbol identity rules
- Evidence classification and precedence (structural, semantic, inferred, unresolved)
- Language-neutral dependency-resolution rules
- Domain error types

See [ADR-0010](../../../docs/adr/0010-engine-clean-architecture-layering.md) and the [dependency matrix](../../README.md).
