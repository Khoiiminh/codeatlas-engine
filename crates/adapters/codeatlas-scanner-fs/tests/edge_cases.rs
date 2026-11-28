//! ENG-01 scanner behaviour on small repositories built in a temporary folder.

use std::fs;
use std::path::Path;

use codeatlas_application::{
    CancelFlag, NoProgress, Phase, Progress, ProgressReporter, RunContext, ScanError, ScanOptions,
    ScanRepository, ScanRequest, SourceScanner,
};
use codeatlas_core::{IgnorePolicy, Inventory, RepoPath, SkipReason};
use codeatlas_scanner_fs::FsSourceScanner;
use tempfile::TempDir;

fn write(root: &Path, path: &str, content: &[u8]) {
    let full = root.join(path);
    fs::create_dir_all(full.parent().unwrap()).unwrap();
    fs::write(full, content).unwrap();
}

fn repo(files: &[(&str, &str)]) -> TempDir {
    let dir = tempfile::tempdir().unwrap();
    for (path, content) in files {
        write(dir.path(), path, content.as_bytes());
    }
    dir
}

fn scan(root: &Path, options: &ScanOptions) -> Inventory {
    ScanRepository::new(&FsSourceScanner)
        .execute(root.to_path_buf(), options, &RunContext::detached())
        .unwrap()
}

fn paths(inventory: &Inventory) -> Vec<&str> {
    inventory.files().iter().map(|f| f.path.as_str()).collect()
}

fn skips(inventory: &Inventory) -> Vec<(&str, &SkipReason)> {
    inventory
        .skipped()
        .iter()
        .map(|s| (s.path.as_str(), &s.reason))
        .collect()
}

#[test]
fn keeps_source_files_and_skips_dependency_and_build_folders() {
    let dir = repo(&[
        ("src/a.ts", "export const a = 1;\n"),
        ("src/b.tsx", "export const B = () => null;\n"),
        ("lib/c.cjs", "module.exports = {};\n"),
        ("types/global.d.ts", "declare const x: number;\n"),
        ("README.md", "# readme\n"),
        ("node_modules/pkg/index.js", "module.exports = 1;\n"),
        ("packages/app/dist/out.js", "console.log(1);\n"),
        ("packages/app/src/main.mts", "export {};\n"),
        (".git/HEAD", "ref: refs/heads/main\n"),
        ("build/gen.ts", "export {};\n"),
        (".storybook/main.ts", "export default {};\n"),
    ]);
    let inv = scan(dir.path(), &ScanOptions::default());
    assert_eq!(
        paths(&inv),
        [
            ".storybook/main.ts",
            "lib/c.cjs",
            "packages/app/src/main.mts",
            "src/a.ts",
            "src/b.tsx"
        ]
    );
    assert_eq!(
        skips(&inv),
        [
            (".git", &SkipReason::IgnoredDirectory),
            ("build", &SkipReason::IgnoredDirectory),
            ("node_modules", &SkipReason::IgnoredDirectory),
            ("packages/app/dist", &SkipReason::IgnoredDirectory),
        ]
    );
}

#[test]
fn options_change_what_is_skipped() {
    let dir = repo(&[
        ("build/tool.ts", "export {};\n"),
        ("src/generated/api.ts", "export {};\n"),
        ("legacy/vendor/lib.js", "export {};\n"),
        ("src/app.ts", "export {};\n"),
    ]);
    let options = ScanOptions {
        include_dir_names: vec!["build".into()],
        ignore_dir_names: vec!["generated".into()],
        ignore_dir_paths: vec![RepoPath::new("legacy/vendor").unwrap()],
        max_file_bytes: None,
    };
    let inv = scan(dir.path(), &options);
    assert_eq!(paths(&inv), ["build/tool.ts", "src/app.ts"]);
    let skipped: Vec<&str> = skips(&inv).into_iter().map(|(p, _)| p).collect();
    assert_eq!(skipped, ["legacy/vendor", "src/generated"]);
}

