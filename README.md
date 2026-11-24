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
cargo clippy --workspace --all-targets -- -D warnings
cd packages/typescript-analyzer && npm ci && npm run typecheck
```

CI (`.github/workflows/ci.yml`) runs the jobs `rust`, `architecture` and `typescript-analyzer`. Mark all three as required in branch protection.

## Decision records

See [`docs/adr/`](docs/adr/README.md).
