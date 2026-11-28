//! ENG-02: parse every source file of an inventory. ENG-03: the parse result also lists the
//! symbols each file declares.
//!
//! Two driven ports do the work: [`SourceReader`] fetches a file's text, and
//! [`LanguageParser`] turns it into a [`ParsedSource`]. The use case [`ParseSources`] owns the
//! rule that one bad file never stops the run. Syntax errors are part of a normal result, and
//! a file that cannot be read or parsed at all is reported as a [`ParseFailure`], even when the
//! parser panics.
//!
//! ADR-0014: files are parsed on up to `RunContext::workers` threads, the report is the same for
//! any number of threads, and a cancelled run returns `Cancelled` instead of a report.

use std::any::Any;
use std::fmt;
use std::panic::{AssertUnwindSafe, catch_unwind, resume_unwind};
use std::path::Path;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::thread;
use std::time::Duration;

use codeatlas_core::{
    Diagnostic, DiagnosticCode, Inventory, Language, ParsedSource, Position, Range, RepoPath,
    Severity, SourceFile, SourceLocation,
};

use crate::run::{CancellationToken, Cancelled, Phase, RunContext};

/// A parser stops recording syntax problems in one file after this many. A file with more is
/// broken throughout, and listing every problem would only bloat the result.
pub const DEFAULT_MAX_PROBLEMS_PER_FILE: usize = 100;

/// A parser stops extracting symbols from one file after this many declarations. Hand-written
/// files declare hundreds at most; the limit only bounds memory on generated code.
pub const DEFAULT_MAX_SYMBOLS_PER_FILE: usize = 100_000;

/// A parser gives up on one file after this long. Parsing hand-written source takes
/// milliseconds; this only stops pathological input from stalling a whole analysis.
pub const DEFAULT_PARSE_TIMEOUT: Duration = Duration::from_secs(10);

/// Limits that apply to each file.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ParseLimits {
    pub max_problems: usize,
    pub max_symbols: usize,
    /// `None` means no time limit.
    pub timeout: Option<Duration>,
}

impl Default for ParseLimits {
    fn default() -> Self {
        ParseLimits {
            max_problems: DEFAULT_MAX_PROBLEMS_PER_FILE,
            max_symbols: DEFAULT_MAX_SYMBOLS_PER_FILE,
            timeout: Some(DEFAULT_PARSE_TIMEOUT),
        }
    }
}

/// One file to parse.
#[derive(Debug, Clone, Copy)]
pub struct ParseInput<'a> {
    pub path: &'a RepoPath,
    pub language: Language,
    pub text: &'a str,
}

/// Why a parser produced no result for a file. Syntax errors are not in this list: they are
/// recorded in [`ParsedSource::problems`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ParseError {
    UnsupportedLanguage(Language),
    TooLarge {
        bytes: u64,
        limit_bytes: u64,
    },
    TimedOut {
        after: Duration,
    },
    /// The parser failed for another reason (a bug or an incompatible grammar).
    Failed(String),
    /// The run was cancelled while this file was being parsed (ADR-0014).
    Cancelled,
}

impl fmt::Display for ParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ParseError::UnsupportedLanguage(l) => write!(f, "no parser for {}", l.as_str()),
            ParseError::TooLarge { bytes, limit_bytes } => {
                write!(
                    f,
                    "{bytes} bytes is more than the parser's limit of {limit_bytes}"
                )
            }
            ParseError::TimedOut { after } => {
                write!(f, "parsing took longer than {} ms", after.as_millis())
            }
            ParseError::Failed(m) => write!(f, "parser failed: {m}"),
            ParseError::Cancelled => write!(f, "parsing was cancelled"),
        }
    }
}

impl std::error::Error for ParseError {}

/// Driven port: parses source text.
///
/// Implementations must:
/// - return `Ok` for any text in a supported language, however broken, and record each place
///   they recovered as a `SyntaxProblem` with positions in the contract's units (zero-based
///   lines, UTF-16 characters), relative to the text after a leading UTF-8 byte order mark;
/// - stop recording after `limits.max_problems` and set `problems_truncated`;
/// - list the symbols the file declares, built with `codeatlas_core::build_symbols` so the
///   identity rules are the same for every parser, with positions in the same units;
/// - stop extracting symbols after `limits.max_symbols` declarations, or at a nesting depth
///   they document, and say where with `symbols_truncated`;
/// - give up with `ParseError::TimedOut` once `limits.timeout` has passed;
/// - stop soon after `cancel` is set, also in the middle of a file, with `ParseError::Cancelled`;
/// - be deterministic: the same input and limits give the same result, apart from timeouts and
///   cancellation;
/// - be usable from several threads at once (`Send + Sync`).
pub trait LanguageParser: Send + Sync {
    fn parse(
        &self,
        input: &ParseInput<'_>,
        limits: &ParseLimits,
        cancel: &dyn CancellationToken,
    ) -> Result<ParsedSource, ParseError>;
}

