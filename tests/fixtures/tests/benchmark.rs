//! ADR-0014 benchmark: generates a synthetic TypeScript repository and times the engine on it.
//!
//!   cargo test --release -p codeatlas-fixture-tests --test benchmark -- --ignored --nocapture
//!
//! `CODEATLAS_BENCH_LINES` sets the size (default 1,000,000 lines). The CI job `benchmark` runs
//! it on every pull request and adds the table to the job summary. It reports and does not
//! gate: CI machines vary too much for a time limit to be fair. It does fail if the parallel and
//! the single-threaded reports differ, or if the symbol count is wrong.
//!
//! The repository is generated, not copied, so it is free of licences and secrets and has the
//! same shape every time. Each file imports a file whose number is a tenth of its own, so imports
//! form a shallow tree, as in real code: a 3,000-file import chain overflows the TypeScript
//! compiler's stack (found while writing ADR-0014).

use std::fmt::Write as _;
use std::fs;
use std::num::NonZeroUsize;
use std::path::Path;
use std::time::{Duration, Instant};

use codeatlas_application::{
    ParseLimits, ParseReport, ParseSources, RunContext, ScanOptions, ScanRepository,
};
use codeatlas_core::Inventory;
use codeatlas_parser_treesitter::TreeSitterParser;
use codeatlas_scanner_fs::{FsSourceReader, FsSourceScanner};

const DEFAULT_LINES: usize = 1_000_000;
const METHODS_PER_CLASS: usize = 24;
/// Per file: an interface with 3 properties, an enum, a class with a field, 2 parameter
/// properties and the methods, and a function.
const SYMBOLS_PER_FILE: usize = 4 + 1 + 1 + 1 + 2 + METHODS_PER_CLASS + 1;

/// ADR-0014 budget for scanning, parsing and extracting symbols, per million lines, on the
/// reference machine (8 logical cores, SSD, files already read once).
const BUDGET_PER_MILLION_LINES: Duration = Duration::from_secs(3);

fn file_text(i: usize) -> String {
    let import = i / 10;
    let mut methods = String::new();
    for m in 0..METHODS_PER_CLASS {
        let _ = write!(
            methods,
            "    method{m}(input: number[]): number {{
        let total = 0;
        for (const value of input) {{
            if (value > {m}) {{
                total += value * 2;
            }} else {{
                total -= value;
            }}
        }}
        this.state = total > 0 ? State{i}.Busy : State{i}.Idle;
        return total + this.options.retries;
    }}

"
        );
    }
    format!(
        "import {{ Service{import} }} from \"./mod{import}\";

export interface Options{i} {{
    readonly id: string;
    retries: number;
    label?: string;
}}

export enum State{i} {{
    Idle = \"idle\",
    Busy = \"busy\",
}}

export class Service{i} {{
    private state: State{i} = State{i}.Idle;
    constructor(private readonly options: Options{i}, private readonly next: Service{import}) {{}}

{methods}}}

export function create{i}(options: Options{i}, next: Service{import}): Service{i} {{
    return new Service{i}(options, next);
}}
"
    )
}

/// Writes about `lines` lines of TypeScript under `root/src`; returns (files, lines, bytes).
fn generate(root: &Path, lines: usize) -> (usize, usize, usize) {
    let src = root.join("src");
    fs::create_dir_all(&src).unwrap();
    let per_file = file_text(0).lines().count();
    let files = lines.div_ceil(per_file).max(1);
    let (mut total_lines, mut total_bytes) = (0, 0);
    for i in 0..files {
        let text = file_text(i);
        total_lines += text.lines().count();
        total_bytes += text.len();
        fs::write(src.join(format!("mod{i}.ts")), text).unwrap();
    }
    (files, total_lines, total_bytes)
}

fn scan(root: &Path) -> Inventory {
    ScanRepository::new(&FsSourceScanner)
        .execute(
            root.to_path_buf(),
            &ScanOptions::default(),
            &RunContext::detached(),
        )
        .unwrap()
}

fn parse(root: &Path, inventory: &Inventory, workers: NonZeroUsize) -> ParseReport {
    let run = RunContext::detached().with_workers(workers);
    ParseSources::new(&FsSourceReader, &TreeSitterParser)
        .execute(root, inventory, &ParseLimits::default(), &run)
        .unwrap()
}

fn timed<T>(f: impl FnOnce() -> T) -> (T, Duration) {
    let started = Instant::now();
    let value = f();
    (value, started.elapsed())
}

fn rate(lines: usize, time: Duration) -> String {
    let per_second = lines as f64 / time.as_secs_f64().max(1e-9);
    format!("{:.0}k lines/s", per_second / 1000.0)
}

#[test]
#[ignore = "benchmark: run with --release -- --ignored"]
fn engine_throughput() {
    let target: usize = std::env::var("CODEATLAS_BENCH_LINES")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(DEFAULT_LINES);
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    let (files, lines, bytes) = generate(root, target);

    // Read every file once first, so operating-system and antivirus costs of opening new files
    // are not timed (on Windows they made a cold scan 300 times slower than a warm one).
    let _ = parse(root, &scan(root), NonZeroUsize::MIN);

    let (inventory, scan_time) = timed(|| scan(root));
    assert_eq!(inventory.files().len(), files);
    let (one, one_time) = timed(|| parse(root, &inventory, NonZeroUsize::MIN));
    let threads = std::thread::available_parallelism().unwrap_or(NonZeroUsize::MIN);
    let (many, many_time) = timed(|| parse(root, &inventory, threads));

    assert!(one.failed.is_empty(), "{:?}", one.failed.first());
    assert_eq!(many, one, "the report must not depend on the thread count");
    let symbols: usize = one.parsed.iter().map(|p| p.symbols().len()).sum();
    assert_eq!(symbols, files * SYMBOLS_PER_FILE);

    let budget = BUDGET_PER_MILLION_LINES.mul_f64(lines as f64 / 1_000_000.0);
    let total = scan_time + many_time;
    let mut table = String::new();
    let _ = writeln!(table, "### Engine benchmark (ADR-0014)\n");
    let _ = writeln!(table, "| Measure | Value |");
    let _ = writeln!(table, "|---|---|");
    let _ = writeln!(
        table,
        "| Repository | {files} files, {lines} lines, {:.1} MB, {symbols} symbols |",
        bytes as f64 / 1e6
    );
    let _ = writeln!(
        table,
        "| Scan | {scan_time:.2?} ({}) |",
        rate(lines, scan_time)
    );
    let _ = writeln!(
        table,
        "| Parse and symbols, 1 thread | {one_time:.2?} ({}) |",
        rate(lines, one_time)
    );
    let _ = writeln!(
        table,
        "| Parse and symbols, {threads} threads | {many_time:.2?} ({}), {:.1}x |",
        rate(lines, many_time),
        one_time.as_secs_f64() / many_time.as_secs_f64().max(1e-9)
    );
    let _ = writeln!(
        table,
        "| Scan + parse, {threads} threads | {total:.2?}; budget {budget:.2?} on the 8-thread reference machine |"
    );
    println!("{table}");
    if let Ok(summary) = std::env::var("GITHUB_STEP_SUMMARY") {
        let mut existing = fs::read_to_string(&summary).unwrap_or_default();
        existing.push_str(&table);
        fs::write(summary, existing).unwrap();
    }
}
