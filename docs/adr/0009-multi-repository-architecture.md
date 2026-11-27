# ADR-0009: Multi-repository architecture and component ownership
- Status: Accepted
- Date: 2026-11-25
- Decision owners: Product architecture (engine, platform and VS Code maintainers)
- Repository scope: cross-repository
- Supersedes: none
- Superseded by: none

## Context

CodeAtlas has three independently meaningful parts, each with its own release and security lifecycle:

- a reusable analysis engine, consumed by the CLI, VS Code, hosted workers and future MCP integrations;
- a hosted SaaS platform, with its own deployment, tenant-security and operations lifecycle;
- a VS Code extension, packaged for and released through the Marketplace.

## Problem

Without explicit boundaries, components start importing each other's source, schemas are forked, and every release has to happen in lockstep. Ownership becomes unclear, and a change to one surface blocks the others.

## Decision

### Repositories and ownership

| Repository | Owns | Does not own |
|---|---|---|
| `codeatlas-engine` | Rust analysis, TypeScript semantic adapter, CLI, local runtime and SQLite, **canonical schemas**, engine releases | SaaS users, billing, OAuth, LLM-provider orchestration, UI |
| `codeatlas-platform` | Next.js, NestJS, workers, hosted orchestration, PostgreSQL/R2, AI services | Parsing and graph algorithms |
| `codeatlas-vscode` | Extension host, commands, runtime lifecycle and client, Webview, source navigation | Parser, graph engine, a separate local database |

Components stay inside their owning repository. The web app, API and worker are not split out, and neither are individual crates, the CLI or the contracts. There is **no fourth contracts repository** initially.

### Integration rules

1. The engine owns canonical JSON Schemas and publishes immutable, versioned contract artifacts: a schema bundle, the generated `@codeatlas/contracts` package, validated Rust types, and the port-behaviour fixture bundle from `contracts/fixtures` (ADR-0006).
2. The platform pins the contract package and a tested engine container or image version.
3. VS Code pins or resolves a compatible engine distribution and declares its supported protocol range.
4. No consumer edits its own copy of the canonical schemas. Generated artifacts record their source schema version.
5. Repositories never import each other's source directories. Git submodules are not the primary integration mechanism.
6. Breaking changes need a major version, compatibility tests, and linked change requests in the consumers. Releases are versioned independently, not in lockstep.
7. Every analysis snapshot records the engine and schema versions.

| Artifact | Owner | Compatibility rule |
|---|---|---|
| Engine runtime/CLI | engine | SemVer; publishes supported protocol range |
| Analysis schemas/contracts | engine | Explicit schema version; compatibility tests |
| JSON-RPC protocol | engine | Negotiated version and capabilities (ADR-0005) |
| Platform | platform | Pins tested engine and contract versions |
| VS Code extension | vscode | Declares supported engine/protocol range |

### Cross-repository ADR rules

1. ADR numbers are **global** across the three repositories and are never reused.
2. Each ADR has **one canonical file**, in the repository that owns the decision (Blueprint v1.3 §6). A cross-repository decision lives in the repository that owns it. Example: ADR-0008, AI grounding, is owned by the platform even though it uses the engine's evidence model. A cross-repository decision that no single repository owns, such as product-wide principles, lives in **`codeatlas-engine`**, because the engine owns the contracts and sits at the centre of the dependency direction.
3. Every repository keeps a `docs/adr/README.md` index listing all ADRs. It links to the canonical file, with a relative link when the ADR is in the same repository. Otherwise it uses a GitHub link to that repository's `development` branch, which is where reviewed changes land. Copies are never kept.
4. ADRs follow the template and lifecycle rules in Architecture Blueprint v1.3 §6.1 and §6.2.

### Repository operations

Each repository owns its CI, required checks (including its architecture boundary check), branch protection, versioning, release tags and changelog. `main` and `development` accept changes through pull requests only.

## Alternatives considered

- **Monorepo.** Atomic cross-component changes, but it couples three different release and security lifecycles and gives coarse access control. Rejected for now.
- **More repositories (per crate, per app, or a contracts repository).** Too much coordination overhead for a small team, and the split components share a lot of code. Rejected initially; a contracts repository can be reconsidered if consumers multiply.
- **Git submodules for sharing code.** Brittle version pinning and source-level coupling. Rejected.

## Consequences

- Positive: clear ownership, independent releases, and a reusable engine with a stable public contract.
- Negative: compatibility testing and versioning discipline are required, and cross-repository changes need coordination.
- Mitigation: immutable versioned artifacts, consumer compatibility tests (INT-01), and linked change requests for breaking changes.

## Implementation and verification

- FND-01 (Done, 2026-11-24): the three repositories exist with ownership READMEs, independent CI, and rulesets that require pull requests and passing checks on `main` and `development`.
- FND-02 (Done, 2026-11-25): ADR-0001..0012 were written, independently reviewed and accepted. The ADR indexes in all three repositories follow the cross-repository ADR rules above.
- FND-03 (2026-11-25): the schema bundle, the generated `@codeatlas/contracts` package and the Rust types are in place. CI checks them against each other and runs a consumer package-install test. The packed tarball is uploaded as a CI artifact, and the platform and VS Code pin it in `vendor/` (lockfile integrity hash, provenance in `vendor/README.md`).
- DIST-01: the engine publishes the contract package to a registry with release provenance, plus versioned binaries.
- IDE-01 and HOST-01: consumers pin or resolve compatible versions.
- INT-01: engine↔VS Code and engine↔platform compatibility tests pass on pinned versions.
- 2026-11-27: [ADR-0013](0013-branch-promotion-and-rule-ownership.md) refines how `main` is updated. `main` only moves forward to `development` by a fast-forward push, a pull request into `main` must come from `development` (required check `source-branch`), and `.github/CODEOWNERS` names the owners of rule files.

## Related records

- Architecture Blueprint v1.3 §1, §1.4, §3, §6, §7
- Traceable Implementation Plan v1.2 §1.1, §3: FND-01/02/03, DIST-01, IDE-01, HOST-01, INT-01
- [ADR-0001](0001-multi-interface-platform.md), [ADR-0002](0002-polyglot-architecture.md), [ADR-0005](0005-inter-process-communication.md)
- Layering inside each repository: [ADR-0010](0010-engine-clean-architecture-layering.md), [ADR-0011](https://github.com/Khoiiminh/codeatlas-platform/blob/development/docs/adr/0011-platform-clean-architecture-bounded-contexts.md), [ADR-0012](https://github.com/Khoiiminh/codeatlas-vscode/blob/development/docs/adr/0012-vscode-extension-layering.md)
