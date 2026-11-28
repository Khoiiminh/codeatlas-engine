# ADR-0014: Engine run model (cancellation, progress, threads) and quality targets
- Status: Accepted
- Date: 2026-11-28
- Decision owners: Engine maintainers
- Repository scope: engine
- Supersedes: none. It makes concrete the `CancellationToken` and `ProgressReporter` ports listed in [ADR-0010](0010-engine-clean-architecture-layering.md) rule 2, and the application side of [ADR-0005](0005-inter-process-communication.md) decision 4.
- Superseded by: none

## Context

ENG-01 to ENG-03 built the first two use cases, `ScanRepository` and `ParseSources`, and their ports. An architecture review before ENG-04 measured the engine on a generated TypeScript repository of 1 million lines in 3,000 files (on a 20-thread development machine, files already read once):

| Step | Time | Note |
|---|---|---|
| Scan (walk and SHA-256) | 0.25 s | |
| Parse and extract symbols | 5.3 s | on 1 of 20 threads |
| TypeScript compiler (`tsc --noEmit`) | 10.2 s, 1.4 GB | single-threaded; what ENG-04's adapter will drive |

Two side findings: on Windows, the first read of freshly written files was about 300 times slower (antivirus scanning new files), for our scanner and the compiler alike; and a 3,000-file import chain overflowed the TypeScript compiler's stack.

## Problem

1. ADR-0005 requires long operations to report progress and accept cancellation, and ADR-0010 names the ports, but no port or use case had them. Every later use case (ENG-04's semantic pass, ENG-05's graph build) would have had to be retrofitted, together with every adapter behind it.
2. The ports did not promise to be safe to share between threads, so no use case could parallelise. Adding that promise after ENG-04 builds a process-backed adapter would break it.
3. Nothing stated how large a repository the engine must handle, or how fast. "It scales" could not be tested.

## Decision

1. **Run context.** Every use case that can take long receives a `RunContext` (`codeatlas-application::run`): a `CancellationToken`, a `ProgressReporter`, and a worker count. The context is passed down to ports that can take long: `SourceScanner::scan` and `LanguageParser::parse` now receive it or its token.
2. **Cooperative cancellation.** Use cases check between units of work (a file, a directory entry). A parser checks inside a file too: the Tree-sitter adapter checks in the progress callback that already enforces the time limit, so one slow file cannot delay cancellation. A cancelled run returns `Cancelled` (or `ScanError::Cancelled`) and nothing else; a partial inventory or report is never returned as if complete. A parser that reports `Cancelled` when the run was not cancelled has a bug; that file is recorded as failed and the run goes on.
3. **Progress** is a `Progress { phase, done, total }`, reported once per unit of work, with `total` when it is known in advance (parsing knows it from the inventory; scanning does not). Reporters are called from worker threads and must be cheap; the runtime (RT-02) throttles and maps them to the contract's `begin` / `report` / `end` notifications, with a percentage when `total` is known.
4. **Threads.** Port traits require `Send + Sync`. Parallel work lives in use cases, built on `std::thread::scope`: no thread pool or async runtime enters the inner layers (ADR-0010 rule 3 is unchanged). Workers take the next item from a shared counter, and results are put back in input order, so **a result never depends on the worker count**. The count comes from the composition root (CLI, runtime), which asks the operating system; the application layer reads nothing from its environment, and a run defaults to one thread.
5. **Quality targets.** These budgets apply on the reference machine: 8 logical cores, SSD, files already read once. They are first estimates from the measurements above, to be tightened as features land; a budget changes only through this ADR's implementation section or a superseding ADR.

   | Budget | Target | Checked by |
   |---|---|---|
   | Scan, parse and symbols for 1 million lines | 3 s or less | the `benchmark` CI job (from this ADR) |
   | Full analysis of 1 million lines, including the TypeScript semantic pass | 30 s or less, peak memory 3 GB or less | ENG-04 and ENG-05 extend the benchmark |
   | Re-analysis after one file changes | 1 s or less | RT-03 |
   | Graph query on a 1-million-line snapshot | 100 ms or less for 95% of queries | RT-01 and RT-02 |
   | Cancellation takes effect | within 1 s | the tests in this ADR show it stops between files and inside one; RT-02 measures the time end to end |

6. **Benchmark.** `tests/fixtures/tests/benchmark.rs` generates a deterministic TypeScript repository (1 million lines by default, `CODEATLAS_BENCH_LINES` to change it) with a shallow import tree, times scanning and parsing on one thread and on all threads, and fails if the two reports differ or the symbol count is wrong. The CI job `benchmark` runs it on every pull request and writes the table to the job summary. It is **report-only**, not a required check, because shared CI machines vary too much for a fair time limit.
7. **Portability** (added 2026-11-28, ARC-02). The engine ships with a VS Code extension that runs on Windows, macOS and Linux, so its whole test suite must pass on all three. `rust` runs it on Linux, and the matrix job `portability` runs it on Windows and macOS with warnings as errors; all three are required checks. A test that needs something a system lacks (symbolic links on Windows without Developer Mode, for example) may skip on a developer's machine, never in CI. DIST-01 builds the release binaries for the same systems.

