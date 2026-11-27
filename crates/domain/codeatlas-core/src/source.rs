//! Source inventory model (ENG-01): which files of a repository are analysable source.
//!
//! Everything here is pure: no file system access. The scanner adapter
//! (`codeatlas-scanner-fs`) walks the disk and builds these values; the rules for *what counts*
//! (language by extension, which folders are dependencies or build output) live here, so every
//! scanner applies the same rules.

use std::collections::BTreeSet;
use std::fmt;

/// A path relative to the repository root, always written with `/` separators.
///
/// Invariants: not empty, no leading or trailing `/`, no `\`, no empty, `.` or `..` segments.
/// Two `RepoPath`s are equal exactly when they name the same file on every operating system.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct RepoPath(String);

/// Why a string is not a valid [`RepoPath`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RepoPathError {
    Empty,
    Absolute,
    Backslash,
    TrailingSlash,
    BadSegment(String),
}

impl fmt::Display for RepoPathError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            RepoPathError::Empty => write!(f, "path is empty"),
            RepoPathError::Absolute => write!(f, "path must be relative to the repository root"),
            RepoPathError::Backslash => write!(f, "path must use '/' separators"),
            RepoPathError::TrailingSlash => write!(f, "path must not end with '/'"),
            RepoPathError::BadSegment(s) => write!(f, "path segment {s:?} is not allowed"),
        }
    }
}

impl std::error::Error for RepoPathError {}

impl RepoPath {
    pub fn new(path: impl Into<String>) -> Result<Self, RepoPathError> {
        let path = path.into();
        if path.is_empty() {
            return Err(RepoPathError::Empty);
        }
        if path.starts_with('/') {
            return Err(RepoPathError::Absolute);
        }
        if path.contains('\\') {
            return Err(RepoPathError::Backslash);
        }
        if path.ends_with('/') {
            return Err(RepoPathError::TrailingSlash);
        }
        if let Some(bad) = path
            .split('/')
            .find(|s| s.is_empty() || *s == "." || *s == "..")
        {
            return Err(RepoPathError::BadSegment(bad.to_string()));
        }
        Ok(RepoPath(path))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// The last segment, for example `page.tsx`.
    pub fn file_name(&self) -> &str {
        self.0.rsplit('/').next().unwrap_or(&self.0)
    }

    /// The path of `child` inside this folder.
    pub fn join(&self, child: &str) -> Result<RepoPath, RepoPathError> {
        RepoPath::new(format!("{}/{child}", self.0))
    }
}

impl fmt::Display for RepoPath {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// Languages CodeAtlas analyses (same values as the contract's `Language`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Language {
    TypeScript,
    Tsx,
    JavaScript,
    Jsx,
}

/// Declaration files describe types only; the TypeScript analyser reads them itself, but they
/// are not part of the source inventory.
const DECLARATION_SUFFIXES: &[&str] = &[".d.ts", ".d.mts", ".d.cts"];

impl Language {
    /// Language of a file, decided by its extension; `None` for anything that is not
    /// analysable source (including declaration files).
    ///
    /// Extensions are matched exactly, in lower case, as TypeScript and Node.js do: `App.TSX`
    /// is not source.
    pub fn detect(path: &RepoPath) -> Option<Language> {
        let name = path.file_name();
        if DECLARATION_SUFFIXES.iter().any(|s| name.ends_with(s)) {
            return None;
        }
        let (stem, ext) = name.rsplit_once('.')?;
        if stem.is_empty() {
            return None; // a dotfile such as `.ts` has no extension
        }
        match ext {
            "ts" | "mts" | "cts" => Some(Language::TypeScript),
            "tsx" => Some(Language::Tsx),
            "js" | "mjs" | "cjs" => Some(Language::JavaScript),
            "jsx" => Some(Language::Jsx),
            _ => None,
        }
    }

    /// The contract spelling (`typescript`, `tsx`, `javascript`, `jsx`).
    pub fn as_str(self) -> &'static str {
        match self {
            Language::TypeScript => "typescript",
            Language::Tsx => "tsx",
            Language::JavaScript => "javascript",
            Language::Jsx => "jsx",
        }
    }
}

/// SHA-256 of a file's bytes.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ContentHash([u8; 32]);

impl ContentHash {
    pub fn from_bytes(bytes: [u8; 32]) -> Self {
        ContentHash(bytes)
    }

    pub fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }

    /// Lower-case hex, as the contract's `Sha256` expects.
    pub fn to_hex(&self) -> String {
        self.0.iter().map(|b| format!("{b:02x}")).collect()
    }
}

impl fmt::Debug for ContentHash {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "ContentHash({})", self.to_hex())
    }
}

