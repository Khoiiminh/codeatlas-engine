# typescript-analyzer

**Layer:** Adapter (driven), out of process.

A Node program that uses the TypeScript Compiler API to resolve imports, calls, references and heritage clauses project by project. The Rust crate `crates/adapters/codeatlas-semantic-typescript` starts it, and `codeatlas-core::link` turns what it reports into relationships between symbol ids ([ADR-0015](../../docs/adr/0015-typescript-semantic-analysis.md)).

```text
src/
├── main.ts                 stdio entry: says hello, reads the request, runs the analysis on a worker thread
├── protocol/messages.ts    the messages exchanged with the Rust adapter (protocol version 1)
└── analysis/
    ├── worker.ts           the analysis thread (large stack, optional heap limit)
    ├── analyze.ts          one program per project, analysed and released in turn
    ├── projects.ts         which tsconfig.json or jsconfig.json owns each file; the inferred project
    ├── host.ts             compiler host: hashes scanned files as it reads them, shares the default library
    ├── inventory.ts        scanned files, and compiler file names back to repository paths
    ├── modules.ts          where a module specifier leads (file, external, or unresolved with a reason)
    ├── references.ts       the walker: what counts as a call, reference or heritage edge, and what it reaches
    └── positions.ts        positions in the engine's units (UTF-16 characters; \n, \r\n and \r only)
scripts/                    cross-checks of the golden files against the compiler (FND-04, ENG-02, ENG-03)
```

## Running it

Node 26 (`.node-version`) runs the TypeScript sources directly; there is no build step.

```sh
npm ci
npm run typecheck
echo '{"protocol":1,"root":"/path/to/repo","files":["src/a.ts"]}' | node src/main.ts
```

The output is one JSON message per line on stdout: `hello`, then `project`, `problem`, `file` and `failed` messages, then `done` (or `fatal`). Positions are zero-based, with characters in UTF-16 code units. A declaration is given by its file and the start of its name; the engine finds the symbol there.

The Rust tests that run it (`cargo test -p codeatlas-semantic-typescript`, `cargo test -p codeatlas-fixture-tests`) need `npm ci` here first. They find Node on `PATH`, or in `CODEATLAS_NODE`. Without either they skip on a developer's machine and fail in CI.

## Rules

- It never executes repository code or installs dependencies, and it writes only protocol messages to stdout; text for people goes to stderr.
- A file belongs to the nearest configuration above it that includes it, following solution references. Files no configuration includes form one inferred project. Project references are not passed to the compiler, so imports resolve to sources.
- What counts as a call, a reference, a heritage edge and an unresolved reference is listed in `src/analysis/references.ts` and in [`tests/fixtures/README.md`](../../tests/fixtures/README.md). Change them together with the golden files.
- Changing a message changes `PROTOCOL_VERSION` here and in `crates/adapters/codeatlas-semantic-typescript/src/wire.rs`.
