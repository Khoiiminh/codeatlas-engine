//! codeatlas-core: Domain layer: core model (files, symbols, edges, evidence, snapshots). No I/O.
//!
//! Allowed workspace dependencies are listed in crates/README.md (ADR-0010).

pub mod diagnostic;
pub mod semantic;
pub mod source;
pub mod symbol;
pub mod syntax;
pub mod text;

pub use diagnostic::{Diagnostic, DiagnosticCode, Severity};
pub use semantic::{
    DeclarationSite, Endpoint, ModuleReference, ModuleReferenceKind, ModuleTarget, ReferenceKind,
    ReferenceTarget, RelationKind, RelationTarget, Relationship, SemanticFacts, SymbolReference,
    SymbolTable, UnresolvedReason, link,
};
pub use source::{
    ContentHash, DEFAULT_IGNORED_DIR_NAMES, IgnorePolicy, Inventory, InventoryError, Language,
    RepoPath, RepoPathError, SkipReason, SkippedEntry, SourceFile,
};
pub use symbol::{
    Declaration, DeclaredSymbol, Symbol, SymbolId, SymbolKind, SymbolLimit, SymbolsTruncated,
    build_symbols,
};
pub use syntax::{ParsedSource, SyntaxProblem, SyntaxProblemKind};
pub use text::{LineIndex, Position, Range, ReversedRange, SourceLocation};
