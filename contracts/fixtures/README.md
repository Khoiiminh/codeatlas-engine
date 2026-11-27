# Contract fixtures

- `examples/<Type>/`: schema examples for each named type in [`../schemas/codeatlas.schema.json`](../schemas/codeatlas.schema.json) (FND-03). Files named `valid-*.json` must validate and `invalid-*.json` must not. Each folder name is a `$defs` entry with a matching Rust type in `codeatlas-protocol`. `crates/adapters/codeatlas-protocol/tests/contract_compatibility.rs` checks all three: the folder name, the valid and invalid files, and the Rust round trip. `packages/contracts` type-checks every valid example against its generated TypeScript type.
- **Port behaviour expectations** for snapshot and graph storage (FND-04): language-neutral fixtures that both the engine SQLite adapter and the platform PostgreSQL adapter run as contract tests (ADR-0006, ADR-0010). Released with the contract package (ADR-0009).

See [`../README.md`](../README.md) for the contract rules and versioning.
