# codeatlas-scanner-fs

**Layer:** Adapter (driven)

Implements `SourceScanner`: deterministic file discovery, language detection, ignore/exclusion rules.

**May depend on:** `codeatlas-application`, `codeatlas-core`, filesystem/ignore crates.

**Must not depend on:** other adapter crates (except `codeatlas-protocol` where stated); entrypoints

**Contains:**
- Filesystem walker
- Exclusion of `.gitignore`d, generated and dependency folders
- Language detection

See [ADR-0010](../../../docs/adr/0010-engine-clean-architecture-layering.md) and the [dependency matrix](../../README.md).
