# typescript-analyzer

**Layer:** Adapter (driven), out of process.

A Node program that uses the TypeScript Compiler API for project-aware import and symbol resolution. The Rust crate `crates/adapters/codeatlas-semantic-typescript` spawns it and maps its output into domain semantic records.

Internal layout (kept light):

```text
src/
├── protocol/   # message types exchanged with the Rust adapter
├── analysis/   # Compiler API program/host setup and resolution logic
└── main.ts     # stdio entry point
```

Rules: it never executes repository scripts or installs dependencies, and it writes diagnostics to stderr only.
