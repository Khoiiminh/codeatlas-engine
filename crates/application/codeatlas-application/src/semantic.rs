//! ENG-04: resolve imports, calls, references and heritage clauses with a compiler.
//!
//! The driven port [`SemanticResolver`] analyses the scanned files (the TypeScript adapter
//! drives the TypeScript compiler in a Node process) and reports [`SemanticFacts`] per file.
//! The use case [`ResolveSemantics`] owns the rules that keep the result trustworthy:
//!
//! - a result must describe the scanned file: one whose content hash differs from the
//!   inventory is a failure (`ChangedSinceScan`), never a result;
//! - every inventory file ends up in exactly one place: linked into relationships, or failed
//!   with a reason. A file the resolver forgot is a failure too;
//! - one bad file, or an analyser that cannot start, never stops the run: the report says
//!   what is missing, as `CA2001` diagnostics, and ENG-05 falls back to syntactic evidence.
//!
//! The facts are linked into relationships with the symbols of the parse report
//! (`codeatlas_core::link`), so ids come from ENG-03 whichever analyser found a relationship.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use std::path::Path;

use codeatlas_core::{
    ContentHash, Diagnostic, DiagnosticCode, Inventory, Position, Range, Relationship, RepoPath,
    SemanticFacts, Severity, SourceFile, SourceLocation, SymbolTable, link,
};

use crate::parse::ParseReport;
use crate::run::{Cancelled, RunContext};

/// What a resolver found in one file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedFile {
    pub path: RepoPath,
    /// SHA-256 of the content the resolver analysed, to check it against the inventory.
    pub content_hash: ContentHash,
    pub facts: SemanticFacts,
}

/// Why one file has no semantic result.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SemanticFailureReason {
    /// The analysed content does not match the inventory.
    ChangedSinceScan,
    /// The resolver could not read the file.
    Unreadable(String),
    /// The analyser failed, stopped or timed out before it finished this file.
    AnalyzerFailed(String),
}

impl fmt::Display for SemanticFailureReason {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SemanticFailureReason::ChangedSinceScan => {
                write!(f, "file changed since it was scanned")
            }
            SemanticFailureReason::Unreadable(m) => write!(f, "file could not be read: {m}"),
            SemanticFailureReason::AnalyzerFailed(m) => write!(f, "{m}"),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SemanticFailure {
    pub path: RepoPath,
    pub reason: SemanticFailureReason,
}

/// A problem with a project configuration, such as an unknown compiler option or a
/// `tsconfig.json` that is not valid JSON. The files the project owns are still analysed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectProblem {
    /// Where in the configuration, when the file is inside the repository.
    pub location: Option<SourceLocation>,
    pub message: String,
}

/// What a resolver returns for one run.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SemanticOutput {
    /// In any order.
    pub files: Vec<ResolvedFile>,
    pub failed: Vec<SemanticFailure>,
    pub problems: Vec<ProjectProblem>,
    /// The analyser's peak memory, when it reports one (ADR-0014 budgets).
    pub peak_memory_bytes: Option<u64>,
}

/// Why a resolver returned nothing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SemanticError {
    /// The run was cancelled (ADR-0014).
    Cancelled,
    /// The analyser could not run at all: no runtime, a missing installation, or one that
    /// speaks another protocol version.
    Unavailable(String),
}

impl fmt::Display for SemanticError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SemanticError::Cancelled => write!(f, "the run was cancelled"),
            SemanticError::Unavailable(m) => write!(f, "semantic analysis is not available: {m}"),
        }
    }
}

impl std::error::Error for SemanticError {}

/// Driven port: resolves the module and symbol references of scanned files.
///
/// Implementations must:
/// - analyse the files of `files` (scanned from `root`) and report each one once, in
///   [`SemanticOutput::files`] or [`SemanticOutput::failed`];
/// - report positions in the contract's units (zero-based lines, UTF-16 characters, the
///   line breaks of `codeatlas_core::text`), relative to the text after a leading byte order
///   mark, and name a declaration by the start of its name (`DeclarationSite`);
/// - return the SHA-256 of the content each result describes;
/// - stop soon after the run is cancelled, with `SemanticError::Cancelled`, and report
///   `Phase::Resolve` progress per file;
/// - never execute repository code or install dependencies (ADR-0004);
/// - be usable from several threads at once (`Send + Sync`).
pub trait SemanticResolver: Send + Sync {
    fn resolve(
        &self,
        root: &Path,
        files: &[SourceFile],
        run: &RunContext<'_>,
    ) -> Result<SemanticOutput, SemanticError>;
}

