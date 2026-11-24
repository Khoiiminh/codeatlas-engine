# codeatlas-cli

**Layer:** Entrypoint (composition root)

The `codeatlas` binary. Parses CLI arguments and configuration, constructs the concrete adapters, injects them into use cases, and either runs a command (`analyze`, `symbols`, `callers`, ...) or starts `codeatlas-runtime` (`serve --stdio`).

**May depend on:** every crate in this workspace.

**Must not depend on:** nothing, but it must contain **no business rules**: only wiring, argument parsing and output formatting

**Contains:**
- Argument parsing
- Configuration loading
- Dependency wiring
- Human/JSON output formatting

See [ADR-0010](../../../docs/adr/0010-engine-clean-architecture-layering.md) and the [dependency matrix](../../README.md).
