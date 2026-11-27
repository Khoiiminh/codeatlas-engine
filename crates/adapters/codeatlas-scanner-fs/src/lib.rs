//! codeatlas-scanner-fs: Adapter (driven): filesystem SourceScanner (ENG-01).
//!
//! Walks a repository on the local disk and builds the source inventory. The rules (which
//! files are source, which folders to skip) come from `codeatlas-core`; this crate only does
//! the I/O: walking, `.gitignore` matching, reading and hashing.
//!
//! Allowed workspace dependencies are listed in crates/README.md (ADR-0010).

use std::fs;
use std::path::{Component, Path};
use std::sync::{Arc, Mutex, PoisonError};

use codeatlas_application::{ScanError, ScanRequest, SourceScanner};
use codeatlas_core::{
    ContentHash, IgnorePolicy, Inventory, Language, RepoPath, SkipReason, SkippedEntry, SourceFile,
};
use ignore::WalkBuilder;
use sha2::{Digest, Sha256};

/// [`SourceScanner`] for a working tree on the local file system.
#[derive(Debug, Default, Clone, Copy)]
pub struct FsSourceScanner;

impl SourceScanner for FsSourceScanner {
    fn scan(&self, request: &ScanRequest) -> Result<Inventory, ScanError> {
        let root = request.root.as_path();
        match fs::metadata(root) {
            Ok(meta) if meta.is_dir() => {}
            Ok(_) => return Err(ScanError::RootNotDirectory(root.to_path_buf())),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                return Err(ScanError::RootNotFound(root.to_path_buf()));
            }
            Err(e) => {
                return Err(ScanError::Io {
                    path: root.to_path_buf(),
                    message: e.to_string(),
                });
            }
        }

        // Folders the policy skips are recorded while the walker prunes them, so it never
        // descends into node_modules and the like.
        let pruned: Arc<Mutex<Vec<SkippedEntry>>> = Arc::default();
        let walker = {
            let pruned = Arc::clone(&pruned);
            let policy: IgnorePolicy = request.policy.clone();
            let root = root.to_path_buf();
            WalkBuilder::new(&root)
                // Only the repository's own .gitignore files: no parent folders, no global or
                // user excludes, no .ignore files, and hidden files are not special. The result
                // then depends only on what is inside the root.
                .standard_filters(false)
                .git_ignore(true)
                .require_git(false)
                .follow_links(false)
                .sort_by_file_name(|a, b| a.cmp(b))
                .filter_entry(move |entry| {
                    let is_dir = entry.file_type().is_some_and(|t| t.is_dir());
                    if entry.depth() == 0 || !is_dir {
                        return true;
                    }
                    let path = repo_path(&root, entry.path()).unwrap_or_else(|lossy| lossy);
                    if policy.ignores_dir(&path) {
                        pruned
                            .lock()
                            .unwrap_or_else(PoisonError::into_inner)
                            .push(SkippedEntry {
                                path,
                                reason: SkipReason::IgnoredDirectory,
                            });
                        return false;
                    }
                    true
                })
                .build()
        };

        let mut files = Vec::new();
        let mut skipped = Vec::new();
        for result in walker {
            let entry = match result {
                Ok(entry) => entry,
                Err(err) => {
                    skipped.push(unreadable(root, &err)?);
                    continue;
                }
            };
            if entry.depth() == 0 {
                continue;
            }
            let Some(file_type) = entry.file_type() else {
                continue;
            };
            if file_type.is_dir() {
                continue;
            }
            let path = match repo_path(root, entry.path()) {
                Ok(path) => path,
                Err(lossy) => {
                    skipped.push(SkippedEntry {
                        path: lossy,
                        reason: SkipReason::NonUnicodePath,
                    });
                    continue;
                }
            };
            if file_type.is_symlink() {
                skipped.push(SkippedEntry {
                    path,
                    reason: SkipReason::Symlink,
                });
                continue;
            }
            if !file_type.is_file() {
                continue;
            }
            let Some(language) = Language::detect(&path) else {
                continue;
            };
            match read_source(entry.path(), request.max_file_bytes) {
                Ok((size_bytes, content_hash)) => files.push(SourceFile {
                    path,
                    language,
                    size_bytes,
                    content_hash,
                }),
                Err(reason) => skipped.push(SkippedEntry { path, reason }),
            }
        }

        skipped.extend(
            Arc::try_unwrap(pruned)
                .map(|m| m.into_inner().unwrap_or_else(PoisonError::into_inner))
                .unwrap_or_else(|shared| {
                    shared
                        .lock()
                        .unwrap_or_else(PoisonError::into_inner)
                        .clone()
                }),
        );
        Inventory::new(files, skipped).map_err(|e| ScanError::InvalidInventory(e.to_string()))
    }
}

/// Size and SHA-256 of a source file, or the reason it cannot be used.
fn read_source(path: &Path, max_bytes: u64) -> Result<(u64, ContentHash), SkipReason> {
    let unreadable = |e: std::io::Error| SkipReason::Unreadable {
        message: e.to_string(),
    };
    let size = fs::metadata(path).map_err(unreadable)?.len();
    if size > max_bytes {
        return Err(SkipReason::TooLarge {
            size_bytes: size,
            limit_bytes: max_bytes,
        });
    }
    let bytes = fs::read(path).map_err(unreadable)?;
    if std::str::from_utf8(&bytes).is_err() {
        return Err(SkipReason::NotUtf8);
    }
    let digest: [u8; 32] = Sha256::digest(&bytes).into();
    Ok((bytes.len() as u64, ContentHash::from_bytes(digest)))
}

/// `path` relative to `root` as a [`RepoPath`]. A name that is not valid Unicode gives
/// `Err` with a lossy rendering, so it can still be reported.
fn repo_path(root: &Path, path: &Path) -> Result<RepoPath, RepoPath> {
    let relative = path.strip_prefix(root).unwrap_or(path);
    let mut lossless = true;
    let segments: Vec<String> = relative
        .components()
        .filter_map(|c| match c {
            Component::Normal(name) => Some(match name.to_str() {
                Some(s) => s.to_string(),
                None => {
                    lossless = false;
                    name.to_string_lossy().into_owned()
                }
            }),
            _ => None,
        })
        .collect();
    let path = RepoPath::new(segments.join("/"))
        .unwrap_or_else(|_| RepoPath::new("<invalid path>").expect("constant is valid"));
    if lossless { Ok(path) } else { Err(path) }
}

/// A walk error becomes a skipped entry when it names a path inside the root; otherwise the
/// scan cannot be trusted and fails.
fn unreadable(root: &Path, err: &ignore::Error) -> Result<SkippedEntry, ScanError> {
    fn path_of(err: &ignore::Error) -> Option<&Path> {
        match err {
            ignore::Error::WithPath { path, .. } => Some(path),
            ignore::Error::WithDepth { err, .. } | ignore::Error::WithLineNumber { err, .. } => {
                path_of(err)
            }
            ignore::Error::Partial(errs) => errs.iter().find_map(path_of),
            _ => None,
        }
    }
    match path_of(err).filter(|p| p.starts_with(root) && *p != root) {
        Some(path) => Ok(SkippedEntry {
            path: repo_path(root, path).unwrap_or_else(|lossy| lossy),
            reason: SkipReason::Unreadable {
                message: err.to_string(),
            },
        }),
        None => Err(ScanError::Io {
            path: root.to_path_buf(),
            message: err.to_string(),
        }),
    }
}
