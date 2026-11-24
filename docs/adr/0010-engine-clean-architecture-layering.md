# ADR-0010: Engine Clean Architecture layering and crate dependency rule
- Status: Proposed
- Date: 2026-11-24
- Decision owners: Engine maintainers
- Repository scope: engine
- Supersedes: none
- Superseded by: none

## Context

`codeatlas-engine` owns reusable analysis computation (Architecture Blueprint v1.3 §1.1, §5). The same logic must be driven by several hosts: the `codeatlas` CLI, the `serve --stdio` JSON-RPC runtime used by VS Code, hosted platform workers that run the engine container, and future MCP integrations. It must also persist to more than one store (local SQLite now; hosted persistence is implemented by the platform against the same behavioural expectations), support more languages over time, and keep canonical contracts stable while internals evolve.

The initial scaffold had a flat set of crates (`codeatlas-core`, `-analysis`, `-graph`, `-impact`, `-protocol`, `-runtime`, `-storage-sqlite`, `-cli`) with no stated dependency direction. Their names also differed from the component list in Blueprint v1.2.

## Problem

Without an explicit and enforced dependency rule, infrastructure concerns (SQLite, Tree-sitter, JSON-RPC, the filesystem) leak into graph and impact logic. Each new host, language or storage back end then requires changes to core algorithms, and the core becomes hard to unit-test.

## Decision

Adopt Clean Architecture (Blueprint v1.3 §2) with four layers, expressed as Cargo crates grouped by folder:

| Layer | Crates |
|---|---|
| Domain | `crates/domain/codeatlas-core`, `codeatlas-graph`, `codeatlas-impact` |
| Application | `crates/application/codeatlas-application` (use cases + port traits) |
| Adapters | `crates/adapters/codeatlas-scanner-fs`, `-parser-treesitter`, `-semantic-typescript`, `-storage-sqlite` (driven); `codeatlas-runtime` (driving); `codeatlas-protocol` (shared contract mapping) |
| Entrypoints | `crates/entrypoints/codeatlas-cli` (the only composition root) |

Rules:

1. Dependencies point inward only. The exact matrix is in [`crates/README.md`](../../crates/README.md) and is normative.
2. Port traits (`SourceScanner`, `LanguageParser`, `SemanticResolver`, `SnapshotStore`, `GraphReader`, `ArtifactStore`, `ProgressReporter`, `CancellationToken`, `Clock`) are declared in `codeatlas-application`.
3. Domain and application crates must not depend on `rusqlite`, `tree-sitter*`, `serde_json`, an async runtime, or process/IPC crates.
4. Contract (wire) types live only in `codeatlas-protocol` and are mapped there. Domain types are never serialized directly as canonical contracts.
5. The only allowed adapter-to-adapter dependency is `codeatlas-runtime` → `codeatlas-protocol`. Any new exception requires amending this ADR.
6. `packages/typescript-analyzer` is an out-of-process driven adapter, reached only through `codeatlas-semantic-typescript`.

Scaffold renames: `codeatlas-analysis` → `codeatlas-application`. New crates: `codeatlas-scanner-fs`, `codeatlas-parser-treesitter`, `codeatlas-semantic-typescript`. The Blueprint v1.2 components map as described in Blueprint v1.3 §1.1.

## Alternatives considered

- **Flat crates with conventions only.** Lower upfront cost, but the dependency direction is not enforced and erodes as soon as deadlines hit. Rejected.
- **A single crate with modules.** Fastest to start, but module privacy cannot stop `use rusqlite` inside graph code, and incremental compile times grow. Rejected.
- **Finer split (one crate per v1.2 component: scanner, parser, symbol-extractor, dependency-resolver, ...).** More boundaries than the team needs initially, and many of them would be adapter internals. Deferred: crates can be split later without changing the layer rule.

## Consequences

- Positive: new hosts, languages and storage back ends are additive. Graph and impact logic is pure and fast to test. The crate graph doubles as living documentation.
- Negative: there is mapping code between contract DTOs and domain types, and more `Cargo.toml` files. Use-case signatures need some upfront design.
- Mitigation: proportionate strictness (Blueprint v1.3 §2.3). Generate DTOs where practical. Review mapping cost at the ENG-05 and LOC-01 reviews.

## Implementation and verification

- FND-01: workspace `Cargo.toml` and per-crate manifests that encode exactly the matrix.
- FND-05: `tests/architecture` (`codeatlas-architecture-tests`) asserts the `cargo metadata` crate graph against the matrix and the forbidden-external list, as the required CI job `architecture`. A deliberately introduced violation must fail CI. Verified on 2026-11-24: an adapter-to-adapter edge and `serde_json` in a domain crate fail the test, and an inward-to-outward edge is rejected by Cargo as a cycle.
- ENG-01/02/04 and LOC-01 each introduce their port in `codeatlas-application` before the adapter.
- Domain/application unit tests run with in-memory port fakes only.

## Related records

- Architecture Blueprint v1.3 §1.1, §2, §4
- Traceable Implementation Plan v1.2: FND-01, FND-05, ENG-01..05, LOC-01, RT-01
- ADR-0004 (hybrid analysis engine), ADR-0005 (IPC), ADR-0006 (persistence), ADR-0009 (multi-repository)
- Sibling layering decisions: ADR-0011 (`codeatlas-platform`), ADR-0012 (`codeatlas-vscode`)
