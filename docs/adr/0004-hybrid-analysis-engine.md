# ADR-0004: Hybrid analysis engine
- Status: Accepted
- Date: 2026-11-25
- Decision owners: Engine maintainers
- Repository scope: engine
- Supersedes: none
- Superseded by: none

## Context

The engine must turn a repository into a graph of files, symbols and relationships (imports, calls and references). Two levels of understanding are needed:

- **Structure:** what symbols and syntax a file contains. This must work across many languages, quickly, and on files that do not compile yet.
- **Semantics:** what an import or call actually resolves to. For TypeScript and JavaScript, this depends on `tsconfig.json`, path aliases, module resolution and types.

## Problem

A syntax parser alone guesses at resolution and produces wrong or missing edges. A compiler alone is slow, works for one language only, and fails on broken projects. Neither on its own produces a trustworthy, explainable graph.

## Decision

Combine **two analysis sources** and merge them with explicit provenance:

1. **Tree-sitter** provides structural parsing: fast, error-tolerant and multi-language. It lives in the `codeatlas-parser-treesitter` adapter behind the `LanguageParser` port.
2. **The TypeScript Compiler API** provides project-aware semantic resolution for TypeScript and JavaScript. It runs in `packages/typescript-analyzer`, a Node process reached through `codeatlas-semantic-typescript` behind the `SemanticResolver` port.
3. Results are normalized into one graph in the domain crates (`codeatlas-core`, `codeatlas-graph`). **Every relationship records its evidence:** source, location and classification (for example semantic, syntactic, inferred or unresolved). Evidence-precedence rules live in `codeatlas-core`.
4. Unresolved references are represented explicitly and are never silently dropped or guessed.
5. Analysis is **static only**. The engine never executes repository scripts and never installs dependencies.
6. Initial languages are TypeScript, JavaScript, TSX and JSX. Further languages and framework analyzers are added as driven adapters behind the same ports. Runtime tracing is deferred.

## Alternatives considered

- **Tree-sitter only.** Fast and multi-language, but import and symbol resolution is imprecise for TypeScript projects. Rejected as the sole source.
- **The TypeScript Compiler API only.** Precise, but TypeScript-only, slower, and fragile on broken projects. Rejected as the sole source.
- **Language Server Protocol servers.** One heavy process per language, with responses shaped for interactive editing rather than whole-repository batch output, so a poor fit for batch analysis. Not adopted initially.
- **Runtime tracing.** Gives dynamic accuracy, but requires executing code. Deferred (Blueprint v1.3 §8).

## Consequences

- Positive: accurate TypeScript edges, a multi-language structural baseline, and every edge explainable through its evidence.
- Negative: two pipelines to merge, plus a Node process dependency for semantic analysis.
- Mitigation: ports keep both sources replaceable, and golden fixture tests (FND-04) lock the merged output.

## Implementation and verification

- FND-04 (2026-11-27): the SynchroStream golden fixture in [`tests/fixtures`](../../tests/fixtures/README.md) lists every file, symbol, relationship and unresolved reference (with reasons) that a complete analysis must report, and the conventions that define them. TypeScript 6.0.3 cross-checks it in CI: import resolution, call and reference targets, and an exhaustive symbol list.
- ENG-01: deterministic scanner inventory that ignores generated and dependency folders.
  - Delivered 2026-11-27. The rules live in the domain (`codeatlas-core::source`): language by exact lower-case extension (`.ts .mts .cts .tsx .js .mjs .cjs .jsx`, never declaration files), and the default ignore policy (dependency, build-output, cache and VCS folder names at any depth, adjustable per scan).
  - The `SourceScanner` port and the `ScanRepository` use case are in `codeatlas-application`. The filesystem adapter `codeatlas-scanner-fs` honours only `.gitignore` files inside the root (no parent, global or user excludes), never follows symlinks, and reports skipped folders and unusable source files (too large, not UTF-8, unreadable, symlink) instead of failing.
  - Verified: the scanner reproduces the `files` and `ignoredPaths` of every FND-04 fixture exactly, its SHA-256 hashes match those in the FND-04 seed snapshots (computed independently), and planted defects (no `dist` rule, no `.mts`, `.gitignore` off, symlinks followed, wrong hash input) each fail a test.
- ENG-02/03: Tree-sitter parsing and symbol extraction, covered by golden snapshots and stable IDs.
  - ENG-02 delivered 2026-11-28. Parsing never fails because of syntax errors. The parser recovers and records each place it did as a `SyntaxProblem` (*unexpected* text, or a *missing* token). The rest of the file keeps its parse, and the problems become `CA1001` warnings. A file that cannot be parsed at all (unreadable, changed since the scan, too large, timed out, or a parser crash) is a `CA1002` warning, and the run continues with the next file. The codes are registered in `codeatlas-core::diagnostic`.
  - Positions use the contract's units: zero-based lines, characters in UTF-16 code units, line breaks `
`, `
` and ``, and a leading byte order mark not counted (`codeatlas-core::text`). ENG-03 builds its source spans on these types.
  - Ports in `codeatlas-application`: `LanguageParser`, `SourceReader` (returns a file only if its size and SHA-256 still match the inventory), and the `ParseSources` use case, which catches parser panics. The adapter `codeatlas-parser-treesitter` uses the `typescript`, `tsx` and `javascript` grammars. JSX is accepted in `.js`, as bundlers do, and rejected in `.ts`, as `tsc` does. Each file has a default limit of 100 recorded problems and 10 seconds.
  - Verified: 17 parser cases with exact expected problems (`crates/adapters/codeatlas-parser-treesitter/tests/cases`). The TypeScript compiler agrees on which of them have syntax errors (`npm run check:parser-cases`, CI job `typescript-analyzer`). Every FND-04 fixture file parses cleanly end to end, a repository with broken files still yields a full report, and nesting 100,000 levels deep, arbitrary text, the problem limit and the timeout are tested. Ten planted defects each fail a test: byte columns, the BOM kept, missing tokens dropped, wrong grammar for TSX and for JavaScript, the limit ignored, the timeout ignored, a panic not caught, a reader that checks size but not hash, and `
` counted as two breaks.
- ENG-04: semantic resolution tests on SynchroStream fixtures.
- ENG-05: graph invariant tests, where every edge has provenance and status.
- IMP-01: change impact distinguishes evidence classes.

## Related records

- Architecture Blueprint v1.3 §1.1, §5, §8
- Traceable Implementation Plan v1.2: FND-04, ENG-01..05, IMP-01
- [ADR-0002](0002-polyglot-architecture.md) (polyglot), [ADR-0008](https://github.com/Khoiiminh/codeatlas-platform/blob/development/docs/adr/0008-evidence-first-ai.md) (evidence-first AI), [ADR-0010](0010-engine-clean-architecture-layering.md) (engine layering)
