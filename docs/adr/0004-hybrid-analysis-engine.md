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
- ENG-02/03: Tree-sitter parsing and symbol extraction, covered by golden snapshots and stable IDs.
- ENG-04: semantic resolution tests on SynchroStream fixtures.
- ENG-05: graph invariant tests, where every edge has provenance and status.
- IMP-01: change impact distinguishes evidence classes.

## Related records

- Architecture Blueprint v1.3 §1.1, §5, §8
- Traceable Implementation Plan v1.2: FND-04, ENG-01..05, IMP-01
- [ADR-0002](0002-polyglot-architecture.md) (polyglot), [ADR-0008](https://github.com/Khoiiminh/codeatlas-platform/blob/development/docs/adr/0008-evidence-first-ai.md) (evidence-first AI), [ADR-0010](0010-engine-clean-architecture-layering.md) (engine layering)
