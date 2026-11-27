# ADR-0002: Polyglot architecture
- Status: Accepted
- Date: 2026-11-25
- Decision owners: Product architecture (engine, platform and VS Code maintainers)
- Repository scope: cross-repository
- Supersedes: none
- Superseded by: none

## Context

CodeAtlas has two very different kinds of work:

- **Analysis computation:** scanning large repositories, parsing thousands of files, and building and traversing graphs. It must be fast, memory-efficient, parallel and deterministic.
- **Product and integration work:** a web UI, HTTP APIs, background jobs, AI provider calls and a VS Code extension. These live in the JavaScript/TypeScript ecosystem.

TypeScript semantic analysis in particular is only available through the TypeScript Compiler API, which runs on Node.js.

## Problem

One language cannot serve both kinds of work well. Using one language for everything either makes analysis slow and memory-hungry, or cuts the product off from the web, editor and TypeScript tooling ecosystems.

## Decision

1. **Rust** owns reusable analysis computation in `codeatlas-engine`: scanning, Tree-sitter parsing, graph construction and traversal, impact computation, local SQLite persistence, the CLI and the stdio runtime.
2. **TypeScript** owns:
   - the application and control plane in `codeatlas-platform`: Next.js, NestJS, workers and AI services;
   - the VS Code extension in `codeatlas-vscode`;
   - the engine's semantic adapter `packages/typescript-analyzer`, which uses the TypeScript Compiler API.
3. The languages integrate only through **versioned contracts and process boundaries**: JSON Schemas with generated TypeScript types and Rust validation (FND-03), JSON-RPC over stdio (ADR-0005), and the engine binary or container. The engine reaches `typescript-analyzer` as a separate Node process through `codeatlas-semantic-typescript` (ADR-0010). There are no in-process language bindings initially.
4. Architecturally this remains a **modular monolith plus an independent analysis runtime**, not a microservices estate.
5. Toolchains are pinned per repository: `rust-toolchain.toml` in the engine and `.node-version` in all three repositories.

## Alternatives considered

- **All TypeScript.** One toolchain, but large-repository analysis would be slower and memory-bound, with weaker parallelism. Rejected for the engine.
- **All Rust.** There is no TypeScript Compiler API equivalent and no web or editor ecosystem. Rejected.
- **Go for services or analysis.** Deferred until evidence justifies a third language (Blueprint v1.3 §8).
- **In-process bindings (N-API or WASM) between TypeScript and Rust.** They couple binary ABIs and crash domains. Deferred; process boundaries are simpler to version and test.

## Consequences

- Positive: each part uses the best-suited ecosystem, and the engine is reusable by every host.
- Negative: two toolchains, contract generation, and process-boundary latency and serialization cost.
- Mitigation: one canonical schema source (engine `contracts/schemas`), generated types, pinned toolchains, and mapping only at adapter boundaries (ADR-0010).

## Implementation and verification

- FND-01: the Rust and Node toolchains are pinned, and each repository builds independently in CI (Done, 2026-11-24).
- FND-03: the schemas generate TypeScript types, and Rust compatibility tests pass.
- ENG-02..05: Rust and the TypeScript adapter produce normalized results through the contracts.
- HOST-01: the platform worker invokes the pinned engine image rather than linking engine code.

## Related records

- Architecture Blueprint v1.3 §1, §3.2, §5, §8
- Traceable Implementation Plan v1.2: FND-01, FND-03, ENG-02..05, HOST-01
- [ADR-0004](0004-hybrid-analysis-engine.md) (hybrid analysis), [ADR-0005](0005-inter-process-communication.md) (IPC), [ADR-0009](0009-multi-repository-architecture.md) (multi-repository), [ADR-0010](0010-engine-clean-architecture-layering.md) (engine layering)
