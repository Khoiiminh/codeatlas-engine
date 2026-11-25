# ADR-0001: Multi-interface platform
- Status: Accepted
- Date: 2026-11-25
- Decision owners: Product architecture (engine, platform and VS Code maintainers)
- Repository scope: cross-repository
- Supersedes: none
- Superseded by: none

## Context

Developers explore a codebase in different places: in a terminal or CI job, inside the editor while they change code, and in a browser when a team reviews architecture together or asks questions about a repository. CodeAtlas has to serve all three without giving different answers for the same repository.

## Problem

If each interface builds its own analysis, the CLI, the editor and the web app drift apart. They report different symbols, edges and impact results. Every language or algorithm change then has to be made three times.

## Decision

CodeAtlas offers **three interfaces over one analysis engine**:

| Interface | Repository | Role |
|---|---|---|
| `codeatlas` CLI (`analyze`, query commands, `serve --stdio`) | `codeatlas-engine` | Local analysis, scripting and CI; hosts the local runtime |
| VS Code extension | `codeatlas-vscode` | Editor-native graph exploration and source navigation (ADR-0007) |
| Web application and API | `codeatlas-platform` | Hosted team features, AI exploration and repository connections |

Rules:

1. All analysis capabilities (scan, parse, resolve, build graph, query, change impact) are implemented once, in the engine's use cases (`AnalyzeRepository`, `QueryGraph`, `ComputeImpact`, `ManageSnapshots`).
2. Each interface is a driving adapter or a consumer of the engine's versioned contracts (ADR-0009). No interface re-implements analysis.
3. Interface-specific behaviour stays in the interface's repository: editor navigation in VS Code; users, tenancy and AI orchestration in the platform.
4. New interfaces, such as a future MCP server, are added as new driving adapters, not forks.

## Alternatives considered

- **VS Code extension only.** Fastest path to a demo, but it gives no CI or scripting use and no shared team view. Rejected.
- **Hosted web product only.** Needs source upload before any value is delivered, which conflicts with the local-first, privacy-preserving default (ADR-0003). Rejected.
- **A separate analysis implementation per interface.** Results diverge and the cost multiplies. Rejected.

## Consequences

- Positive: results are consistent across interfaces, analysis is improved once, and hosts can be added without touching the analysis logic.
- Negative: every interface depends on contract and protocol discipline. Version negotiation and compatibility tests are mandatory (ADR-0005, ADR-0009).
- Mitigation: canonical schemas owned by the engine (FND-03), plus cross-repository compatibility tests (INT-01).

## Implementation and verification

- CLI-01/02: the CLI exposes analysis and graph queries.
- IDE-01/02: the VS Code extension uses the same runtime through JSON-RPC.
- WEB-01: the platform web/API boundary consumes engine contracts and never parses source itself.
- INT-01: engine↔VS Code and engine↔platform-worker compatibility tests pass on pinned versions. Together with CLI-01/02, IDE-01/02 and WEB-01, this shows that the interfaces use one engine and do not re-implement analysis.

## Related records

- Architecture Blueprint v1.3 Purpose, §1, §3.1
- Traceable Implementation Plan v1.2: CLI-01/02, IDE-01/02, WEB-01, INT-01
- [ADR-0003](0003-hybrid-execution.md) (hybrid execution), [ADR-0005](0005-inter-process-communication.md) (IPC), [ADR-0007](https://github.com/Khoiiminh/codeatlas-vscode/blob/development/docs/adr/0007-vscode-extension.md) (VS Code extension), [ADR-0009](0009-multi-repository-architecture.md) (multi-repository)
