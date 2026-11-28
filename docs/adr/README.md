# Architecture Decision Records: index

ADR numbers are global across the three CodeAtlas repositories. Each ADR has one canonical file, in the repository that owns the decision. Other repositories list it here with a link and never keep copies. The full rules are in [ADR-0009](0009-multi-repository-architecture.md), under "Cross-repository ADR rules".

- A decision lives in the repository that owns it, including a cross-repository decision with a clear owner.
- A cross-repository decision with no single owner lives in `codeatlas-engine`.
- Links to ADRs in another repository point to that repository's `development` branch.

| ADR | Title | Scope | Status | Canonical location |
|---|---|---|---|---|
| ADR-0001 | [Multi-interface platform](0001-multi-interface-platform.md) | cross-repository | Accepted | this repository |
| ADR-0002 | [Polyglot architecture](0002-polyglot-architecture.md) | cross-repository | Accepted | this repository |
| ADR-0003 | [Hybrid execution](0003-hybrid-execution.md) | cross-repository | Accepted | this repository |
| ADR-0004 | [Hybrid analysis engine](0004-hybrid-analysis-engine.md) | engine | Accepted | this repository |
| ADR-0005 | [Inter-process communication](0005-inter-process-communication.md) | cross-repository | Accepted | this repository |
| ADR-0006 | [Persistence strategy](0006-persistence-strategy.md) | cross-repository | Accepted | this repository |
| ADR-0007 | [VS Code extension](https://github.com/Khoiiminh/codeatlas-vscode/blob/development/docs/adr/0007-vscode-extension.md) | vscode | Accepted | [`codeatlas-vscode`](https://github.com/Khoiiminh/codeatlas-vscode/blob/development/docs/adr/0007-vscode-extension.md) |
| ADR-0008 | [Evidence-first AI](https://github.com/Khoiiminh/codeatlas-platform/blob/development/docs/adr/0008-evidence-first-ai.md) | platform | Accepted | [`codeatlas-platform`](https://github.com/Khoiiminh/codeatlas-platform/blob/development/docs/adr/0008-evidence-first-ai.md) |
| ADR-0009 | [Multi-repository architecture and component ownership](0009-multi-repository-architecture.md) | cross-repository | Accepted | this repository |
| ADR-0010 | [Engine Clean Architecture layering and crate dependency rule](0010-engine-clean-architecture-layering.md) | engine | Accepted | this repository |
| ADR-0011 | [Platform Clean Architecture, bounded contexts and package dependency rule](https://github.com/Khoiiminh/codeatlas-platform/blob/development/docs/adr/0011-platform-clean-architecture-bounded-contexts.md) | platform | Accepted | [`codeatlas-platform`](https://github.com/Khoiiminh/codeatlas-platform/blob/development/docs/adr/0011-platform-clean-architecture-bounded-contexts.md) |
| ADR-0012 | [VS Code extension core/adapters/presentation layering](https://github.com/Khoiiminh/codeatlas-vscode/blob/development/docs/adr/0012-vscode-extension-layering.md) | vscode | Accepted | [`codeatlas-vscode`](https://github.com/Khoiiminh/codeatlas-vscode/blob/development/docs/adr/0012-vscode-extension-layering.md) |
| ADR-0013 | [Branch promotion and ownership of repository rules](0013-branch-promotion-and-rule-ownership.md) | cross-repository | Accepted | this repository |
| ADR-0014 | [Engine run model (cancellation, progress, threads) and quality targets](0014-engine-run-model-and-quality-targets.md) | engine | Accepted | this repository |
| ADR-0015 | [TypeScript semantic analysis: facts, analyzer process and project model](0015-typescript-semantic-analysis.md) | engine | Accepted | this repository |

## Writing and changing ADRs

- Use the mandatory template (Architecture Blueprint v1.3 §6.1): Status, Date, Decision owners, Repository scope, Supersedes / Superseded by, then Context, Problem, Decision, Alternatives considered, Consequences, Implementation and verification, Related records.
- Take the next free global number. Never reuse a number, even for a rejected ADR.
- `Proposed` ADRs are reviewed before implementation commits to a long-lived direction. Once `Accepted`, an ADR is not rewritten. A substantive change needs a new ADR that supersedes it, and the old ADR is then marked `Superseded` with a link to its successor.
- One exception keeps the layering ADRs (0010–0012) useful: their dependency matrices may be amended in place. The amendment is dated and reviewed, and it lands in the same pull request as the matching README and config copies.
- When an ADR is added or its status changes, update this index in **all three** repositories in the same change set.
- Link the work items, pull requests and tests that implement an ADR in its "Implementation and verification" section.
