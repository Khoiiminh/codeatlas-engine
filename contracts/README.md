# Canonical contracts (FND-03)

This folder is the **single source of truth** for every message that crosses a process or repository boundary: the analysis model (nodes, edges, evidence, snapshots, requests and results) and the JSON-RPC runtime protocol. Decision records: [ADR-0005](../docs/adr/0005-inter-process-communication.md) (protocol) and [ADR-0009](../docs/adr/0009-multi-repository-architecture.md) (ownership and distribution).

```text
contracts/
├── schemas/
│   └── codeatlas.schema.json      JSON Schema 2020-12 bundle; every type is a $defs entry
└── fixtures/
    └── examples/<Type>/           valid-*.json must pass, invalid-*.json must fail
```

## Who uses it

| Consumer | How | Kept in step by |
|---|---|---|
| Rust: `crates/adapters/codeatlas-protocol` | Hand-written serde types | `tests/contract_compatibility.rs`: every example is validated against the schema and round-tripped through the Rust type (`rust` CI job) |
| TypeScript: `packages/contracts` (`@codeatlas/contracts`) | Types and constants generated from the schema, plus the schema file itself | `npm run check:generated`, the fixture type-check and the consumer package-install test (`contracts` CI job) |
| `codeatlas-platform`, `codeatlas-vscode` | Pin a packed `@codeatlas/contracts` tarball in `vendor/` | Lockfile integrity hash; their own typecheck and boundary checks |

Domain and application layers never import contract types (ADR-0010/0011/0012); adapters map them.

## Rules

- **Wire conventions:** property names are `camelCase` and enum values are `snake_case`. Paths are repository-relative with `/`. Positions are zero-based, with `character` counted in UTF-16 code units (as in VS Code and LSP). Timestamps are RFC 3339.
- **Closed for producers, open for readers:** every object schema has `additionalProperties: false`, so tests catch a producer that sends a misspelled or undeclared field. Readers must **ignore unknown properties**, so an engine that adds a field in a later version does not break an older consumer. The Rust types follow this (no `deny_unknown_fields`).
- **Evidence-first:** every edge has at least one `Evidence`. An unresolved reference is an edge with `resolution: "unresolved"` and an `unresolved` reason, never a guess (ADR-0004).
- **Bounded:** graph queries carry `depth` (≤ 5) and `limit` (≤ 1000), and results say whether they were `truncated`.

## Versioning

- `x-contract-version` (SemVer `MAJOR.MINOR.PATCH`, also in `$id`) versions the whole bundle. `x-protocol-version` (`MAJOR.MINOR`) versions the JSON-RPC protocol.
- **While the major version is 0, every minor bump may break** (SemVer). Two versions are compatible when they share a *compatibility key*: `MAJOR.MINOR` while MAJOR is 0, and `MAJOR` from 1.0 on. Both the Rust `is_protocol_compatible` and the TypeScript `isProtocolCompatible` implement this rule.
- From 1.0: additive optional fields are a minor bump, and removing, renaming or tightening anything is a major bump. A major bump needs linked consumer change requests (ADR-0009).
- The `initialize` handshake negotiates the protocol. The client lists the versions it supports, and the engine picks one or answers `UNSUPPORTED_PROTOCOL_VERSION` (-32010).

## Changing a contract

1. Edit `schemas/codeatlas.schema.json` and bump `x-contract-version`, `$id` and, if the protocol changed, `x-protocol-version`.
2. Add or update examples in `fixtures/examples/<Type>/`, with at least one `valid-` and one `invalid-` file per type.
3. Update the Rust types and the version constants in `codeatlas-protocol`, then run `cargo test -p codeatlas-protocol`.
4. In `packages/contracts`: update `version` in `package.json`, run `npm run generate`, then run `npm run typecheck` and `npm run test:consumer`.
5. Commit the schema, the examples, the Rust changes and the generated files together. CI fails if any of them disagree.
6. To ship it to the consumers, take the `codeatlas-contracts` tarball from the CI run (or from `npm pack`) and follow `vendor/README.md` in `codeatlas-platform` and `codeatlas-vscode`. Releases to a registry come with DIST-01.