## Alternatives considered

- **Add cancellation and progress when the runtime needs them (RT-02).** Cheapest today, but every use case and long-running port written until then would change its signature. Rejected: the cost grows with each work item.
- **A thread pool crate (`rayon`) in the application layer.** Less code, work stealing. It adds a dependency to an inner layer for something `std::thread::scope` and an atomic counter do for whole files. Rejected for now; reconsider if finer-grained parallelism is needed.
- **An async runtime (`tokio`) for concurrency.** The work is CPU-bound, and ADR-0010 keeps async runtimes out of the inner layers. Rejected.
- **Streaming results to a sink instead of returning a report.** Parse results hold about 100,000 symbols per million lines; that is an estimated few tens of megabytes, small next to the compiler's 1.4 GB, but not yet measured. Deferred to ENG-05, which designs how the graph is built and measures memory.
- **Time limits as required CI checks.** Unfair on shared runners and flaky. Rejected in favour of a report-only job; budgets are judged on the reference machine.

## Consequences

- Positive: the VS Code client will be able to stop an analysis and show its progress; parsing uses every core (11.7 times faster on 20 threads, below); ENG-04's port is designed with cancellation, progress and thread safety from its first line; scale claims are measurable.
- Negative: every long port takes one more parameter; adapters must be thread-safe, so a stateful adapter (such as ENG-04's out-of-process TypeScript analyzer) must manage its own synchronisation; parallel runs are harder to debug than sequential ones.
- Mitigation: `RunContext::detached()` keeps tests and tools simple; determinism is tested for every thread count from 1 to 64, so a parallel bug shows up as a test failure, not as a different result.

## Implementation and verification

- 2026-11-28 (ARC-01, before ENG-04):
  - `codeatlas-application::run`: `RunContext`, `CancellationToken`, `ProgressReporter`, `Progress`, `Phase`, `Cancelled`, and the ready-made `CancelFlag`, `NeverCancelled` and `NoProgress`. `SourceScanner`, `SourceReader` and `LanguageParser` require `Send + Sync`.
  - `ScanRepository::execute` and `SourceScanner::scan` take the run; `FsSourceScanner` checks it between directory entries and reports each source file found.
  - `ParseSources::execute` takes the run, parses on up to `workers` threads, and returns `Result<ParseReport, Cancelled>`. `LanguageParser::parse` takes the cancellation token; `TreeSitterParser` checks it before parsing and in Tree-sitter's progress callback.
  - Tests:
    - the same report for 1, 2, 3, 8 and 64 threads (unit fakes), and for 1, 2, 7 and 32 threads on both FND-04 fixtures;
    - progress counts every file exactly once from every thread;
    - work really is spread over several threads;
    - a cancelled run returns no report, cancelling mid-run stops it, and no file starts after the cancellation even if the parser ignores the token;
    - a parser's own `Cancelled` fails only that file;
    - the Tree-sitter adapter stops in the middle of a 20,000-line file;
    - a cancelled scan returns `ScanError::Cancelled`, and scan progress counts source files.
  - Eight planted defects each fail a test: results left in completion order, the worker count ignored, no cancellation check between files (first missed, which added the test for a parser that ignores the token), a parser's own `Cancelled` stopping the run, a partial report returned on cancel, Tree-sitter or the scanner ignoring cancellation, and the scanner reporting no progress.
  - Benchmark on the development machine (20 threads), 1,000,330 lines in 2,995 files: scan 0.23 s; parse and symbols 5.42 s on 1 thread, 0.46 s on 20 threads (11.7 times faster); scan and parse 0.69 s, within the 3 s budget.
- 2026-11-28 (ARC-02, decision 7): the job `portability` in `.github/workflows/ci.yml` runs `cargo test --workspace` on `windows-latest` and `macos-latest` with `RUSTFLAGS=-D warnings` and reports the checks `portability (windows-latest)` and `portability (macos-latest)`; it is a separate job so that the required check `rust` keeps its name. The scanner's symlink test now fails in CI (where `CI` is set) instead of skipping when links cannot be created. Before the push, the job's command passed on the Windows 11 development machine with `core.autocrlf=true`, as in a fresh Windows checkout, and `CI` set (111 tests passed and 1 ignored, the benchmark; no warnings; the symlink test ran with real links). macOS is first checked by the pull request's CI. When ENG-04's tests start Node, the job installs Node as `typescript-analyzer` does.
- ENG-04: the `SemanticResolver` port takes the run context. One TypeScript program per `tsconfig` project bounds memory; Node runs with a larger stack for deep import chains; the semantic pass joins the benchmark.
- RT-02: the runtime maps `Progress` to `$/progress` notifications and `$/cancelRequest` to a `CancelFlag`, and checks the 1 s cancellation budget end to end.

## Related records

- [ADR-0005](0005-inter-process-communication.md) (progress and cancellation on the wire), [ADR-0010](0010-engine-clean-architecture-layering.md) (layers and ports), [ADR-0004](0004-hybrid-analysis-engine.md) (analysis pipeline)
- Traceable Implementation Plan v1.2: ARC-01, ENG-04, ENG-05, RT-02, RT-03
