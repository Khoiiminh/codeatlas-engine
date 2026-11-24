# codeatlas-runtime

**Layer:** Adapter (driving)

The `serve --stdio` JSON-RPC server: reads protocol messages from stdin, invokes use cases, writes responses to stdout and sends diagnostics to stderr.

**May depend on:** `codeatlas-application`, `codeatlas-core`, `codeatlas-protocol`.

**Must not depend on:** driven adapters (the composition root injects them); entrypoints

**Contains:**
- Message loop and request correlation
- Handshake and version negotiation
- Progress/cancellation bridging
- Graceful shutdown

See [ADR-0010](../../../docs/adr/0010-engine-clean-architecture-layering.md) and the [dependency matrix](../../README.md).
