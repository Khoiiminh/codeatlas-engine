//! ENG-01: build the source inventory of a repository.
//!
//! The use case owns the decisions (which folders to skip, how large a source file may be);
//! the [`SourceScanner`] port only walks and reads. Any scanner (local disk today, an isolated
//! worker snapshot in the hosted platform later) therefore produces the same inventory.

use std::fmt;
use std::path::PathBuf;

use codeatlas_core::{IgnorePolicy, Inventory, RepoPath};

/// Source files larger than this are reported as skipped, not analysed. Hand-written source is
/// far smaller; files this big are almost always generated (bundles, generated API clients).
pub const DEFAULT_MAX_FILE_BYTES: u64 = 2 * 1024 * 1024;

/// What a scanner is asked to do.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScanRequest {
    /// Repository root on the scanner's file system.
    pub root: PathBuf,
    pub policy: IgnorePolicy,
    pub max_file_bytes: u64,
}

/// Why a scan could not produce an inventory at all. Problems with individual files are not
/// errors: they are reported in [`Inventory::skipped`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ScanError {
    RootNotFound(PathBuf),
    RootNotDirectory(PathBuf),
    Io {
        path: PathBuf,
        message: String,
    },
    /// The scanner broke the inventory rules (a bug in the adapter).
    InvalidInventory(String),
}

impl fmt::Display for ScanError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ScanError::RootNotFound(p) => {
                write!(f, "repository root {} does not exist", p.display())
            }
            ScanError::RootNotDirectory(p) => {
                write!(f, "repository root {} is not a directory", p.display())
            }
            ScanError::Io { path, message } => write!(f, "{}: {message}", path.display()),
            ScanError::InvalidInventory(m) => {
                write!(f, "scanner produced an invalid inventory: {m}")
            }
        }
    }
}

impl std::error::Error for ScanError {}

/// Driven port: walks a repository and returns its source inventory.
///
/// Implementations must:
/// - skip every folder the request's policy ignores (without descending into it) and report it
///   as `SkipReason::IgnoredDirectory`, except the root itself, which is always scanned;
/// - honour `.gitignore` files inside the root, and nothing outside it (no parent folders, no
///   global or user configuration), so the result depends only on the repository's contents;
/// - never follow symbolic links;
/// - classify files with `Language::detect`, and report source files that are too large, not
///   UTF-8 or unreadable instead of failing.
pub trait SourceScanner {
    fn scan(&self, request: &ScanRequest) -> Result<Inventory, ScanError>;
}

/// Caller's choices for one scan; everything else comes from the defaults.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ScanOptions {
    /// Extra folder names to skip anywhere (for example `generated`).
    pub ignore_dir_names: Vec<String>,
    /// Extra folders to skip, relative to the root.
    pub ignore_dir_paths: Vec<RepoPath>,
    /// Default folder names to scan after all (for example `build` when it holds real source).
    pub include_dir_names: Vec<String>,
    /// Overrides [`DEFAULT_MAX_FILE_BYTES`].
    pub max_file_bytes: Option<u64>,
}

/// Use case: inventory the source files of the repository at `root`.
pub struct ScanRepository<'a> {
    scanner: &'a dyn SourceScanner,
}

impl<'a> ScanRepository<'a> {
    pub fn new(scanner: &'a dyn SourceScanner) -> Self {
        ScanRepository { scanner }
    }

    pub fn execute(&self, root: PathBuf, options: &ScanOptions) -> Result<Inventory, ScanError> {
        let mut policy = IgnorePolicy::default();
        for name in &options.include_dir_names {
            policy = policy.include_name(name);
        }
        for name in &options.ignore_dir_names {
            policy = policy.ignore_name(name.clone());
        }
        for path in &options.ignore_dir_paths {
            policy = policy.ignore_path(path.clone());
        }
        let request = ScanRequest {
            root,
            policy,
            max_file_bytes: options.max_file_bytes.unwrap_or(DEFAULT_MAX_FILE_BYTES),
        };
        self.scanner.scan(&request)
    }
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;

    use super::*;

    /// Records the request and returns an empty inventory.
    #[derive(Default)]
    struct Recorder(RefCell<Option<ScanRequest>>);

    impl SourceScanner for Recorder {
        fn scan(&self, request: &ScanRequest) -> Result<Inventory, ScanError> {
            *self.0.borrow_mut() = Some(request.clone());
            Ok(Inventory::default())
        }
    }

    fn p(s: &str) -> RepoPath {
        RepoPath::new(s).unwrap()
    }

    #[test]
    fn defaults_reach_the_scanner() {
        let scanner = Recorder::default();
        ScanRepository::new(&scanner)
            .execute(PathBuf::from("repo"), &ScanOptions::default())
            .unwrap();
        let request = scanner.0.take().unwrap();
        assert_eq!(request.root, PathBuf::from("repo"));
        assert_eq!(request.max_file_bytes, DEFAULT_MAX_FILE_BYTES);
        assert_eq!(request.policy, IgnorePolicy::default());
    }

    #[test]
    fn options_adjust_the_policy_and_limit() {
        let scanner = Recorder::default();
        let options = ScanOptions {
            ignore_dir_names: vec!["generated".into()],
            ignore_dir_paths: vec![p("legacy/vendor")],
            include_dir_names: vec!["build".into()],
            max_file_bytes: Some(10),
        };
        ScanRepository::new(&scanner)
            .execute(PathBuf::from("repo"), &options)
            .unwrap();
        let request = scanner.0.take().unwrap();
        assert_eq!(request.max_file_bytes, 10);
        assert!(request.policy.ignores_dir(&p("src/generated")));
        assert!(request.policy.ignores_dir(&p("legacy/vendor")));
        assert!(!request.policy.ignores_dir(&p("build")));
        assert!(request.policy.ignores_dir(&p("node_modules")));
    }

    #[test]
    fn scanner_errors_are_returned_unchanged() {
        struct Missing;
        impl SourceScanner for Missing {
            fn scan(&self, request: &ScanRequest) -> Result<Inventory, ScanError> {
                Err(ScanError::RootNotFound(request.root.clone()))
            }
        }
        let err = ScanRepository::new(&Missing)
            .execute(PathBuf::from("nowhere"), &ScanOptions::default())
            .unwrap_err();
        assert_eq!(err, ScanError::RootNotFound(PathBuf::from("nowhere")));
        assert!(err.to_string().contains("does not exist"));
    }
}
