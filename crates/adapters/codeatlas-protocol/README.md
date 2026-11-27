# codeatlas-protocol

**Layer:** Adapter (shared interface mapping)

Wire-level types generated from or validated against `contracts/schemas`, plus mappers between wire types and application/domain types. All driving adapters share it, so the mapping exists once.

**May depend on:** `codeatlas-application`, `codeatlas-core`, `serde`, `serde_json`.

**Must not depend on:** driven adapters; entrypoints

**Contains:**
- Analysis request/result DTOs
- JSON-RPC request/response/notification types, handshake and capabilities
- Contract-to-domain mappers
- Schema version constants

See [ADR-0010](../../../docs/adr/0010-engine-clean-architecture-layering.md) and the [dependency matrix](../../README.md).
