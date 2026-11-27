//! codeatlas-application: Application layer: use cases and port traits.
//!
//! Allowed workspace dependencies are listed in crates/README.md (ADR-0010).

pub mod scan;

pub use scan::{
    DEFAULT_MAX_FILE_BYTES, ScanError, ScanOptions, ScanRepository, ScanRequest, SourceScanner,
};
