# ADR-0015: TypeScript semantic analysis: facts, analyzer process and project model
- Status: Accepted
- Date: 2026-11-28
- Decision owners: Engine maintainers
- Repository scope: engine
- Supersedes: none. It makes concrete the semantic half of [ADR-0004](0004-hybrid-analysis-engine.md) decision 2, and [ADR-0010](0010-engine-clean-architecture-layering.md) rule 6 (the analyzer is an out-of-process adapter).
- Superseded by: none

## Context

ADR-0004 combines two analysis sources: Tree-sitter for structure (ENG-02, ENG-03) and the TypeScript Compiler API for what an import or a name actually resolves to. ENG-04 builds the second one. It runs in `packages/typescript-analyzer`, a Node program, behind the `SemanticResolver` port. ADR-0014 adds three constraints: every long use case can be cancelled within 1 s, it reports progress, and a full analysis of 1 million lines stays within 30 s and 3 GB, including this semantic pass. The architecture review before ENG-04 also found that a 3,000-file import chain overflows the compiler's stack.

## Problem

1. **Identity.** ENG-03 gives every symbol a stable id from `codeatlas-core`. If the compiler side also named symbols, the engine would have two implementations of the identity rules at run time, and they would drift.
2. **A second runtime.** The analyzer is a separate process in another language. It can be missing, the wrong version, crash, run out of memory, hang, or be slower than the user's patience, and none of that may stop or corrupt an analysis.
3. **Projects.** In a real repository the meaning of an import depends on which `tsconfig.json` owns the file: path aliases, module resolution and the JSX mode differ between packages. One program for the whole repository gets aliases wrong and needs memory for everything at once.
4. **What counts.** The golden files (FND-04) fix what a call, a reference, a heritage edge and an unresolved reference are. A producer must reproduce them exactly, and the rules must be written down.

## Decision

1. **The analyzer reports places, not ids.** For each file it reports `SemanticFacts`: the modules the file names (specifier, position, type-only flag, target) and the declarations each call, reference and heritage clause reaches. A reached declaration is the file and the start of its name (`DeclarationSite`). `codeatlas-core::link` turns facts into `Relationship`s using the ENG-03 symbols:
   - the target is the symbol declared at that name; a declaration that is no symbol (a local, an enum member, an object-literal member, a declaration file) is no relationship;
   - the source of an import is its file; of anything else, the innermost symbol whose declaration contains it, else the file.

   Identity therefore has one owner. `scripts/symbol-rules.ts` stays a test oracle only.
2. **The port and the use case.** `SemanticResolver::resolve(root, files, run)` returns a `SemanticOutput`: resolved files with the SHA-256 of what was analysed, failed files with a reason, and configuration problems. `ResolveSemantics` owns the rules that keep it trustworthy:
   - a result for different content than the inventory is a failure (`ChangedSinceScan`);
   - every inventory file ends up linked or failed exactly once, including a file the resolver forgot;
   - an analyzer that cannot run is one `CA2001` warning and an empty result, not a failed run, so ENG-05 can fall back to syntactic evidence;
   - a failed file is a `CA2001` warning with its location, and a configuration problem is a `CA2002` warning.
3. **One analyzer process per call, protocol version 1.** `codeatlas-semantic-typescript` starts `node packages/typescript-analyzer/src/main.ts`; Node 26 runs the TypeScript source directly. It writes one JSON request to stdin and reads one JSON message per line from stdout:
   - `hello` with the protocol version comes first;
   - then `project`, `problem`, `file` and `failed` messages;
   - last `done`, or `fatal`.

   stderr is free text for people. A different protocol version, an unknown message type or a malformed value is never guessed at. The message types are written twice, in `src/protocol/messages.ts` and `src/wire.rs`, and change together with `PROTOCOL_VERSION`. The analyzer protocol is private to this adapter, not a canonical contract (ADR-0010 rule 4 covers the canonical ones). How the analyzer ships, as bundled JavaScript with a pinned Node or with the user's Node, is decided in DIST-01.
4. **Failures keep what is known.** These failures are `SemanticError::Unavailable`:
   - the program cannot start;
   - the analyzer exits before `hello`;
   - it speaks another protocol version.

   An analyzer that crashes, reports `fatal`, sends an invalid message or makes no progress for the stall timeout (default 300 s, because building a large program is one long silent step) is killed. Files it already reported keep their results; every other file fails with the reason, including the tail of stderr. Inside the analyzer, an exception in one project fails the rest of that project only, since the checker's state is then unknown. The next project gets a new program.
