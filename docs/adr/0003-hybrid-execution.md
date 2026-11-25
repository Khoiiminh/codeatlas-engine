# ADR-0003: Hybrid execution
- Status: Accepted
- Date: 2026-11-25
- Decision owners: Product architecture (engine and platform maintainers)
- Repository scope: cross-repository
- Supersedes: none
- Superseded by: none

## Context

Many teams do not want their source code to leave developer machines. They still expect shared, hosted features: team workspaces, GitHub-connected repositories and AI exploration. Analysis must also work offline and give fast feedback inside the editor.

## Problem

A hosted-only product blocks privacy-sensitive users and adds latency. A local-only product cannot offer team or hosted AI features. Running two different analysis implementations, one local and one hosted, would make results diverge.

## Decision

CodeAtlas runs in **two execution modes over the same engine implementation**, and **local-first is the default**:

| Concern | Local (engine) | Hosted (platform) |
|---|---|---|
| Who runs analysis | CLI / `serve --stdio` runtime on the developer machine | Isolated worker invoking a **pinned** engine release or container |
| Repository content | Local working tree | Authorized, isolated worker snapshot |
| Persistence | SQLite | PostgreSQL (+ pgvector when justified), R2 artifacts |
| Job coordination | In-process runtime | Redis + BullMQ |

Rules:

1. Both modes run the same engine use cases. The platform never re-implements analysis (ADR-0001, ADR-0009).
2. Hosted orchestration, tenant policy, credentials and AI-provider calls stay outside the engine (Blueprint v1.3 §5).
3. Synchronization between local and hosted is **explicit**: the user selects snapshots, metadata and artifacts to sync through APIs. SQLite files are never copied into PostgreSQL.
4. Uploading source code is opt-in, and is explained separately from syncing metadata.
5. Snapshots are immutable. A snapshot's identity includes repository identity, commit or content fingerprint, schema version, engine version, configuration fingerprint, creation time and completion status. A new analysis creates a new snapshot. A failed run never replaces the last successful snapshot.
6. Hosted workers are isolated, never execute repository scripts and never install repository dependencies.

## Alternatives considered

- **Local-only.** Simplest and most private, but offers no team features or hosted AI. Rejected as the end state; it remains the first vertical slice.
- **Hosted-only.** Easier operations, but requires source upload, breaks offline use and is slower inside the editor. Rejected.
- **Separate local and hosted engines.** Results diverge and cost doubles. Rejected.

## Consequences

- Positive: a privacy-preserving default, one analysis implementation and a clear upgrade path to hosted features.
- Negative: two persistence adapters, snapshot-sync rules, and engine-version pinning in the platform.
- Mitigation: shared port behaviour fixtures (ADR-0006), and a pinned engine image (DIST-01, HOST-01).
- Open: conflict behaviour for divergent local and hosted snapshots is not yet specified (Blueprint v1.3 §4.1). It must be decided in a follow-up ADR before snapshot synchronization ships; the owner is HOST-02.

## Implementation and verification

- RT-03: snapshot freshness and incremental analysis, compared with a full-analysis baseline.
- HOST-01: the hosted job pipeline (202 enqueue, isolated worker, pinned engine version).
- HOST-03: cross-tenant tests, no script execution, and source logs disabled.

## Related records

- Architecture Blueprint v1.3 §3.3, §4, §4.1, §5
- Traceable Implementation Plan v1.2: RT-03, HOST-01, HOST-02, HOST-03
- [ADR-0001](0001-multi-interface-platform.md) (interfaces), [ADR-0006](0006-persistence-strategy.md) (persistence), [ADR-0009](0009-multi-repository-architecture.md) (multi-repository), [ADR-0011](https://github.com/Khoiiminh/codeatlas-platform/blob/development/docs/adr/0011-platform-clean-architecture-bounded-contexts.md) (platform layering)