/// The result of [`ResolveSemantics`].
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SemanticReport {
    /// In path order, then by position within each file.
    pub relationships: Vec<Relationship>,
    /// In path order.
    pub failed: Vec<SemanticFailure>,
    pub problems: Vec<ProjectProblem>,
    /// Set when the analyser could not run at all; nothing was resolved.
    pub unavailable: Option<String>,
    pub peak_memory_bytes: Option<u64>,
}

impl SemanticReport {
    /// One `CA2001` warning when semantic analysis was unavailable, else one per failed file,
    /// then one `CA2002` warning per project problem.
    pub fn diagnostics(&self) -> Vec<Diagnostic> {
        let mut out = Vec::new();
        if let Some(message) = &self.unavailable {
            out.push(Diagnostic {
                severity: Severity::Warning,
                code: DiagnosticCode::SEMANTICS_UNAVAILABLE,
                message: format!(
                    "Semantic analysis is not available ({message}), so imports, calls and references are not resolved."
                ),
                location: None,
            });
        }
        for failure in &self.failed {
            out.push(Diagnostic {
                severity: Severity::Warning,
                code: DiagnosticCode::SEMANTICS_UNAVAILABLE,
                message: format!(
                    "Imports, calls and references of this file are not resolved: {}.",
                    failure.reason
                ),
                location: Some(SourceLocation {
                    path: failure.path.clone(),
                    range: Range::empty(Position::new(0, 0)),
                }),
            });
        }
        for problem in &self.problems {
            out.push(Diagnostic {
                severity: Severity::Warning,
                code: DiagnosticCode::PROJECT_CONFIG_PROBLEM,
                message: format!(
                    "TypeScript project configuration problem: {}",
                    problem.message
                ),
                location: problem.location.clone(),
            });
        }
        out
    }
}

/// Use case: resolve the references of every file in an inventory scanned from `root`.
pub struct ResolveSemantics<'a> {
    resolver: &'a dyn SemanticResolver,
}

impl<'a> ResolveSemantics<'a> {
    pub fn new(resolver: &'a dyn SemanticResolver) -> Self {
        ResolveSemantics { resolver }
    }