/// Why a file's text could not be read.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReadError {
    /// The file's size or content hash no longer matches the inventory, so a result would
    /// describe a different file than the one scanned.
    ChangedSinceScan,
    NotUtf8,
    Io(String),
}

impl fmt::Display for ReadError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ReadError::ChangedSinceScan => write!(f, "file changed since it was scanned"),
            ReadError::NotUtf8 => write!(f, "file is not valid UTF-8"),
            ReadError::Io(m) => write!(f, "file could not be read: {m}"),
        }
    }
}

impl std::error::Error for ReadError {}

/// Driven port: reads the text of a scanned file.
///
/// Implementations must return exactly the content the inventory describes: if the size or
/// SHA-256 differs from `file`, they return `ReadError::ChangedSinceScan`.
pub trait SourceReader: Send + Sync {
    fn read(&self, root: &Path, file: &SourceFile) -> Result<String, ReadError>;
}

/// Why one file has no parse result.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ParseFailureReason {
    Read(ReadError),
    Parser(ParseError),
    /// The parser panicked. The run continued with the next file.
    Panicked(String),
}

impl fmt::Display for ParseFailureReason {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ParseFailureReason::Read(e) => e.fmt(f),
            ParseFailureReason::Parser(e) => e.fmt(f),
            ParseFailureReason::Panicked(m) => write!(f, "parser crashed: {m}"),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParseFailure {
    pub path: RepoPath,
    pub language: Language,
    pub reason: ParseFailureReason,
}

/// Results for every file of an inventory: each file is in exactly one of the two lists, and
/// both are in inventory (path) order.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ParseReport {
    pub parsed: Vec<ParsedSource>,
    pub failed: Vec<ParseFailure>,
}

impl ParseReport {
    /// `CA1001` for every syntax problem and `CA1002` for every failed file, in path order.
    pub fn diagnostics(&self) -> Vec<Diagnostic> {
        let mut out: Vec<(RepoPath, Vec<Diagnostic>)> = self
            .parsed
            .iter()
            .map(|p| (p.path().clone(), p.diagnostics()))
            .chain(self.failed.iter().map(|f| {
                let diagnostic = Diagnostic {
                    severity: Severity::Warning,
                    code: DiagnosticCode::PARSE_FAILED,
                    message: format!("File could not be parsed: {}.", f.reason),
                    location: Some(SourceLocation {
                        path: f.path.clone(),
                        range: Range::empty(Position::new(0, 0)),
                    }),
                };
                (f.path.clone(), vec![diagnostic])
            }))
            .collect();
        out.sort_by(|a, b| a.0.cmp(&b.0));
        out.into_iter().flat_map(|(_, d)| d).collect()
    }
}

/// Use case: parse every source file in an inventory scanned from `root`.
pub struct ParseSources<'a> {
    reader: &'a dyn SourceReader,
    parser: &'a dyn LanguageParser,
}

impl<'a> ParseSources<'a> {
    pub fn new(reader: &'a dyn SourceReader, parser: &'a dyn LanguageParser) -> Self {
        ParseSources { reader, parser }
    }

