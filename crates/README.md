# Crates: Clean Architecture layout

Dependencies point **inward only**: `entrypoints -> adapters -> application -> domain`.

```text
crates/
├── domain/        Layer 1: pure model and rules (no I/O)
├── application/   Layer 2: use cases + port traits
├── adapters/      Layer 3: port implementations (driven) and delivery mechanisms (driving)
└── entrypoints/   Layer 4: composition roots (binaries)
```

## Dependency matrix

✔ = allowed; blank = forbidden. Enforced by `tests/architecture` in required CI (FND-05).

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

External crates forbidden in `domain/*` and `application/*`: `rusqlite`, `tree-sitter*`, `serde_json`, `tokio` (or any async runtime), process/IPC crates.

## Extending

- **New language:** add a driven adapter implementing `LanguageParser` (and optionally `SemanticResolver`). No domain change.
- **New host (for example an MCP server):** add a driving adapter that calls `codeatlas-application` use cases, wired from `codeatlas-cli` or a new entrypoint.
- **New storage back end:** add a driven adapter implementing the storage ports that passes the `contracts/fixtures` behaviour tests.

Decision record: [ADR-0010](../docs/adr/0010-engine-clean-architecture-layering.md).
