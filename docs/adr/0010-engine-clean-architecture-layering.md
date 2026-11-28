# ADR-0010: Engine Clean Architecture layering and crate dependency rule
- Status: Accepted
- Date: 2026-11-25
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

1. Dependencies point inward only, as defined by the **dependency matrix below, which is normative**.
2. Port traits (`SourceScanner`, `LanguageParser`, `SemanticResolver`, `SnapshotStore`, `GraphReader`, `ArtifactStore`, `JobQueue`, `ProgressReporter`, `CancellationToken`, `Clock`) are declared in `codeatlas-application`.
3. Domain and application crates must not depend, directly or transitively, on infrastructure crates: SQLite (`rusqlite`, `libsqlite3-sys`), `tree-sitter*`, `serde_json`, async runtimes (`tokio*`, `async-std`, `smol`) or IPC crates (`interprocess`, `ipc-channel`). The authoritative list is `FORBIDDEN_IN_INNER_LAYERS` in `tests/architecture/src/lib.rs`. Adding a crate to it tightens the rule; removing one requires amending this ADR.
4. Contract (wire) types live only in `codeatlas-protocol` and are mapped there. Domain types are never serialized directly as canonical contracts.
5. The only allowed adapter-to-adapter dependency is `codeatlas-runtime` → `codeatlas-protocol`. Any new exception requires amending this ADR.
6. `packages/typescript-analyzer` is an out-of-process driven adapter, reached only through `codeatlas-semantic-typescript`.
7. Adapters depend on `codeatlas-core` and `codeatlas-application` only, not directly on `codeatlas-graph` or `codeatlas-impact`. This is stricter than Blueprint v1.3 §2.1 on purpose: graph and impact internals can change without touching adapters. Graph or impact types that an adapter needs, for example in `codeatlas-protocol` mappers, are exposed through `codeatlas-application` (use-case results or re-exports).
8. No direct query-port exception (Blueprint v1.3 §2.3) is granted. `codeatlas-runtime` and `codeatlas-cli` call use cases.

### Dependency matrix (normative)

✔ = allowed; blank = forbidden. "Other adapters" means adapter crates other than `codeatlas-protocol`.

| Crate ↓ may depend on → | core | graph | impact | application | protocol | other adapters |
|---|:-:|:-:|:-:|:-:|:-:|:-:|
| `domain/codeatlas-core` | | | | | | |
| `domain/codeatlas-graph` | ✔ | | | | | |
| `domain/codeatlas-impact` | ✔ | ✔ | | | | |
| `application/codeatlas-application` | ✔ | ✔ | ✔ | | | |
| `adapters/codeatlas-protocol` | ✔ | | | ✔ | | |
| `adapters/codeatlas-runtime` | ✔ | | | ✔ | ✔ | |
| `adapters/codeatlas-scanner-fs`, `-parser-treesitter`, `-semantic-typescript`, `-storage-sqlite` | ✔ | | | ✔ | | |
| `entrypoints/codeatlas-cli` | ✔ | ✔ | ✔ | ✔ | ✔ | ✔ |

Copies that must match it: [`crates/README.md`](../../crates/README.md) (for readers) and `MATRIX` in `tests/architecture/src/lib.rs` (enforced). Changes to this matrix are made by amending this ADR in the same pull request that updates its copies. A change that adds an edge (a new layer exception) needs review by the decision owners before merge (Traceable Implementation Plan §10 rule 7). The amendment is recorded with its date under "Implementation and verification".

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
- ENG-01 (2026-11-27): first production code in the layers. `codeatlas-core` and `codeatlas-application` still have no external dependencies. The only external crates are in the `codeatlas-scanner-fs` adapter: `ignore` for `.gitignore` matching and `sha2` for content hashes.
- ENG-02 (2026-11-28): `codeatlas-application` defines the `LanguageParser` and `SourceReader` ports before their adapters. `tree-sitter`, `tree-sitter-typescript` and `tree-sitter-javascript` are used only in `codeatlas-parser-treesitter`, whose Tree-sitter types never leave the crate. `FsSourceReader` lives in `codeatlas-scanner-fs`. The end-to-end test that combines both adapters lives in the tooling crate `codeatlas-fixture-tests`, because adapters may not depend on each other, not even in tests. The architecture check caught exactly that in a first draft.
- FND-04 (2026-11-27): `tests/fixtures` adds a second test-tooling crate, `codeatlas-fixture-tests`, listed in `TOOLING_CRATES`. Like the architecture tests, it is outside the dependency matrix and no product crate may depend on it. The matrix is unchanged.
- FND-05: `tests/architecture` (`codeatlas-architecture-tests`) asserts the `cargo metadata` crate graph against the matrix and the forbidden-external list, as the required CI job `architecture`. A deliberately introduced violation must fail CI. Verified on 2026-11-24: an adapter-to-adapter edge and `serde_json` in a domain crate fail the test, and an inward-to-outward edge is rejected by Cargo as a cycle.
- ENG-01/02/04 and LOC-01 each introduce their port in `codeatlas-application` before the adapter.
- Domain/application unit tests run with in-memory port fakes only.

## Related records

- Architecture Blueprint v1.3 §1.1, §2, §4
- Traceable Implementation Plan v1.2: FND-01, FND-05, ENG-01..05, LOC-01, RT-01
- [ADR-0004](0004-hybrid-analysis-engine.md) (hybrid analysis engine), [ADR-0005](0005-inter-process-communication.md) (IPC), [ADR-0006](0006-persistence-strategy.md) (persistence), [ADR-0009](0009-multi-repository-architecture.md) (multi-repository)
- Sibling layering decisions: [ADR-0011](https://github.com/Khoiiminh/codeatlas-platform/blob/development/docs/adr/0011-platform-clean-architecture-bounded-contexts.md) (`codeatlas-platform`), [ADR-0012](https://github.com/Khoiiminh/codeatlas-vscode/blob/development/docs/adr/0012-vscode-extension-layering.md) (`codeatlas-vscode`)
