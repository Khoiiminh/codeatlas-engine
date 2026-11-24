# codeatlas-parser-treesitter

**Layer:** Adapter (driven)

Implements `LanguageParser`: Tree-sitter parsing and structural symbol/relationship extraction for TS/JS/TSX/JSX. New languages are added here (or as sibling crates) without touching domain or application code.

**May depend on:** `codeatlas-application`, `codeatlas-core`, `tree-sitter` and grammar crates.

**Must not depend on:** other adapter crates (except `codeatlas-protocol` where stated); entrypoints

**Contains:**
- Grammar registry
- CST to domain symbol/relationship extraction
- Syntax-error tolerant parsing

See [ADR-0010](../../../docs/adr/0010-engine-clean-architecture-layering.md) and the [dependency matrix](../../README.md).
