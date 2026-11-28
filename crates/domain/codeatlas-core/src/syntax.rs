//! Result of parsing one source file (ENG-02).
//!
//! A parser always produces a tree, even for broken code: it recovers by skipping text it
//! cannot place (an *unexpected* region) or by assuming a token that is not there (a *missing*
//! token). [`ParsedSource`] records where that happened. It is never an error to parse a file
//! with problems; the rest of the analysis keeps whatever the parser recovered, and the
//! problems become diagnostics (`CA1001`).

use crate::diagnostic::{Diagnostic, DiagnosticCode, Severity};
use crate::source::{Language, RepoPath};
use crate::text::{Range, SourceLocation};

/// What the parser had to do to recover.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum SyntaxProblemKind {
    /// Text that does not fit the grammar at this point and was skipped.
    Unexpected,
    /// A token or construct the grammar requires here but the text lacks, for example `}`
    /// or `identifier`. Its range is empty, at the place it was expected.
    Missing { expected: String },
}

/// One place where the parser recovered from a syntax error.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct SyntaxProblem {
    pub range: Range,
    pub kind: SyntaxProblemKind,
}

/// The parse result for one file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParsedSource {
    path: RepoPath,
    language: Language,
    problems: Vec<SyntaxProblem>,
    problems_truncated: bool,
}

impl ParsedSource {
    /// Problems are sorted by position. `problems_truncated` says the parser stopped
    /// recording after a limit, so there were more than `problems` lists.
    pub fn new(
        path: RepoPath,
        language: Language,
        mut problems: Vec<SyntaxProblem>,
        problems_truncated: bool,
    ) -> Self {
        problems.sort();
        problems.dedup();
        ParsedSource {
            path,
            language,
            problems,
            problems_truncated,
        }
    }

    pub fn path(&self) -> &RepoPath {
        &self.path
    }

    pub fn language(&self) -> Language {
        self.language
    }

    pub fn problems(&self) -> &[SyntaxProblem] {
        &self.problems
    }

    pub fn problems_truncated(&self) -> bool {
        self.problems_truncated
    }

    /// True when the file parsed without any recovery.
    pub fn is_clean(&self) -> bool {
        self.problems.is_empty() && !self.problems_truncated
    }

    /// One `CA1001` warning per recorded problem, plus one without a location when the list
    /// was truncated.
    pub fn diagnostics(&self) -> Vec<Diagnostic> {
        let mut out: Vec<Diagnostic> = self
            .problems
            .iter()
            .map(|p| {
                let detail = match &p.kind {
                    SyntaxProblemKind::Unexpected => "unexpected syntax".to_string(),
                    SyntaxProblemKind::Missing { expected } => format!("expected `{expected}`"),
                };
                Diagnostic {
                    severity: Severity::Warning,
                    code: DiagnosticCode::PARSE_INCOMPLETE,
                    message: format!("File could not be parsed completely: {detail}."),
                    location: Some(SourceLocation {
                        path: self.path.clone(),
                        range: p.range,
                    }),
                }
            })
            .collect();
        if self.problems_truncated {
            out.push(Diagnostic {
                severity: Severity::Warning,
                code: DiagnosticCode::PARSE_INCOMPLETE,
                message: format!(
                    "File could not be parsed completely: more syntax problems in {} than are listed.",
                    self.path
                ),
                location: None,
            });
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::text::Position;

    fn at(line: u32, character: u32) -> Range {
        Range::empty(Position::new(line, character))
    }

    #[test]
    fn problems_are_sorted_and_deduplicated() {
        let missing = SyntaxProblem {
            range: at(3, 0),
            kind: SyntaxProblemKind::Missing {
                expected: "}".into(),
            },
        };
        let unexpected = SyntaxProblem {
            range: at(1, 4),
            kind: SyntaxProblemKind::Unexpected,
        };
        let parsed = ParsedSource::new(
            RepoPath::new("src/a.ts").unwrap(),
            Language::TypeScript,
            vec![missing.clone(), unexpected.clone(), missing.clone()],
            false,
        );
        assert_eq!(parsed.problems(), [unexpected, missing]);
        assert!(!parsed.is_clean());
    }

    #[test]
    fn problems_become_ca1001_warnings() {
        let parsed = ParsedSource::new(
            RepoPath::new("src/a.ts").unwrap(),
            Language::TypeScript,
            vec![SyntaxProblem {
                range: at(2, 7),
                kind: SyntaxProblemKind::Missing {
                    expected: ";".into(),
                },
            }],
            true,
        );
        let diagnostics = parsed.diagnostics();
        assert_eq!(diagnostics.len(), 2);
        assert_eq!(diagnostics[0].code.as_str(), "CA1001");
        assert_eq!(diagnostics[0].severity, Severity::Warning);
        assert_eq!(
            diagnostics[0].message,
            "File could not be parsed completely: expected `;`."
        );
        assert_eq!(diagnostics[0].location.as_ref().unwrap().range, at(2, 7));
        assert!(diagnostics[1].location.is_none());
    }

    #[test]
    fn a_clean_file_has_no_diagnostics() {
        let parsed = ParsedSource::new(
            RepoPath::new("a.js").unwrap(),
            Language::JavaScript,
            vec![],
            false,
        );
        assert!(parsed.is_clean());
        assert!(parsed.diagnostics().is_empty());
    }
}
