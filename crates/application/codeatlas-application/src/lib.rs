//! codeatlas-application: Application layer: use cases and port traits.
//!
//! Allowed workspace dependencies are listed in crates/README.md (ADR-0010).

pub mod parse;
pub mod run;
pub mod scan;
pub mod semantic;

pub use parse::{
    DEFAULT_MAX_PROBLEMS_PER_FILE, DEFAULT_MAX_SYMBOLS_PER_FILE, DEFAULT_PARSE_TIMEOUT,
    LanguageParser, ParseError, ParseFailure, ParseFailureReason, ParseInput, ParseLimits,
    ParseReport, ParseSources, ReadError, SourceReader,
};
pub use run::{
    CancelFlag, CancellationToken, Cancelled, NeverCancelled, NoProgress, Phase, Progress,
    ProgressReporter, RunContext,
};
pub use scan::{
    DEFAULT_MAX_FILE_BYTES, ScanError, ScanOptions, ScanRepository, ScanRequest, SourceScanner,
};
pub use semantic::{
    ProjectProblem, ResolveSemantics, ResolvedFile, SemanticError, SemanticFailure,
    SemanticFailureReason, SemanticOutput, SemanticReport, SemanticResolver,
};
