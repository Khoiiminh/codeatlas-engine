//! ENG-02 parser fixtures: every file in `tests/cases` is parsed, and the syntax problems
//! must equal its `<file>.expected.json` exactly. See `tests/cases/README.md`.
//!
//! To write the expected files after a deliberate change, run the test with
//! `CODEATLAS_BLESS=1`, then review every changed `.expected.json` before committing it.

use std::fs;
use std::path::{Path, PathBuf};

use codeatlas_application::{LanguageParser, ParseInput, ParseLimits};
use codeatlas_core::{Language, ParsedSource, RepoPath, SyntaxProblemKind};
use codeatlas_parser_treesitter::TreeSitterParser;
use serde_json::{Value, json};

const EXPECTED_SUFFIX: &str = ".expected.json";

fn cases_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/cases")
}

/// Source files in `tests/cases`, sorted. README and expectation files are not cases.
fn case_files() -> Vec<String> {
    let mut names: Vec<String> = fs::read_dir(cases_dir())
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .filter(|n| !n.ends_with(EXPECTED_SUFFIX) && n != "README.md")
        .collect();
    names.sort();
    names
}

fn parse_case(name: &str) -> ParsedSource {
    let path = RepoPath::new(name).unwrap();
    let language = Language::detect(&path).unwrap_or_else(|| panic!("{name}: not a source file"));
    let text = fs::read_to_string(cases_dir().join(name)).unwrap();
    TreeSitterParser
        .parse(
            &ParseInput {
                path: &path,
                language,
                text: &text,
            },
            &ParseLimits::default(),
        )
        .unwrap_or_else(|e| panic!("{name}: {e}"))
}

/// `line:character-line:character`, zero-based like the contract.
fn range_text(parsed: &codeatlas_core::SyntaxProblem) -> String {
    let (s, e) = (parsed.range.start(), parsed.range.end());
    format!("{}:{}-{}:{}", s.line, s.character, e.line, e.character)
}

fn to_json(parsed: &ParsedSource) -> Value {
    let problems: Vec<Value> = parsed
        .problems()
        .iter()
        .map(|p| match &p.kind {
            SyntaxProblemKind::Unexpected => {
                json!({ "kind": "unexpected", "range": range_text(p) })
            }
            SyntaxProblemKind::Missing { expected } => {
                json!({ "kind": "missing", "expected": expected, "range": range_text(p) })
            }
        })
        .collect();
    json!({ "problems": problems, "truncated": parsed.problems_truncated() })
}

#[test]
fn every_case_matches_its_expected_problems() {
    let bless = std::env::var_os("CODEATLAS_BLESS").is_some();
    let names = case_files();
    assert!(
        names.len() >= 10,
        "expected the parser cases, found {names:?}"
    );
    let mut failures = Vec::new();
    for name in &names {
        let got = to_json(&parse_case(name));
        let expected_path = cases_dir().join(format!("{name}{EXPECTED_SUFFIX}"));
        if bless {
            let text = serde_json::to_string_pretty(&got).unwrap() + "\n";
            fs::write(&expected_path, text).unwrap();
            continue;
        }
        let Ok(text) = fs::read_to_string(&expected_path) else {
            failures.push(format!("{name}: no {name}{EXPECTED_SUFFIX}"));
            continue;
        };
        let want: Value = serde_json::from_str(&text).unwrap();
        if got != want {
            failures.push(format!("{name}:\n  expected {want}\n  got      {got}"));
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[test]
fn clean_cases_are_clean_and_broken_cases_are_not() {
    for name in case_files() {
        let parsed = parse_case(&name);
        assert_eq!(
            parsed.is_clean(),
            name.starts_with("clean-"),
            "{name}: the file name says whether it should parse cleanly"
        );
    }
}

#[test]
fn every_expectation_has_a_case() {
    let names = case_files();
    for entry in fs::read_dir(cases_dir()).unwrap() {
        let file = entry.unwrap().file_name().to_string_lossy().into_owned();
        if let Some(case) = file.strip_suffix(EXPECTED_SUFFIX) {
            assert!(names.iter().any(|n| n == case), "{file} has no case file");
        }
    }
}