5. **Cancellation kills the process.** The compiler is synchronous and cannot read a cancel message while it works, so the adapter polls the run's token every 50 ms while it waits for output and kills the analyzer when the run is cancelled. That meets ADR-0014's 1 s budget at any point, even inside one long compiler step.
6. **Project model.** A file belongs to the nearest `tsconfig.json` or `jsconfig.json` above it, inside the root, that includes it, as in an editor:
   - a configuration that does not include the file is followed through its project references (a solution `tsconfig.json` with `"files": []`), then the search goes on upwards;
   - a `jsconfig.json` starts from the defaults tsserver gives it;
   - files that no configuration includes form one inferred project: JavaScript allowed, `bundler` resolution, `ESNext` (`INFERRED_OPTIONS` in `projects.ts`);
   - references are used to find owners only and are not passed to the compiler, so imports resolve to the referenced project's source, as an editor does, not to build output that may not exist.

   Projects are analysed one at a time, in configuration order, and each program is released before the next is built, so memory follows the largest project, not the repository. The parsed default library is shared between programs.
7. **Stack and memory.** The analysis runs on a worker thread whose stack the analyzer sets (256 MiB by default): the main thread's stack is fixed when Node starts, and the type checker recurses once per file along an import chain. The thread's heap can be limited too, and running out of it is a `fatal` message, not a crash of the whole process.
8. **What counts** (the full conventions are in `tests/fixtures/README.md`, and the walker's rules in `src/analysis/references.ts`).

   Modules: imports, `import … = require()`, `require()`, `import()` (computed specifiers are `dynamic`), type `import()`, and re-exports. A module target is one of:
   - a scanned source file (a declaration file stands for its scanned JavaScript source);
   - external, for packages and `node:` built-ins, installed or not, because the engine never installs dependencies;
   - `unsupported`, when the module exists but is not scanned source;
   - `not_found`, for a relative path, a `paths` alias or a specifier that cannot be a package name (`@/x`, `~x`), when it leads nowhere.

   References: a call reaches its callee, `super(…)` the base class, a value use or a JSX tag its declaration, and a heritage clause its base. A qualified name through a namespace counts as its final name; `Enum.Member` counts as the enum. Member reads, declaration names, type positions and export lists are not references.

   Unresolved: a name nobody declares, a name imported from a module that is not found, and a name two `export *` sources export with different declarations (`ambiguous`). A name imported from a package or a JSON module is not ours to judge and is not reported.
9. **Static only.** The analyzer builds programs with `ts.createProgram`, which loads no language-service plugins and runs nothing from the repository. It reads only the files the compiler asks for, and the scanned files are hashed as they are read.

## Alternatives considered

- **The analyzer computes symbol ids** (with `symbol-rules.ts`). Simpler linking, but two implementations of the identity rules at run time, in two languages. Rejected; positional linking keeps one owner.
- **A long-lived analyzer with the language service** (incremental programs, file watching). Needed for RT-03's 1 s re-analysis of one changed file, but not for a full analysis. Deferred to RT-03: the protocol starts with `hello` and a request, so a server mode can be added as protocol version 2.
- **Sending file contents over stdin.** It would guarantee that the analyser sees the scanned bytes, but doubles the data in flight and memory. Rejected: the analyzer hashes what it reads, and `ResolveSemantics` rejects any result whose hash differs.
- **tsserver or an LSP server.** Interactive, one request per position, with no whole-program batch output (ADR-0004). Rejected.
- **One program for the whole repository.** Wrong options for every package that has its own aliases, and memory for everything at once. Rejected.
- **Compiling against referenced projects' build output** (passing `references` to the compiler). The output usually does not exist in a checkout, and editors resolve to sources. Rejected.
- **Several analyzer processes in parallel**, one per project. Faster on monorepos, but memory multiplies with each program. Deferred until a monorepo benchmark shows the need; projects are already independent, so it is a change inside the adapter.

## Consequences

- Positive:
  - imports, calls, references and heritage are resolved by the compiler, per project, with aliases;
  - every relationship has a source position and every gap a reason;
  - an analyzer problem degrades the result instead of failing it;
  - cancellation and progress work as ADR-0014 requires;
  - the semantic pass fits the full-analysis budget with room to spare (below).
- Negative:
  - the engine needs Node 26 next to it;
  - each call starts a process (about half a second before the first file);
  - there is no incremental re-analysis yet;
  - a declaration in a file that failed to parse has no symbol, so references to it are dropped;
  - module and reference rules are written twice (the analyzer and the golden conventions).
- Mitigation:
  - DIST-01 decides how Node ships;
  - RT-03 adds the server mode;
  - failed files are reported as diagnostics;
  - the golden tests and the TypeScript cross-check (`npm run check:fixtures`) fail as soon as the analyzer, the conventions and the compiler disagree.

## Implementation and verification

- 2026-11-28 (ENG-04):
  - `codeatlas-core::semantic`: `SemanticFacts`, `ModuleReference`, `SymbolReference`, `DeclarationSite`, `SymbolTable` and `link`. The diagnostic codes `CA2001` and `CA2002` are registered in `codeatlas-core::diagnostic`.
  - `codeatlas-application::semantic`: `SemanticResolver`, `ResolveSemantics` and `SemanticReport`, plus `Phase::Resolve`.
  - `codeatlas-semantic-typescript`: `TypeScriptResolver`, `AnalyzerCommand` and `AnalyzerSettings`; the wire protocol is in `src/wire.rs`.
  - `packages/typescript-analyzer/src`: `main.ts` (hello, the request and the worker thread), plus `analysis/` for projects, the compiler host, module targets, the reference walker and positions, and `protocol/messages.ts`. `typescript` is now a run-time dependency of the package.
- Tests:
  - core unit tests for linking: innermost source, the first symbol among several sites, unmatched sites dropped, order independence, nested declarations;
  - use-case tests with a fake resolver: hash mismatch, forgotten and duplicate files, unavailable analyzer, configuration problems, cancellation, progress;
  - process tests with fake analyzers: missing runtime, another protocol version, a crash before `hello`, a crash, `fatal`, an invalid message, a stall, cancellation killing the process within the budget, progress per file;
  - real-analyzer tests: UTF-16 positions with a byte order mark, CRLF, a lone CR and U+2028 (not a line break for the engine, unlike TypeScript); a 3,000-file import chain on the default stack, and the same chain overflowing on a 1 MiB stack; configuration problems; an unreadable file. On the Windows development machine the chain takes about 8 s when its files have been read before, and up to about 60 s right after they are written: the antivirus cost of reading new files that ADR-0014 measured, not the analyzer's;
  - end to end on both FND-04 fixtures: every edge and unresolved reference reproduced exactly with ids, specifiers and reasons, each pointing into its golden text, and the external modules exact;
  - seven rule cases the fixtures do not contain: member reads and export lists; namespaces and enums; every module target; CommonJS export forms; the `export *` ambiguity rules; decorators, mixins and `super`; shorthand, instantiation expressions and undeclared names.
- 19 planted defects each fail a test. The first sweep missed one: not following shorthand members, because TypeScript already binds `module.exports = { a }` as aliases; a call through an ordinary object's shorthand member was added. A review then found that ambiguity was checked for named imports only; a call through a namespace import (`all.same()`) was added, with its defect.
  - In the analyzer: member reads counted as references; no `export *` ambiguity check, for named imports and through a namespace import; names from packages reported as not found; shorthand members not followed; export lists counted as references; `super(…)` not resolved; a declaration file not standing for its JavaScript source; every unresolved bare specifier external; project references not followed; configurations ignored; a lone CR not a line break; U+2028 a line break.
  - In core: the outermost instead of the innermost source; the first site winning when it is no symbol.
  - In the use case: a result for different content accepted; forgotten files not failed.
  - In the adapter: cancellation not killing the analyzer; a crash discarding what was already reported.
- The `monorepo` fixture (grown from `monorepo-minimal`) covers the project model:
  - a base configuration through `extends`;
  - two packages in which the same specifier `@/lib/format` reaches different files;
  - a solution configuration whose references own the files;
  - a `jsconfig.json` CommonJS package;
  - two files that only the inferred project owns;
  - an alias that exists in another project only, so it is `not_found`.
- Benchmark (ADR-0014) on the development machine, 1,000,330 generated lines in 2,995 files:
  - semantic pass 7.88 s on one process, analyzer peak 1.55 GB, 152,745 relationships (exactly 51 per file, as the benchmark asserts);
  - scan, parse and semantic pass 8.56 s, within the 30 s and 3 GB budget.
- CI: the `rust`, `portability` and `benchmark` jobs install Node and the analyzer, so the semantic tests run on Linux, Windows and macOS. A test that cannot run the analyzer skips on a developer's machine and fails in CI (ADR-0014 decision 7).
- RT-03: a server mode (protocol version 2) with incremental programs.
- DIST-01: how the analyzer and Node ship with the engine.

## Related records

- [ADR-0004](0004-hybrid-analysis-engine.md) (hybrid analysis), [ADR-0005](0005-inter-process-communication.md) (stdio processes), [ADR-0010](0010-engine-clean-architecture-layering.md) (layers, rule 6), [ADR-0014](0014-engine-run-model-and-quality-targets.md) (run model and budgets)
- Traceable Implementation Plan v1.2: ENG-04, ENG-05, RT-03, DIST-01
