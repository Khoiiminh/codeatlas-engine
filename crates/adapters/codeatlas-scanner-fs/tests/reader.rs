//! ENG-02: `FsSourceReader` returns exactly the content the scanner inventoried.

use std::fs;

use codeatlas_application::{ReadError, RunContext, ScanOptions, ScanRepository, SourceReader};
use codeatlas_core::{Inventory, SourceFile};
use codeatlas_scanner_fs::{FsSourceReader, FsSourceScanner};
use tempfile::TempDir;

fn scanned(files: &[(&str, &str)]) -> (TempDir, Inventory) {
    let dir = tempfile::tempdir().unwrap();
    for (path, content) in files {
        let full = dir.path().join(path);
        fs::create_dir_all(full.parent().unwrap()).unwrap();
        fs::write(full, content).unwrap();
    }
    let inventory = ScanRepository::new(&FsSourceScanner)
        .execute(
            dir.path().to_path_buf(),
            &ScanOptions::default(),
            &RunContext::detached(),
        )
        .unwrap();
    (dir, inventory)
}

fn file<'a>(inventory: &'a Inventory, path: &str) -> &'a SourceFile {
    inventory
        .files()
        .iter()
        .find(|f| f.path.as_str() == path)
        .unwrap()
}

#[test]
fn reads_unchanged_files() {
    let (dir, inventory) = scanned(&[("src/a.ts", "export const a = 1;\n"), ("b.js", "é\r\n")]);
    assert_eq!(
        FsSourceReader.read(dir.path(), file(&inventory, "src/a.ts")),
        Ok("export const a = 1;\n".to_string())
    );
    assert_eq!(
        FsSourceReader.read(dir.path(), file(&inventory, "b.js")),
        Ok("é\r\n".to_string())
    );
}

#[test]
fn a_file_edited_after_the_scan_is_refused() {
    let (dir, inventory) = scanned(&[("same-size.ts", "const a = 1;\n"), ("grown.ts", "x\n")]);
    // Same length, different content: only the hash can tell.
    fs::write(dir.path().join("same-size.ts"), "const b = 2;\n").unwrap();
    fs::write(dir.path().join("grown.ts"), "x\ny\n").unwrap();
    for path in ["same-size.ts", "grown.ts"] {
        assert_eq!(
            FsSourceReader.read(dir.path(), file(&inventory, path)),
            Err(ReadError::ChangedSinceScan),
            "{path}"
        );
    }
}

#[test]
fn a_deleted_or_replaced_file_is_refused() {
    let (dir, inventory) = scanned(&[("gone.ts", "x\n"), ("now-a-dir.ts", "x\n")]);
    fs::remove_file(dir.path().join("gone.ts")).unwrap();
    fs::remove_file(dir.path().join("now-a-dir.ts")).unwrap();
    fs::create_dir(dir.path().join("now-a-dir.ts")).unwrap();
    assert!(matches!(
        FsSourceReader.read(dir.path(), file(&inventory, "gone.ts")),
        Err(ReadError::Io(_))
    ));
    assert_eq!(
        FsSourceReader.read(dir.path(), file(&inventory, "now-a-dir.ts")),
        Err(ReadError::ChangedSinceScan)
    );
}
