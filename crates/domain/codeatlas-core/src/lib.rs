//! codeatlas-core: Domain layer: core model (files, symbols, edges, evidence, snapshots). No I/O.
//!
//! Allowed workspace dependencies are listed in crates/README.md (ADR-0010).

pub mod source;

pub use source::{
    ContentHash, DEFAULT_IGNORED_DIR_NAMES, IgnorePolicy, Inventory, InventoryError, Language,
    RepoPath, RepoPathError, SkipReason, SkippedEntry, SourceFile,
};
