# CodeAtlas Engine

The core codebase intelligence engine.

Responsibilities:
- Repository scanning and parsing
- Symbol extraction and semantic analysis
- Dependency and call graph construction
- Static change impact analysis
- Local runtime and CLI
- Canonical analysis contracts

## Architecture

The engine follows Clean Architecture with an inward-only dependency rule ([ADR-0010](docs/adr/0010-engine-clean-architecture-layering.md)):

```text
crates/
├── domain/        codeatlas-core, codeatlas-graph, codeatlas-impact     (pure model and rules)
├── application/   codeatlas-application                                  (use cases + port traits)
├── adapters/      scanner-fs, parser-treesitter, semantic-typescript,
│                  storage-sqlite, protocol, runtime                      (ports implemented / delivery)
└── entrypoints/   codeatlas-cli                                          (composition root)
packages/typescript-analyzer   out-of-process TypeScript Compiler API adapter
contracts/                     canonical JSON Schemas and fixtures (single source of truth)
tests/                         fixtures, integration, compatibility, architecture
```

See [`crates/README.md`](crates/README.md) for the crate dependency matrix and how to add languages, hosts and storage back ends.

## Build and checks

Toolchains are pinned in `rust-toolchain.toml` and `.node-version`.

```sh
cargo build --workspace
cargo test --workspace                        # includes the architecture boundary check
cargo test -p codeatlas-architecture-tests    # boundary check only (tests/architecture)
cargo test -p codeatlas-fixture-tests         # FND-04 fixtures: SynchroStream golden file, port scenarios
cargo clippy --workspace --all-targets -- -D warnings
cd packages/typescript-analyzer && npm ci && npm run typecheck && npm run check:fixtures
cd packages/contracts && npm ci && npm run check:generated && npm run format:check && npm run typecheck && npm run test:consumer
```

CI (`.github/workflows/ci.yml`) runs the jobs `rust`, `architecture`, `contracts` and `typescript-analyzer`. Mark all four as required in branch protection. The `contracts` job also uploads the packed `@codeatlas/contracts` tarball as the `codeatlas-contracts` artifact.

Canonical contracts (schemas, examples, versioning rules): [`contracts/README.md`](contracts/README.md). Test fixtures: [`tests/fixtures/README.md`](tests/fixtures/README.md) (source repositories) and [`contracts/fixtures/ports/README.md`](contracts/fixtures/ports/README.md) (snapshot-port scenarios).

## Decision records

See [`docs/adr/`](docs/adr/README.md).
