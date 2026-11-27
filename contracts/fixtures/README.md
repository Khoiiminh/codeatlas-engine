# Contract fixtures

- `examples/<Type>/`: schema examples for each named type in [`../schemas/codeatlas.schema.json`](../schemas/codeatlas.schema.json) (FND-03). Files named `valid-*.json` must validate and `invalid-*.json` must not. Each folder name is a `$defs` entry with a matching Rust type in `codeatlas-protocol`. `crates/adapters/codeatlas-protocol/tests/contract_compatibility.rs` checks all three: the folder name, the valid and invalid files, and the Rust round trip. `packages/contracts` type-checks every valid example against its generated TypeScript type.
- [`ports/`](ports/README.md): **port behaviour scenarios** for snapshot and graph storage (FND-04). Language-neutral scenarios that both the engine SQLite adapter (LOC-01) and the platform PostgreSQL adapter (HOST-02) run as contract tests (ADR-0006). `tests/fixtures` (`codeatlas-fixture-tests`) validates them and runs them against an in-memory reference store. They are released with the contract package (ADR-0009).

See [`../README.md`](../README.md) for the contract rules and versioning.
