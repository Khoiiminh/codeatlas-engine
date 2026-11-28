//! ENG-02: parse every source file of an inventory.
//!
//! Two driven ports do the work: [`SourceReader`] fetches a file's text, and
//! [`LanguageParser`] turns it into a [`ParsedSource`]. The use case [`ParseSources`] owns the
//! rule that one bad file never stops the run. Syntax errors are part of a normal result, and
//! a file that cannot be read or parsed at all is reported as a [`ParseFailure`], even when the
//! parser panics.

use std::any::Any;
use std::fmt;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::path::Path;
use std::time::Duration;

use codeatlas_core::{
    Diagnostic, DiagnosticCode, Inventory, Language, ParsedSource, Position, Range, RepoPath,
    Severity, SourceFile, SourceLocation,
};

/// A parser stops recording syntax problems in one file after this many. A file with more is
/// broken throughout, and listing every problem would only bloat the result.
pub const DEFAULT_MAX_PROBLEMS_PER_FILE: usize = 100;

/// A parser gives up on one file after this long. Parsing hand-written source takes
/// milliseconds; this only stops pathological input from stalling a whole analysis.
pub const DEFAULT_PARSE_TIMEOUT: Duration = Duration::from_secs(10);

/// Limits that apply to each file.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ParseLimits {
    pub max_problems: usize,
    /// `None` means no time limit.
    pub timeout: Option<Duration>,
}

impl Default for ParseLimits {
    fn default() -> Self {
        ParseLimits {
            max_problems: DEFAULT_MAX_PROBLEMS_PER_FILE,
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
/// - give up with `ParseError::TimedOut` once `limits.timeout` has passed;
/// - be deterministic: the same input and limits give the same result, apart from timeouts.
pub trait LanguageParser {
    fn parse(
        &self,
        input: &ParseInput<'_>,
        limits: &ParseLimits,
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
pub trait SourceReader {
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

    pub fn execute(&self, root: &Path, inventory: &Inventory, limits: &ParseLimits) -> ParseReport {
        let mut report = ParseReport::default();
        for file in inventory.files() {
            match self.parse_one(root, file, limits) {
                Ok(parsed) => report.parsed.push(parsed),
                Err(reason) => report.failed.push(ParseFailure {
                    path: file.path.clone(),
                    language: file.language,
                    reason,
                }),
            }
        }
        report
    }

    fn parse_one(
        &self,
        root: &Path,
        file: &SourceFile,
        limits: &ParseLimits,
    ) -> Result<ParsedSource, ParseFailureReason> {
        let text = self
            .reader
            .read(root, file)
            .map_err(ParseFailureReason::Read)?;
        let input = ParseInput {
            path: &file.path,
            language: file.language,
            text: &text,
        };
        let parsed = catch_unwind(AssertUnwindSafe(|| self.parser.parse(&input, limits)))
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

    use codeatlas_core::{ContentHash, SyntaxProblem, SyntaxProblemKind};

    use super::*;

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

    /// Reports one problem per `!` in the text, panics on `panic`, times out on `slow`.
    struct Fake;

    impl LanguageParser for Fake {
        fn parse(
            &self,
            input: &ParseInput<'_>,
            limits: &ParseLimits,
        ) -> Result<ParsedSource, ParseError> {
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
        ParseSources::new(&texts, &Fake).execute(Path::new("repo"), &inventory(paths), &limits)
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
        let report = ParseSources::new(&texts, &Wrong).execute(
            Path::new("repo"),
            &inventory(&["a.ts"]),
            &ParseLimits::default(),
        );
        assert!(report.parsed.is_empty());
        assert!(matches!(
            report.failed[0].reason,
            ParseFailureReason::Parser(ParseError::Failed(_))
        ));
    }
}