    /// Parses every file of `inventory`. Workers take the next file from a shared counter, so a
    /// slow file does not hold up the others; results are put back in inventory order.
    /// Progress is `Phase::Parse` with `done` counting finished files: each value from 1 to the
    /// total arrives exactly once, though threads may deliver them out of order.
    pub fn execute(
        &self,
        root: &Path,
        inventory: &Inventory,
        limits: &ParseLimits,
        run: &RunContext<'_>,
    ) -> Result<ParseReport, Cancelled> {
        let files = inventory.files();
        let total = files.len();
        let next = AtomicUsize::new(0);
        let finished = AtomicUsize::new(0);
        let work = || -> Result<Vec<(usize, Outcome)>, Cancelled> {
            let mut outcomes = Vec::new();
            loop {
                run.check()?;
                let i = next.fetch_add(1, Ordering::Relaxed);
                let Some(file) = files.get(i) else {
                    return Ok(outcomes);
                };
                let outcome = self.parse_one(root, file, limits, run.cancellation());
                // A parser that stops for a cancelled run ends the run. One that reports
                // `Cancelled` on its own has a bug, and that file is recorded as failed.
                let stopped = matches!(
                    outcome,
                    Err(ParseFailureReason::Parser(ParseError::Cancelled))
                );
                if stopped && run.is_cancelled() {
                    return Err(Cancelled);
                }
                outcomes.push((i, outcome));
                let done = finished.fetch_add(1, Ordering::Relaxed) + 1;
                run.report(Phase::Parse, done, Some(total));
            }
        };

        let workers = run.workers().get().min(total);
        let mut outcomes = if workers <= 1 {
            work()?
        } else {
            let per_worker: Vec<Result<Vec<(usize, Outcome)>, Cancelled>> = thread::scope(|s| {
                let handles: Vec<_> = (0..workers).map(|_| s.spawn(work)).collect();
                handles
                    .into_iter()
                    // A panic outside the parser (in the reader) was never caught; keep that.
                    .map(|h| h.join().unwrap_or_else(|panic| resume_unwind(panic)))
                    .collect()
            });
            let mut all = Vec::with_capacity(total);
            for outcomes in per_worker {
                all.extend(outcomes?);
            }
            all
        };
        outcomes.sort_unstable_by_key(|(i, _)| *i);

        let mut report = ParseReport::default();
        for (i, outcome) in outcomes {
            let file = &files[i];
            match outcome {
                Ok(parsed) => report.parsed.push(parsed),
                Err(reason) => report.failed.push(ParseFailure {
                    path: file.path.clone(),
                    language: file.language,
                    reason,
                }),
            }
        }
        Ok(report)
    }

    fn parse_one(
        &self,
        root: &Path,
        file: &SourceFile,
        limits: &ParseLimits,
        cancel: &dyn CancellationToken,
    ) -> Outcome {
        let text = self
            .reader
            .read(root, file)
            .map_err(ParseFailureReason::Read)?;
        let input = ParseInput {
            path: &file.path,
            language: file.language,
            text: &text,
        };
        let parsed = catch_unwind(AssertUnwindSafe(|| {
            self.parser.parse(&input, limits, cancel)
        }))
        .map_err(|panic| ParseFailureReason::Panicked(panic_message(panic.as_ref())))?
        .map_err(ParseFailureReason::Parser)?;
        if parsed.path() != &file.path || parsed.language() != file.language {
            return Err(ParseFailureReason::Parser(ParseError::Failed(format!(
                "result describes {} ({}) instead of the requested file",
                parsed.path(),
                parsed.language().as_str()
            ))));
        }
        Ok(parsed)
    }
}

/// What happened to one file.
type Outcome = Result<ParsedSource, ParseFailureReason>;