/// One analysable source file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourceFile {
    pub path: RepoPath,
    pub language: Language,
    pub size_bytes: u64,
    pub content_hash: ContentHash,
}

/// Why something under the root is not in the inventory. Only entries a user may want to know
/// about are reported: excluded folders and source files that could not be used. Files that are
/// simply not source (images, JSON, `.d.ts`) and paths matched by the repository's own
/// `.gitignore` are left out silently.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SkipReason {
    /// A folder excluded by the [`IgnorePolicy`] (dependencies, build output, VCS metadata).
    IgnoredDirectory,
    /// A source file larger than the scan limit.
    TooLarge { size_bytes: u64, limit_bytes: u64 },
    /// A source file whose content is not valid UTF-8.
    NotUtf8,
    /// A symbolic link; links are never followed, so the inventory cannot escape the root.
    Symlink,
    /// A path whose name is not valid Unicode; the stored path is a lossy rendering.
    NonUnicodePath,
    /// The file or folder could not be read.
    Unreadable { message: String },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SkippedEntry {
    pub path: RepoPath,
    pub reason: SkipReason,
}

/// The result of scanning a repository: source files and reported skips, each sorted by path.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Inventory {
    files: Vec<SourceFile>,
    skipped: Vec<SkippedEntry>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InventoryError {
    DuplicatePath(RepoPath),
}

impl fmt::Display for InventoryError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            InventoryError::DuplicatePath(p) => write!(f, "{p} appears more than once"),
        }
    }
}

impl std::error::Error for InventoryError {}

impl Inventory {
    /// Sorts both lists by path, so the result never depends on the order a file system
    /// returns entries in. A path may appear only once across both lists.
    pub fn new(
        mut files: Vec<SourceFile>,
        mut skipped: Vec<SkippedEntry>,
    ) -> Result<Self, InventoryError> {
        files.sort_by(|a, b| a.path.cmp(&b.path));
        skipped.sort_by(|a, b| a.path.cmp(&b.path));
        let mut seen = BTreeSet::new();
        for path in files
            .iter()
            .map(|f| &f.path)
            .chain(skipped.iter().map(|s| &s.path))
        {
            if !seen.insert(path) {
                return Err(InventoryError::DuplicatePath(path.clone()));
            }
        }
        Ok(Inventory { files, skipped })
    }

    pub fn files(&self) -> &[SourceFile] {
        &self.files
    }

    pub fn skipped(&self) -> &[SkippedEntry] {
        &self.skipped
    }
}

/// Folder names that hold dependencies, build output, caches or VCS metadata in JavaScript and
/// TypeScript projects. They are skipped wherever they appear, at any depth.
pub const DEFAULT_IGNORED_DIR_NAMES: &[&str] = &[
    // version control
    ".git",
    ".hg",
    ".svn",
    // dependencies
    "node_modules",
    "bower_components",
    "jspm_packages",
    ".yarn",
    ".pnpm-store",
    // build output
    "dist",
    "build",
    "out",
    ".next",
    ".nuxt",
    ".output",
    ".svelte-kit",
    ".vercel",
    "storybook-static",
    // caches and reports
    "coverage",
    ".turbo",
    ".cache",
    ".parcel-cache",
];

/// Which folders a scan skips. The defaults suit JavaScript and TypeScript repositories; a
/// project can add folder names or exact paths, or re-include a default name it uses for real
/// source (some projects keep source in `build/`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IgnorePolicy {
    dir_names: BTreeSet<String>,
    dir_paths: BTreeSet<RepoPath>,
}

impl Default for IgnorePolicy {
    fn default() -> Self {
        IgnorePolicy {
            dir_names: DEFAULT_IGNORED_DIR_NAMES
                .iter()
                .map(|s| s.to_string())
                .collect(),
            dir_paths: BTreeSet::new(),
        }
    }
}

impl IgnorePolicy {
    /// A policy that skips nothing.
    pub fn none() -> Self {
        IgnorePolicy {
            dir_names: BTreeSet::new(),
            dir_paths: BTreeSet::new(),
        }
    }

    /// Also skip every folder with this name.
    pub fn ignore_name(mut self, name: impl Into<String>) -> Self {
        self.dir_names.insert(name.into());
        self
    }

    /// Also skip this one folder.
    pub fn ignore_path(mut self, path: RepoPath) -> Self {
        self.dir_paths.insert(path);
        self
    }

    /// Stop skipping folders with this name.
    pub fn include_name(mut self, name: &str) -> Self {
        self.dir_names.remove(name);
        self
    }