    /// Resolves every file of `inventory` and links the facts with the symbols of `parsed`
    /// (the report of the same inventory). A file the parser could not read has no symbols,
    /// so its relationships come from the file, and nothing can target its declarations.
    pub fn execute(
        &self,
        root: &Path,
        inventory: &Inventory,
        parsed: &ParseReport,
        run: &RunContext<'_>,
    ) -> Result<SemanticReport, Cancelled> {
        run.check()?;
        let output = match self.resolver.resolve(root, inventory.files(), run) {
            Ok(output) => output,
            Err(SemanticError::Cancelled) => return Err(Cancelled),
            Err(SemanticError::Unavailable(message)) => {
                run.check()?;
                return Ok(SemanticReport {
                    unavailable: Some(message),
                    ..SemanticReport::default()
                });
            }
        };
        run.check()?;

        let scanned: BTreeMap<&RepoPath, &SourceFile> =
            inventory.files().iter().map(|f| (&f.path, f)).collect();
        let mut seen: BTreeSet<RepoPath> = BTreeSet::new();
        let mut resolved: BTreeMap<RepoPath, SemanticFacts> = BTreeMap::new();
        let mut failed: BTreeMap<RepoPath, SemanticFailureReason> = BTreeMap::new();
        // A path outside the inventory is ignored, and only the first result for a path counts.
        for file in output.files {
            let Some(source) = scanned.get(&file.path) else {
                continue;
            };
            if !seen.insert(file.path.clone()) {
                continue;
            }
            if file.content_hash == source.content_hash {
                resolved.insert(file.path, file.facts);
            } else {
                failed.insert(file.path, SemanticFailureReason::ChangedSinceScan);
            }
        }
        for failure in output.failed {
            if scanned.contains_key(&failure.path) && seen.insert(failure.path.clone()) {
                failed.insert(failure.path, failure.reason);
            }
        }
        for path in scanned.keys() {
            if !seen.contains(*path) {
                failed.insert(
                    (*path).clone(),
                    SemanticFailureReason::AnalyzerFailed(
                        "the analyser returned no result for this file".into(),
                    ),
                );
            }
        }

        let symbols = SymbolTable::new(parsed.parsed.iter().flat_map(|p| p.symbols()));
        let relationships = resolved
            .iter()
            .flat_map(|(path, facts)| link(path, facts, &symbols))
            .collect();
        Ok(SemanticReport {
            relationships,
            failed: failed
                .into_iter()
                .map(|(path, reason)| SemanticFailure { path, reason })
                .collect(),
            problems: output.problems,
            unavailable: None,
            peak_memory_bytes: output.peak_memory_bytes,
        })
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;
    use std::sync::Mutex;

    use codeatlas_core::{
        Declaration, DeclarationSite, DeclaredSymbol, Endpoint, Language, ModuleReference,
        ModuleReferenceKind, ModuleTarget, ParsedSource, ReferenceKind, ReferenceTarget,
        RelationTarget, SymbolKind, SymbolReference, build_symbols,
    };

    use super::*;
    use crate::run::{
        CancelFlag, CancellationToken, NoProgress, Phase, Progress, ProgressReporter,
    };

    fn path(p: &str) -> RepoPath {
        RepoPath::new(p).unwrap()
    }

    fn hash(n: u8) -> ContentHash {
        ContentHash::from_bytes([n; 32])
    }

    fn source(p: &str, n: u8) -> SourceFile {
        SourceFile {
            path: path(p),
            language: Language::TypeScript,
            size_bytes: 10,
            content_hash: hash(n),
        }
    }

    fn inventory() -> Inventory {
        Inventory::new(
            vec![source("a.ts", 1), source("b.ts", 2), source("c.ts", 3)],
            vec![],
        )
        .unwrap()
    }

    fn range(line: u32, from: u32, to: u32) -> Range {
        Range::new(Position::new(line, from), Position::new(line, to)).unwrap()
    }

    /// `b.ts` declares function `helper` over lines 0-2, its name at 0:9.
    fn parsed() -> ParseReport {
        let symbols = build_symbols(
            &path("b.ts"),
            vec![DeclaredSymbol {
                qualified_name: vec!["helper".into()],
                declaration: Declaration {
                    range: Range::new(Position::new(0, 0), Position::new(2, 1)).unwrap(),
                    name_range: range(0, 9, 15),
                    kind: SymbolKind::Function,
                },
                exported: true,
            }],
            &BTreeSet::new(),
        );
        ParseReport {
            parsed: vec![
                ParsedSource::new(path("a.ts"), Language::TypeScript, vec![], false),
                ParsedSource::new(path("b.ts"), Language::TypeScript, vec![], false)
                    .with_symbols(symbols, None),
            ],
            failed: vec![],
        }
    }

    /// `a.ts` imports `./b` and calls `helper` at top level.
    fn facts_of_a() -> SemanticFacts {
        SemanticFacts {
            modules: vec![ModuleReference {
                kind: ModuleReferenceKind::Import,
                specifier: "./b".into(),
                range: range(0, 20, 25),
                type_only: false,
                target: ModuleTarget::File(path("b.ts")),
            }],
            references: vec![SymbolReference {
                kind: ReferenceKind::Call,
                range: range(2, 0, 6),
                target: ReferenceTarget::Declarations(vec![DeclarationSite {
                    path: path("b.ts"),
                    name_start: Position::new(0, 9),
                }]),
            }],
        }
    }

    struct Fake(Mutex<Option<Result<SemanticOutput, SemanticError>>>);

    impl Fake {
        fn new(result: Result<SemanticOutput, SemanticError>) -> Self {
            Fake(Mutex::new(Some(result)))
        }
    }

    impl SemanticResolver for Fake {
        fn resolve(
            &self,
            _root: &Path,
            files: &[SourceFile],
            run: &RunContext<'_>,
        ) -> Result<SemanticOutput, SemanticError> {
            for (i, _) in files.iter().enumerate() {
                run.report(Phase::Resolve, i + 1, Some(files.len()));
            }
            self.0.lock().unwrap().take().expect("called once")
        }
    }

    fn resolved(p: &str, n: u8, facts: SemanticFacts) -> ResolvedFile {
        ResolvedFile {
            path: path(p),
            content_hash: hash(n),
            facts,
        }
    }

    fn run_with(fake: &Fake) -> SemanticReport {
        ResolveSemantics::new(fake)
            .execute(
                Path::new("."),
                &inventory(),
                &parsed(),
                &RunContext::detached(),
            )
            .unwrap()
    }

    #[test]
    fn facts_are_linked_with_the_parse_symbols() {
        let fake = Fake::new(Ok(SemanticOutput {
            files: vec![
                resolved("c.ts", 3, SemanticFacts::default()),
                resolved("a.ts", 1, facts_of_a()),
                resolved("b.ts", 2, SemanticFacts::default()),
            ],
            peak_memory_bytes: Some(42),
            ..SemanticOutput::default()
        }));
        let report = run_with(&fake);
        assert!(report.failed.is_empty(), "{:?}", report.failed);
        assert_eq!(report.peak_memory_bytes, Some(42));
        let edges: Vec<(String, String, String)> = report
            .relationships
            .iter()
            .map(|r| {
                let RelationTarget::Resolved(target) = &r.target else {
                    panic!("{r:?}");
                };
                (r.kind.to_string(), r.from.id(), target.id())
            })
            .collect();
        assert_eq!(
            edges,
            [
                ("imports".into(), "file:a.ts".into(), "file:b.ts".into()),
                (
                    "calls".into(),
                    "file:a.ts".into(),
                    "symbol:b.ts#helper".into()
                ),
            ]
        );
        assert!(report.diagnostics().is_empty());
    }

    #[test]
    fn a_result_for_different_content_is_a_failure() {
        let fake = Fake::new(Ok(SemanticOutput {
            files: vec![
                resolved("a.ts", 9, facts_of_a()),
                resolved("b.ts", 2, SemanticFacts::default()),
                resolved("c.ts", 3, SemanticFacts::default()),
            ],
            ..SemanticOutput::default()
        }));
        let report = run_with(&fake);
        assert!(report.relationships.is_empty());
        assert_eq!(
            report.failed,
            [SemanticFailure {
                path: path("a.ts"),
                reason: SemanticFailureReason::ChangedSinceScan
            }]
        );
    }

    #[test]
    fn every_file_is_accounted_for_exactly_once() {
        let fake = Fake::new(Ok(SemanticOutput {
            files: vec![
                // A path outside the inventory is ignored.
                resolved("zzz.ts", 1, facts_of_a()),
                resolved("a.ts", 1, facts_of_a()),
                // A second result for a path is ignored.
                resolved("a.ts", 9, SemanticFacts::default()),
            ],
            failed: vec![
                SemanticFailure {
                    path: path("c.ts"),
                    reason: SemanticFailureReason::Unreadable("denied".into()),
                },
                // A failure for a path that already has a result is ignored.
                SemanticFailure {
                    path: path("a.ts"),
                    reason: SemanticFailureReason::Unreadable("late".into()),
                },
            ],
            ..SemanticOutput::default()
        }));
        let report = run_with(&fake);
        assert_eq!(report.relationships.len(), 2);
        // b.ts was forgotten by the resolver.
        assert_eq!(
            report.failed,
            [
                SemanticFailure {
                    path: path("b.ts"),
                    reason: SemanticFailureReason::AnalyzerFailed(
                        "the analyser returned no result for this file".into()
                    )
                },
                SemanticFailure {
                    path: path("c.ts"),
                    reason: SemanticFailureReason::Unreadable("denied".into())
                },
            ]
        );
        let codes: Vec<(String, Option<String>)> = report
            .diagnostics()
            .iter()
            .map(|d| {
                (
                    d.code.to_string(),
                    d.location.as_ref().map(|l| l.path.to_string()),
                )
            })
            .collect();
        assert_eq!(
            codes,
            [
                ("CA2001".into(), Some("b.ts".into())),
                ("CA2001".into(), Some("c.ts".into()))
            ]
        );
    }

    #[test]
    fn an_unavailable_analyser_is_one_diagnostic_not_a_failed_run() {
        let fake = Fake::new(Err(SemanticError::Unavailable("node was not found".into())));
        let report = run_with(&fake);
        assert!(report.relationships.is_empty() && report.failed.is_empty());
        let diagnostics = report.diagnostics();
        assert_eq!(diagnostics.len(), 1);
        assert_eq!(diagnostics[0].code, DiagnosticCode::SEMANTICS_UNAVAILABLE);
        assert_eq!(diagnostics[0].location, None);
        assert!(diagnostics[0].message.contains("node was not found"));
    }

    #[test]
    fn project_problems_are_ca2002_warnings() {
        let location = SourceLocation {
            path: path("tsconfig.json"),
            range: range(3, 4, 10),
        };
        let fake = Fake::new(Ok(SemanticOutput {
            files: vec![
                resolved("a.ts", 1, SemanticFacts::default()),
                resolved("b.ts", 2, SemanticFacts::default()),
                resolved("c.ts", 3, SemanticFacts::default()),
            ],
            problems: vec![ProjectProblem {
                location: Some(location.clone()),
                message: "Unknown compiler option 'strictt'.".into(),
            }],
            ..SemanticOutput::default()
        }));
        let diagnostics = run_with(&fake).diagnostics();
        assert_eq!(diagnostics.len(), 1);
        assert_eq!(diagnostics[0].code, DiagnosticCode::PROJECT_CONFIG_PROBLEM);
        assert_eq!(diagnostics[0].location, Some(location));
    }

    #[test]
    fn a_cancelled_run_returns_no_report() {
        let fake = Fake::new(Err(SemanticError::Cancelled));
        let result = ResolveSemantics::new(&fake).execute(
            Path::new("."),
            &inventory(),
            &parsed(),
            &RunContext::detached(),
        );
        assert_eq!(result, Err(Cancelled));

        // Cancelled while the resolver ran, even if the resolver returned a result.
        struct CancelsDuringResolve<'f>(&'f CancelFlag);
        impl SemanticResolver for CancelsDuringResolve<'_> {
            fn resolve(
                &self,
                _: &Path,
                _: &[SourceFile],
                _: &RunContext<'_>,
            ) -> Result<SemanticOutput, SemanticError> {
                self.0.cancel();
                Ok(SemanticOutput::default())
            }
        }
        let flag = CancelFlag::new();
        let run = RunContext::new(&flag, &NoProgress);
        let result = ResolveSemantics::new(&CancelsDuringResolve(&flag)).execute(
            Path::new("."),
            &inventory(),
            &parsed(),
            &run,
        );
        assert_eq!(result, Err(Cancelled));
        assert!(flag.is_cancelled());
    }

    #[test]
    fn progress_reaches_the_reporter() {
        struct Events(Mutex<Vec<Progress>>);
        impl ProgressReporter for Events {
            fn report(&self, progress: Progress) {
                self.0.lock().unwrap().push(progress);
            }
        }
        let events = Events(Mutex::new(Vec::new()));
        let fake = Fake::new(Ok(SemanticOutput::default()));
        let run = RunContext::new(&crate::run::NeverCancelled, &events);
        ResolveSemantics::new(&fake)
            .execute(Path::new("."), &inventory(), &parsed(), &run)
            .unwrap();
        let seen: Vec<(Phase, usize)> = events
            .0
            .into_inner()
            .unwrap()
            .iter()
            .map(|p| (p.phase, p.done))
            .collect();
        assert_eq!(
            seen,
            [
                (Phase::Resolve, 1),
                (Phase::Resolve, 2),
                (Phase::Resolve, 3)
            ]
        );
    }

    #[test]
    fn links_survive_a_file_the_parser_could_not_read() {
        // b.ts has no parse result, so a call into it is dropped: there is no symbol to target.
        let mut parsed = parsed();
        parsed.parsed.retain(|p| p.path().as_str() != "b.ts");
        let fake = Fake::new(Ok(SemanticOutput {
            files: vec![
                resolved("a.ts", 1, facts_of_a()),
                resolved("b.ts", 2, SemanticFacts::default()),
                resolved("c.ts", 3, SemanticFacts::default()),
            ],
            ..SemanticOutput::default()
        }));
        let report = ResolveSemantics::new(&fake)
            .execute(
                Path::new("."),
                &inventory(),
                &parsed,
                &RunContext::detached(),
            )
            .unwrap();
        let targets: Vec<String> = report
            .relationships
            .iter()
            .map(|r| match &r.target {
                RelationTarget::Resolved(e) => e.id(),
                RelationTarget::Unresolved { specifier, .. } => specifier.clone(),
            })
            .collect();
        assert_eq!(targets, ["file:b.ts"]);
        assert_eq!(report.relationships[0].from, Endpoint::File(path("a.ts")));
    }
}
