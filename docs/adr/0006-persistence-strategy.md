# ADR-0006: Persistence strategy
- Status: Accepted
- Date: 2026-11-25
- Decision owners: Engine maintainers (local adapter, port fixtures); platform maintainers (hosted adapters)
- Repository scope: cross-repository
- Supersedes: none
- Superseded by: none

## Context

Analysis produces snapshots: a graph of files, symbols and relationships with evidence, plus metadata and artifacts. Locally, they must be stored without any server. When hosted, they must be stored for many tenants with concurrent access, search and durable artifacts. Execution is hybrid (ADR-0003).

## Problem

One database cannot fit both settings well: a server database is too heavy for a laptop, and a file database cannot serve multi-tenant hosting. Two unrelated storage designs, on the other hand, would behave differently for the same snapshot.

## Decision

| Concern | Local (engine) | Hosted (platform) |
|---|---|---|
| Graph and snapshots | **SQLite**: `codeatlas-storage-sqlite` | **PostgreSQL**: `packages/persistence` |
| Semantic vectors | Deferred / optional | **pgvector**, only when justified |
| Artifacts | Local filesystem | **Cloudflare R2** |
| Job coordination | In-process runtime | **Redis + BullMQ** (`packages/messaging`) |

Rules:

1. Storage and job coordination are reached only through application-layer **ports**: `SnapshotStore`, `GraphReader` and `ArtifactStore` for storage, `JobQueue` for jobs, and `EmbeddingIndex` (deferred). Use cases never see a database or queue (ADR-0010, ADR-0011).
2. The **conceptual graph model and contracts are shared. Physical schemas are not**: each adapter has its own schema and migrations, and physical parity is not forced.
3. Both adapters pass the same **port behaviour fixtures**, kept by the engine in `contracts/fixtures` (FND-04). They are released as a versioned fixture bundle with each contract release (ADR-0009), and both adapters run them as contract tests.
4. Snapshots are **immutable and published atomically**. A failed analysis never replaces the last successful snapshot.
5. Local and hosted stores sync through APIs. **SQLite files are never copied into PostgreSQL.**

## Alternatives considered

- **PostgreSQL everywhere.** Requires a local server install, which is heavy for a CLI or extension. Rejected for local use.
- **SQLite everywhere.** Lacks multi-tenant concurrency and operational tooling for hosted use. Rejected for hosting.
- **A graph database (Neo4j).** A natural fit for graph queries, but it adds another operational system. Deferred until query evidence justifies it (Blueprint v1.3 §8).
- **One shared physical schema.** Couples the two adapters' migrations and blocks engine-specific optimizations. Rejected; behavioural parity is enforced instead.

## Consequences

- Positive: zero-setup local storage, hosted scalability, and replaceable adapters with verified equivalent behaviour.
- Negative: two persistence implementations and a shared fixture suite to maintain.
- Mitigation: the fixtures are owned and versioned in one place (engine `contracts/fixtures`), and mapping cost is reviewed at LOC-01.

## Implementation and verification

- FND-04: snapshot-port behaviour fixtures that both adapters can consume.
- LOC-01: SQLite migrations and round-trip tests, with the adapter passing the port fixtures.
- HOST-02: the PostgreSQL adapter passes the same fixtures, with no database-file sync.

## Related records

- Architecture Blueprint v1.3 §4, §4.1
- Traceable Implementation Plan v1.2: FND-04, LOC-01, HOST-02
- [ADR-0003](0003-hybrid-execution.md) (hybrid execution), [ADR-0010](0010-engine-clean-architecture-layering.md) (engine layering), [ADR-0011](https://github.com/Khoiiminh/codeatlas-platform/blob/development/docs/adr/0011-platform-clean-architecture-bounded-contexts.md) (platform layering)