#[test]
fn honours_gitignore_files_inside_the_repository() {
    let dir = repo(&[
        (".gitignore", "generated/\n*.gen.ts\n!keep.gen.ts\n"),
        ("generated/client.ts", "export {};\n"),
        ("src/api.gen.ts", "export {};\n"),
        ("src/keep.gen.ts", "export {};\n"),
        ("web/.gitignore", "local.ts\n"),
        ("web/local.ts", "export {};\n"),
        ("web/app.ts", "export {};\n"),
        ("local.ts", "export {};\n"),
    ]);
    let inv = scan(dir.path(), &ScanOptions::default());
    // web/.gitignore applies only below web/; gitignored paths are not reported.
    assert_eq!(paths(&inv), ["local.ts", "src/keep.gen.ts", "web/app.ts"]);
    assert!(inv.skipped().is_empty(), "{:?}", inv.skipped());
}

#[test]
fn ignores_gitignore_files_outside_the_root() {
    let outer = repo(&[
        (".gitignore", "*.ts\n"),
        ("repo/src/app.ts", "export {};\n"),
    ]);
    let inv = scan(&outer.path().join("repo"), &ScanOptions::default());
    assert_eq!(paths(&inv), ["src/app.ts"]);
}

#[test]
fn reports_large_and_non_utf8_source_files() {
    let dir = repo(&[("small.ts", "ok\n"), ("big.ts", "0123456789012345678\n")]);
    write(dir.path(), "latin1.js", &[0x63, 0x61, 0x66, 0xe9, 0x0a]);
    write(dir.path(), "image.png", &[0xff, 0xd8, 0xff]);
    let options = ScanOptions {
        max_file_bytes: Some(10),
        ..ScanOptions::default()
    };
    let inv = scan(dir.path(), &options);
    assert_eq!(paths(&inv), ["small.ts"]);
    assert_eq!(
        skips(&inv),
        [
            (
                "big.ts",
                &SkipReason::TooLarge {
                    size_bytes: 20,
                    limit_bytes: 10
                }
            ),
            ("latin1.js", &SkipReason::NotUtf8),
        ]
    );
}

#[test]
fn records_size_and_sha256() {
    let dir = repo(&[("hello.ts", "hello\n")]);
    let inv = scan(dir.path(), &ScanOptions::default());
    let file = &inv.files()[0];
    assert_eq!(file.size_bytes, 6);
    assert_eq!(
        file.content_hash.to_hex(),
        "5891b5b522d5df086d0ff0b110fbd9d21bb4fc7163af34d08286a2e846f6be03"
    );
}

#[test]
fn handles_unicode_and_bracketed_names() {
    let dir = repo(&[
        (
            "web/app/party/[roomCode]/page.tsx",
            "export default null;\n",
        ),
        ("src/héllo-世界.ts", "export {};\n"),
    ]);
    let inv = scan(dir.path(), &ScanOptions::default());
    assert_eq!(
        paths(&inv),
        ["src/héllo-世界.ts", "web/app/party/[roomCode]/page.tsx"]
    );
}

#[test]
fn a_root_named_like_an_ignored_folder_is_still_scanned() {
    let outer = repo(&[("dist/index.ts", "export {};\n")]);
    let inv = scan(&outer.path().join("dist"), &ScanOptions::default());
    assert_eq!(paths(&inv), ["index.ts"]);
}

#[test]
fn symlinks_are_reported_not_followed() {
    let dir = repo(&[("real/a.ts", "export {};\n")]);
    let linked = make_symlink(&dir.path().join("real/a.ts"), &dir.path().join("link.ts"))
        && make_dir_symlink(&dir.path().join("real"), &dir.path().join("mirror"));
    if !linked {
        // Windows without Developer Mode cannot create symlinks. CI can on every system (`rust`
        // on Linux, `portability` on Windows and macOS), so there it must not skip quietly.
        assert!(
            std::env::var_os("CI").is_none(),
            "CI must be able to create symlinks"
        );
        eprintln!("symlinks not available here; skipping");
        return;
    }
    let inv = scan(dir.path(), &ScanOptions::default());
    assert_eq!(paths(&inv), ["real/a.ts"]);
    assert_eq!(
        skips(&inv),
        [
            ("link.ts", &SkipReason::Symlink),
            ("mirror", &SkipReason::Symlink)
        ]
    );
}