fn panic_message(panic: &(dyn Any + Send)) -> String {
    panic
        .downcast_ref::<&str>()
        .map(|s| s.to_string())
        .or_else(|| panic.downcast_ref::<String>().cloned())
        .unwrap_or_else(|| "unknown panic".to_string())
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;
    use std::num::NonZeroUsize;
    use std::sync::Mutex;

    use codeatlas_core::{ContentHash, SyntaxProblem, SyntaxProblemKind};

    use super::*;
    use crate::run::{CancelFlag, NeverCancelled, NoProgress, Progress, ProgressReporter};

    fn p(s: &str) -> RepoPath {
        RepoPath::new(s).unwrap()
    }

    fn inventory(paths: &[&str]) -> Inventory {
        let files = paths
            .iter()
            .map(|path| SourceFile {
                path: p(path),
                language: Language::detect(&p(path)).unwrap(),
                size_bytes: 0,
                content_hash: ContentHash::from_bytes([0; 32]),
            })
            .collect();
        Inventory::new(files, vec![]).unwrap()
    }

    /// Serves text from a map; a missing entry is an I/O error.
    struct Texts(BTreeMap<&'static str, Result<&'static str, ReadError>>);

    impl SourceReader for Texts {
        fn read(&self, _root: &Path, file: &SourceFile) -> Result<String, ReadError> {
            match self.0.get(file.path.as_str()) {
                Some(Ok(text)) => Ok(text.to_string()),
                Some(Err(e)) => Err(e.clone()),
                None => Err(ReadError::Io("missing".into())),
            }
        }
    }

    /// Reports one problem per `!` in the text, panics on `panic`, times out on `slow`,
    /// reports `Cancelled` on `cancelled`, and honours cancellation.
    struct Fake;

    impl LanguageParser for Fake {
        fn parse(
            &self,
            input: &ParseInput<'_>,
            limits: &ParseLimits,
            cancel: &dyn CancellationToken,
        ) -> Result<ParsedSource, ParseError> {
            if cancel.is_cancelled() || input.text.contains("cancelled") {
                return Err(ParseError::Cancelled);
            }
            if input.text.contains("panic") {
                panic!("boom in {}", input.path);
            }
            if input.text.contains("slow") {
                return Err(ParseError::TimedOut {
                    after: limits.timeout.unwrap_or_default(),
                });
            }
            let problems: Vec<SyntaxProblem> = input
                .text
                .match_indices('!')
                .map(|(i, _)| SyntaxProblem {
                    range: Range::empty(Position::new(0, i as u32)),
                    kind: SyntaxProblemKind::Unexpected,
                })
                .collect();
            let truncated = problems.len() > limits.max_problems;
            Ok(ParsedSource::new(
                input.path.clone(),
                input.language,
                problems.into_iter().take(limits.max_problems).collect(),
                truncated,
            ))
        }
    }

    fn run(texts: Texts, paths: &[&str], limits: ParseLimits) -> ParseReport {
        ParseSources::new(&texts, &Fake)
            .execute(
                Path::new("repo"),
                &inventory(paths),
                &limits,
                &RunContext::detached(),
            )
            .unwrap()
    }

    fn workers(n: usize) -> NonZeroUsize {
        NonZeroUsize::new(n).unwrap()
    }

    /// 60 files of every kind: clean, broken, panicking, timing out, unreadable, missing.
    fn mixed() -> (Texts, Vec<String>) {
        let kinds: [Result<&'static str, ReadError>; 6] = [
            Ok("fine"),
            Ok("x!y!"),
            Ok("panic"),
            Ok("slow"),
            Err(ReadError::NotUtf8),
            Ok("also fine"),
        ];
        let mut texts = BTreeMap::new();
        let mut paths = Vec::new();
        for i in 0..60 {
            let path: &'static str = Box::leak(format!("src/f{i:02}.ts").into_boxed_str());
            paths.push(path.to_string());
            if i % 7 != 6 {
                texts.insert(path, kinds[i % kinds.len()].clone());
            }
        }
        (Texts(texts), paths)
    }

    /// Records every progress event.
    #[derive(Default)]
    struct Events(Mutex<Vec<Progress>>);

    impl ProgressReporter for Events {
        fn report(&self, progress: Progress) {
            self.0.lock().unwrap().push(progress);
        }
    }

    #[test]
    fn every_file_ends_up_parsed_or_failed_and_the_run_never_stops() {
        let texts = Texts(BTreeMap::from([
            ("a.ts", Ok("fine")),
            ("b.ts", Ok("broken!")),
            ("c.ts", Ok("panic")),
            ("d.ts", Ok("slow")),
            ("e.ts", Err(ReadError::ChangedSinceScan)),
            ("g.js", Ok("also fine")),
        ]));
        let report = run(
            texts,
            &["a.ts", "b.ts", "c.ts", "d.ts", "e.ts", "f.ts", "g.js"],
            ParseLimits::default(),
        );
        let parsed: Vec<(&str, bool)> = report
            .parsed
            .iter()
            .map(|p| (p.path().as_str(), p.is_clean()))
            .collect();
        assert_eq!(parsed, [("a.ts", true), ("b.ts", false), ("g.js", true)]);
        let failed: Vec<(&str, &ParseFailureReason)> = report
            .failed
            .iter()
            .map(|f| (f.path.as_str(), &f.reason))
            .collect();
        assert_eq!(
            failed,
            [
                ("c.ts", &ParseFailureReason::Panicked("boom in c.ts".into())),
                (
                    "d.ts",
                    &ParseFailureReason::Parser(ParseError::TimedOut {
                        after: DEFAULT_PARSE_TIMEOUT
                    })
                ),
                (
                    "e.ts",
                    &ParseFailureReason::Read(ReadError::ChangedSinceScan)
                ),
                (
                    "f.ts",
                    &ParseFailureReason::Read(ReadError::Io("missing".into()))
                ),
            ]
        );
    }

    #[test]
    fn limits_reach_the_parser() {
        let texts = Texts(BTreeMap::from([("a.ts", Ok("!!!!"))]));
        let limits = ParseLimits {
            max_problems: 2,
            timeout: None,
            ..ParseLimits::default()
        };
        let report = run(texts, &["a.ts"], limits);
        assert_eq!(report.parsed[0].problems().len(), 2);
        assert!(report.parsed[0].problems_truncated());
    }

    #[test]
    fn diagnostics_cover_problems_and_failures_in_path_order() {
        let texts = Texts(BTreeMap::from([
            ("a.ts", Ok("x!")),
            ("b.ts", Err(ReadError::NotUtf8)),
            ("c.ts", Ok("ok")),
        ]));
        let report = run(texts, &["a.ts", "b.ts", "c.ts"], ParseLimits::default());
        let diagnostics = report.diagnostics();
        let summary: Vec<(&str, &str, String)> = diagnostics
            .iter()
            .map(|d| {
                let location = d.location.as_ref().unwrap();
                (
                    location.path.as_str(),
                    d.code.as_str(),
                    location.range.start().to_string(),
                )
            })
            .collect();
        assert_eq!(
            summary,
            [
                ("a.ts", "CA1001", "1:2".to_string()),
                ("b.ts", "CA1002", "1:1".to_string())
            ]
        );
        assert_eq!(
            diagnostics[1].message,
            "File could not be parsed: file is not valid UTF-8."
        );
    }

    #[test]
    fn a_result_for_another_file_is_rejected() {
        struct Wrong;
        impl LanguageParser for Wrong {
            fn parse(
                &self,
                input: &ParseInput<'_>,
                _: &ParseLimits,
                _: &dyn CancellationToken,
            ) -> Result<ParsedSource, ParseError> {
                Ok(ParsedSource::new(
                    p("other.ts"),
                    input.language,
                    vec![],
                    false,
                ))
            }
        }
        let texts = Texts(BTreeMap::from([("a.ts", Ok(""))]));
        let report = ParseSources::new(&texts, &Wrong)
            .execute(
                Path::new("repo"),
                &inventory(&["a.ts"]),
                &ParseLimits::default(),
                &RunContext::detached(),
            )
            .unwrap();
        assert!(report.parsed.is_empty());
        assert!(matches!(
            report.failed[0].reason,
            ParseFailureReason::Parser(ParseError::Failed(_))
        ));
    }

    #[test]
    fn the_report_is_the_same_for_any_number_of_threads() {
        let (texts, paths) = mixed();
        let paths: Vec<&str> = paths.iter().map(String::as_str).collect();
        let inventory = inventory(&paths);
        let parse = |n: usize| {
            let run = RunContext::detached().with_workers(workers(n));
            ParseSources::new(&texts, &Fake)
                .execute(Path::new("repo"), &inventory, &ParseLimits::default(), &run)
                .unwrap()
        };
        let one = parse(1);
        assert_eq!(one.parsed.len() + one.failed.len(), 60);
        assert!(!one.failed.is_empty() && !one.parsed.is_empty());
        for n in [2, 3, 8, 64] {
            assert_eq!(parse(n), one, "{n} threads");
        }
    }

    #[test]
    fn progress_counts_every_file_once() {
        let (texts, paths) = mixed();
        let paths: Vec<&str> = paths.iter().map(String::as_str).collect();
        for n in [1, 4] {
            let events = Events::default();
            let run = RunContext::new(&NeverCancelled, &events).with_workers(workers(n));
            ParseSources::new(&texts, &Fake)
                .execute(
                    Path::new("repo"),
                    &inventory(&paths),
                    &ParseLimits::default(),
                    &run,
                )
                .unwrap();
            let events = events.0.into_inner().unwrap();
            assert!(
                events
                    .iter()
                    .all(|e| e.phase == Phase::Parse && e.total == Some(60))
            );
            let mut done: Vec<usize> = events.iter().map(|e| e.done).collect();
            done.sort_unstable();
            assert_eq!(done, (1..=60).collect::<Vec<_>>(), "{n} threads");
        }
    }

    #[test]
    fn a_cancelled_run_returns_no_report() {
        let (texts, paths) = mixed();
        let paths: Vec<&str> = paths.iter().map(String::as_str).collect();
        let flag = CancelFlag::new();
        flag.cancel();
        for n in [1, 4] {
            let run = RunContext::new(&flag, &NoProgress).with_workers(workers(n));
            let result = ParseSources::new(&texts, &Fake).execute(
                Path::new("repo"),
                &inventory(&paths),
                &ParseLimits::default(),
                &run,
            );
            assert_eq!(result, Err(Cancelled), "{n} threads");
        }
    }

    /// Cancels the run when it reads the file `stop.ts`.
    struct StopAt<'a>(&'a CancelFlag, Texts);

    impl SourceReader for StopAt<'_> {
        fn read(&self, root: &Path, file: &SourceFile) -> Result<String, ReadError> {
            if file.path.as_str() == "stop.ts" {
                self.0.cancel();
            }
            self.1.read(root, file)
        }
    }

    #[test]
    fn cancelling_during_the_run_stops_it() {
        let flag = CancelFlag::new();
        let texts = Texts(BTreeMap::from([
            ("a.ts", Ok("fine")),
            ("stop.ts", Ok("fine")),
            ("z.ts", Ok("fine")),
        ]));
        let reader = StopAt(&flag, texts);
        let events = Events::default();
        let run = RunContext::new(&flag, &events);
        let result = ParseSources::new(&reader, &Fake).execute(
            Path::new("repo"),
            &inventory(&["a.ts", "stop.ts", "z.ts"]),
            &ParseLimits::default(),
            &run,
        );
        assert_eq!(result, Err(Cancelled));
        // a.ts finished; the parser saw the cancellation on stop.ts; z.ts never started.
        assert_eq!(events.0.into_inner().unwrap().len(), 1);
    }

    #[test]
    fn work_is_spread_over_the_threads() {
        /// Records which thread parsed each file; each file takes a few milliseconds.
        #[derive(Default)]
        struct Threads(Mutex<std::collections::HashSet<thread::ThreadId>>);
        impl LanguageParser for Threads {
            fn parse(
                &self,
                input: &ParseInput<'_>,
                _: &ParseLimits,
                _: &dyn CancellationToken,
            ) -> Result<ParsedSource, ParseError> {
                self.0.lock().unwrap().insert(thread::current().id());
                thread::sleep(Duration::from_millis(3));
                Ok(ParsedSource::new(
                    input.path.clone(),
                    input.language,
                    vec![],
                    false,
                ))
            }
        }
        let (texts, paths) = mixed();
        let paths: Vec<&str> = paths.iter().map(String::as_str).collect();
        let parser = Threads::default();
        let run = RunContext::detached().with_workers(workers(4));
        ParseSources::new(&texts, &parser)
            .execute(
                Path::new("repo"),
                &inventory(&paths),
                &ParseLimits::default(),
                &run,
            )
            .unwrap();
        assert!(
            parser.0.into_inner().unwrap().len() > 1,
            "files were parsed on one thread"
        );
    }

    #[test]
    fn the_use_case_stops_even_if_the_parser_ignores_cancellation() {
        /// Never looks at the token; counts the files it parses.
        #[derive(Default)]
        struct Oblivious(AtomicUsize);
        impl LanguageParser for Oblivious {
            fn parse(
                &self,
                input: &ParseInput<'_>,
                _: &ParseLimits,
                _: &dyn CancellationToken,
            ) -> Result<ParsedSource, ParseError> {
                self.0.fetch_add(1, Ordering::Relaxed);
                Ok(ParsedSource::new(
                    input.path.clone(),
                    input.language,
                    vec![],
                    false,
                ))
            }
        }
        let flag = CancelFlag::new();
        let texts = Texts(BTreeMap::from([
            ("stop.ts", Ok("fine")),
            ("x.ts", Ok("fine")),
            ("y.ts", Ok("fine")),
        ]));
        let reader = StopAt(&flag, texts);
        let parser = Oblivious::default();
        let result = ParseSources::new(&reader, &parser).execute(
            Path::new("repo"),
            &inventory(&["stop.ts", "x.ts", "y.ts"]),
            &ParseLimits::default(),
            &RunContext::new(&flag, &NoProgress),
        );
        assert_eq!(result, Err(Cancelled));
        assert_eq!(
            parser.0.load(Ordering::Relaxed),
            1,
            "no file starts after the cancellation"
        );
    }

    #[test]
    fn a_parser_that_reports_cancelled_on_its_own_fails_that_file_only() {
        let texts = Texts(BTreeMap::from([
            ("a.ts", Ok("cancelled")),
            ("b.ts", Ok("fine")),
        ]));
        let report = run(texts, &["a.ts", "b.ts"], ParseLimits::default());
        assert_eq!(report.parsed.len(), 1);
        assert_eq!(
            report.failed[0].reason,
            ParseFailureReason::Parser(ParseError::Cancelled)
        );
    }
}
