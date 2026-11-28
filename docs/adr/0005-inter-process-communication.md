# ADR-0005: Inter-process communication
- Status: Accepted
- Date: 2026-11-25
- Decision owners: Engine maintainers (protocol owner); VS Code maintainers (primary consumer)
- Repository scope: cross-repository
- Supersedes: none
- Superseded by: none

## Context

The VS Code extension, and future hosts such as an MCP server, must talk to a long-running local engine process. That process holds an analysed snapshot, answers graph queries, reports progress and can be cancelled. The engine and its consumers are released independently (ADR-0009), so the protocol has to be versioned and negotiated.

## Problem

An ad-hoc or implicit protocol breaks silently when either side upgrades. Some transports bring their own problems: network ports need authentication and can trigger firewall prompts, and native modules couple binary ABIs and crash domains.

## Decision

1. The local runtime is **`codeatlas serve --stdio`**, speaking **JSON-RPC 2.0 over standard input/output**.
2. **stdout carries protocol messages only.** All diagnostics and logs go to **stderr**.
3. The first exchange is a **handshake** that negotiates protocol version, runtime and engine versions, and capabilities. A consumer fails clearly on an unsupported major version.
4. Long operations report **progress notifications** and accept **cancellation**. Shutdown is explicit and must end in a clean process exit.
5. Request, response and notification shapes are defined by the canonical JSON Schemas in `contracts/schemas` (FND-03). They are implemented in `codeatlas-protocol` and served by the `codeatlas-runtime` driving adapter (ADR-0010).
6. Protocol changes follow the compatibility policy: additive fields are allowed under the schema policy, and breaking changes need a major version, coordinated consumer changes, and a migration window where practical (ADR-0009).

## Alternatives considered

- **gRPC.** Strong typing, but it adds a heavy toolchain and a network listener for a local, single-client channel. Deferred (Blueprint v1.3 §8).
- **HTTP or WebSocket on localhost.** Needs port management, local authentication and firewall handling. Rejected for the local path.
- **A native Node module (N-API) inside the extension host.** Binaries are tied to platform and ABI, and an engine crash takes down the extension host. Rejected.
- **The Language Server Protocol.** Its message set is editor-centric and does not model graph queries or snapshots. Not adopted; JSON-RPC keeps the same framing style without LSP semantics.

## Consequences

- Positive: a simple transport with no ports, the same protocol for every local host, crash isolation, and testability by piping messages in and out.
- Negative: JSON serialization cost on large results, and an engine process lifecycle the extension has to manage.
- Mitigation: bounded query results (LOC-02, IDE-02), and lifecycle and reconnect handling in `codeatlas-vscode/src/adapters/engine` (IDE-01).

## Implementation and verification

- FND-03 (2026-11-25): the protocol is defined in `contracts/schemas/codeatlas.schema.json`: the JSON-RPC envelopes, `initialize` / `analysis/run` / `graph/query` / `snapshot/list` / `shutdown`, `$/progress`, `$/cancelRequest`, `exit`, and the error codes including `UNSUPPORTED_PROTOCOL_VERSION` (-32010). Version negotiation follows the compatibility-key rule in `codeatlas_protocol::version`, which is unit-tested. `contract_compatibility.rs` checks schema, examples and Rust types against each other.
- RT-01: protocol tests; a test asserts that stdout is protocol-only.
- ARC-01 (2026-11-28): the engine side of decision 4 exists: use cases report `Progress` and stop on a `CancellationToken` ([ADR-0014](0014-engine-run-model-and-quality-targets.md)). RT-02 maps them to `$/progress` and `$/cancelRequest`.
- RT-02: progress, cancellation, stderr diagnostics and clean-exit lifecycle tests.
- DIST-01: releases publish the supported protocol range.
- IDE-01: the extension negotiates versions and fails clearly on an unsupported version.

## Related records

- Architecture Blueprint v1.3 §3.3, §3.4
- Traceable Implementation Plan v1.2: FND-03, RT-01/02, DIST-01, IDE-01
- [ADR-0007](https://github.com/Khoiiminh/codeatlas-vscode/blob/development/docs/adr/0007-vscode-extension.md) (VS Code extension), [ADR-0009](0009-multi-repository-architecture.md) (multi-repository), [ADR-0010](0010-engine-clean-architecture-layering.md) (engine layering), [ADR-0012](https://github.com/Khoiiminh/codeatlas-vscode/blob/development/docs/adr/0012-vscode-extension-layering.md) (VS Code layering)