#[cfg(unix)]
fn make_symlink(target: &Path, link: &Path) -> bool {
    std::os::unix::fs::symlink(target, link).is_ok()
}
#[cfg(unix)]
fn make_dir_symlink(target: &Path, link: &Path) -> bool {
    std::os::unix::fs::symlink(target, link).is_ok()
}
#[cfg(windows)]
fn make_symlink(target: &Path, link: &Path) -> bool {
    std::os::windows::fs::symlink_file(target, link).is_ok()
}
#[cfg(windows)]
fn make_dir_symlink(target: &Path, link: &Path) -> bool {
    std::os::windows::fs::symlink_dir(target, link).is_ok()
}

#[test]
fn root_problems_are_errors() {
    let dir = repo(&[("file.ts", "export {};\n")]);
    let request = |root: &Path| ScanRequest {
        root: root.to_path_buf(),
        policy: IgnorePolicy::default(),
        max_file_bytes: 1024,
    };
    let missing = dir.path().join("missing");
    assert_eq!(
        FsSourceScanner.scan(&request(&missing), &RunContext::detached()),
        Err(ScanError::RootNotFound(missing.clone()))
    );
    let file = dir.path().join("file.ts");
    assert_eq!(
        FsSourceScanner.scan(&request(&file), &RunContext::detached()),
        Err(ScanError::RootNotDirectory(file.clone()))
    );
}

#[test]
fn result_does_not_depend_on_creation_order() {
    let names = ["b.ts", "a/c.ts", "a.ts", "z/y/x.js"];
    let forward = repo(&names.iter().map(|n| (*n, "x\n")).collect::<Vec<_>>());
    let backward = repo(&names.iter().rev().map(|n| (*n, "x\n")).collect::<Vec<_>>());
    let a = scan(forward.path(), &ScanOptions::default());
    let b = scan(backward.path(), &ScanOptions::default());
    assert_eq!(a, b);
    assert_eq!(paths(&a), ["a.ts", "a/c.ts", "b.ts", "z/y/x.js"]);
}

/// ADR-0014: a cancelled scan returns no inventory, and progress counts source files.
#[test]
fn scans_can_be_cancelled_and_report_progress() {
    let dir = tempfile::tempdir().unwrap();
    let request = |root: &Path| ScanRequest {
        root: root.to_path_buf(),
        policy: IgnorePolicy::default(),
        max_file_bytes: 1024,
    };
    for i in 0..5 {
        fs::write(dir.path().join(format!("f{i}.ts")), "export {};\n").unwrap();
    }
    fs::write(dir.path().join("notes.md"), "not source").unwrap();

    struct Events(std::sync::Mutex<Vec<Progress>>);
    impl ProgressReporter for Events {
        fn report(&self, progress: Progress) {
            self.0.lock().unwrap().push(progress);
        }
    }
    let events = Events(std::sync::Mutex::new(Vec::new()));
    let run = RunContext::new(&codeatlas_application::NeverCancelled, &events);
    let inventory = FsSourceScanner.scan(&request(dir.path()), &run).unwrap();
    assert_eq!(inventory.files().len(), 5);
    let done: Vec<(Phase, usize, Option<usize>)> = events
        .0
        .into_inner()
        .unwrap()
        .iter()
        .map(|p| (p.phase, p.done, p.total))
        .collect();
    assert_eq!(
        done,
        (1..=5).map(|n| (Phase::Scan, n, None)).collect::<Vec<_>>()
    );

    let flag = CancelFlag::new();
    flag.cancel();
    let cancelled =
        FsSourceScanner.scan(&request(dir.path()), &RunContext::new(&flag, &NoProgress));
    assert_eq!(cancelled, Err(ScanError::Cancelled));
}