    pub fn ignores_dir(&self, dir: &RepoPath) -> bool {
        self.dir_names.contains(dir.file_name()) || self.dir_paths.contains(dir)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn p(s: &str) -> RepoPath {
        RepoPath::new(s).unwrap()
    }

    fn hash(n: u8) -> ContentHash {
        ContentHash::from_bytes([n; 32])
    }

    #[test]
    fn repo_paths_are_validated() {
        assert_eq!(p("web/src/app.tsx").as_str(), "web/src/app.tsx");
        assert_eq!(p("web/src/app.tsx").file_name(), "app.tsx");
        assert_eq!(p("web").join("src").unwrap(), p("web/src"));
        assert_eq!(RepoPath::new(""), Err(RepoPathError::Empty));
        assert_eq!(RepoPath::new("/etc/passwd"), Err(RepoPathError::Absolute));
        assert_eq!(RepoPath::new("web\\src"), Err(RepoPathError::Backslash));
        assert_eq!(RepoPath::new("web/"), Err(RepoPathError::TrailingSlash));
        for bad in ["web//src", "./web", "web/../etc", "web/."] {
            assert!(
                matches!(RepoPath::new(bad), Err(RepoPathError::BadSegment(_))),
                "{bad}"
            );
        }
    }

    #[test]
    fn languages_are_detected_by_extension() {
        let cases = [
            ("a.ts", Some(Language::TypeScript)),
            ("a.mts", Some(Language::TypeScript)),
            ("a.cts", Some(Language::TypeScript)),
            ("a.tsx", Some(Language::Tsx)),
            ("a.js", Some(Language::JavaScript)),
            ("a.mjs", Some(Language::JavaScript)),
            ("a.cjs", Some(Language::JavaScript)),
            ("a.jsx", Some(Language::Jsx)),
            ("web/[roomCode]/page.tsx", Some(Language::Tsx)),
            ("a.d.ts", None),
            ("a.d.mts", None),
            ("a.d.cts", None),
            ("a.json", None),
            ("a.TS", None),
            ("Makefile", None),
            (".ts", None),
            ("a.ts.map", None),
        ];
        for (path, want) in cases {
            assert_eq!(Language::detect(&p(path)), want, "{path}");
        }
        assert_eq!(Language::Tsx.as_str(), "tsx");
    }

    #[test]
    fn content_hash_is_lower_case_hex() {
        let mut bytes = [0u8; 32];
        bytes[0] = 0xab;
        bytes[31] = 0x01;
        let hex = ContentHash::from_bytes(bytes).to_hex();
        assert_eq!(hex.len(), 64);
        assert!(hex.starts_with("ab00") && hex.ends_with("0001"));
    }

    #[test]
    fn inventory_is_sorted_and_rejects_duplicates() {
        let file = |path: &str| SourceFile {
            path: p(path),
            language: Language::TypeScript,
            size_bytes: 1,
            content_hash: hash(1),
        };
        let skip = |path: &str| SkippedEntry {
            path: p(path),
            reason: SkipReason::IgnoredDirectory,
        };
        let inv = Inventory::new(
            vec![file("b.ts"), file("a/z.ts"), file("a.ts")],
            vec![skip("dist")],
        )
        .unwrap();
        let order: Vec<&str> = inv.files().iter().map(|f| f.path.as_str()).collect();
        assert_eq!(order, ["a.ts", "a/z.ts", "b.ts"]);
        assert_eq!(
            Inventory::new(vec![file("a.ts"), file("a.ts")], vec![]),
            Err(InventoryError::DuplicatePath(p("a.ts")))
        );
        assert_eq!(
            Inventory::new(vec![file("dist")], vec![skip("dist")]),
            Err(InventoryError::DuplicatePath(p("dist")))
        );
    }

    #[test]
    fn ignore_policy_matches_names_at_any_depth_and_exact_paths() {
        let policy = IgnorePolicy::default();
        assert!(policy.ignores_dir(&p("node_modules")));
        assert!(policy.ignores_dir(&p("packages/app/node_modules")));
        assert!(policy.ignores_dir(&p("backend/dist")));
        assert!(!policy.ignores_dir(&p("backend/src")));
        assert!(!policy.ignores_dir(&p("distribution")));

        let custom = IgnorePolicy::default()
            .include_name("build")
            .ignore_name("generated")
            .ignore_path(p("legacy/vendor"));
        assert!(!custom.ignores_dir(&p("build")));
        assert!(custom.ignores_dir(&p("src/generated")));
        assert!(custom.ignores_dir(&p("legacy/vendor")));
        assert!(!custom.ignores_dir(&p("vendor")));
        assert!(!IgnorePolicy::none().ignores_dir(&p("node_modules")));
    }
}
