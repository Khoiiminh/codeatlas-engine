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
cargo test -p codeatlas-fixture-tests         # FND-04 fixtures and port scenarios; ENG-01..03 end to end against the golden files
cargo test --release -p codeatlas-fixture-tests --test benchmark -- --ignored --nocapture   # ADR-0014 benchmark (1M generated lines)
cargo clippy --workspace --all-targets -- -D warnings
cd packages/typescript-analyzer && npm ci && npm run typecheck && npm run check:fixtures && npm run check:parser-cases && npm run check:symbol-cases
cd packages/contracts && npm ci && npm run check:generated && npm run format:check && npm run typecheck && npm run test:consumer
```

CI (`.github/workflows/ci.yml`) runs the jobs `rust`, `portability`, `architecture`, `contracts` and `typescript-analyzer`. `portability` runs the Rust tests on Windows and macOS (`rust` covers Linux) and reports two checks, `portability (windows-latest)` and `portability (macos-latest)`. Mark these six checks as required in `protect-development` and `protect-main`. Another job, `benchmark`, times the engine on a generated repository and writes the result to the job summary; it is report-only, so do not make it required ([ADR-0014](docs/adr/0014-engine-run-model-and-quality-targets.md)). The `contracts` job also uploads the packed `@codeatlas/contracts` tarball as the `codeatlas-contracts` artifact.

Branches ([ADR-0013](docs/adr/0013-branch-promotion-and-rule-ownership.md)): work reaches `development` through pull requests, which must be up to date with `development` before they merge ("Require branches to be up to date before merging" in `protect-development`), so a squash merge lands exactly what CI tested. New pull requests default to base `main`, so set the base to `development`. `main` only moves forward to a commit of `development` (a fast-forward push by a maintainer on the `protect-main` bypass list, bypass mode Always), so both branches share one history. The `source-branch` check in `.github/workflows/branch-policy.yml` fails any pull request into `main` that does not come from this repository's `development`. Make it required in `protect-main` only. `.github/CODEOWNERS` names the owners of the files that define this repository's rules. Releases are annotated `vX.Y.Z` tags on commits that `main` already points to; a tag is never moved or deleted, and a bad release gets a new patch version. The contract package and schema bundle are versioned separately and tagged `contracts-vX.Y.Z`.

Canonical contracts (schemas, examples, versioning rules): [`contracts/README.md`](contracts/README.md). Test fixtures: [`tests/fixtures/README.md`](tests/fixtures/README.md) (source repositories) and [`contracts/fixtures/ports/README.md`](contracts/fixtures/ports/README.md) (snapshot-port scenarios).

## Decision records

See [`docs/adr/`](docs/adr/README.md).
