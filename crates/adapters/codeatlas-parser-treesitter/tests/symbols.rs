//! ENG-03 symbol fixtures: every file in `tests/symbols` is parsed, and its symbols must equal
//! `<file>.symbols.json` exactly. See `tests/symbols/README.md`; the TypeScript compiler
//! cross-checks the same files (`npm run check:symbol-cases` in packages/typescript-analyzer).
//!
//! To write the expected files after a deliberate change, run the test with
//! `CODEATLAS_BLESS=1`, then review every changed `.symbols.json` before committing it.

use std::fs;
use std::path::{Path, PathBuf};

use codeatlas_application::{LanguageParser, NeverCancelled, ParseInput, ParseLimits};
use codeatlas_core::{Language, ParsedSource, Range, RepoPath, SymbolLimit};
use codeatlas_parser_treesitter::TreeSitterParser;
use serde_json::{Value, json};

const EXPECTED_SUFFIX: &str = ".symbols.json";

fn cases_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/symbols")
}

/// Source files in `tests/symbols`, sorted. README and expectation files are not cases.
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
            &NeverCancelled,
        )
        .unwrap_or_else(|e| panic!("{name}: {e}"))
}

/// `line:character-line:character`, zero-based like the contract.
fn range_text(range: Range) -> String {
    let (s, e) = (range.start(), range.end());
    format!("{}:{}-{}:{}", s.line, s.character, e.line, e.character)
}

fn to_json(parsed: &ParsedSource) -> Value {
    let symbols: Vec<Value> = parsed
        .symbols()
        .iter()
        .map(|s| {
            let declarations: Vec<Value> = s
                .declarations()
                .iter()
                .map(|d| {
                    json!({
                        "kind": d.kind.as_str(),
                        "range": range_text(d.range),
                        "name": range_text(d.name_range),
                    })
                })
                .collect();
            let mut entry = json!({
                "id": s.id().as_str(),
                "name": s.name(),
                "kind": s.kind().as_str(),
                "exported": s.exported(),
                "declarations": declarations,
            });
            if let Some(parent) = s.parent() {
                entry["parent"] = json!(parent.as_str());
            }
            entry
        })
        .collect();
    let truncated = parsed.symbols_truncated().map(|t| {
        let limit = match t.limit {
            SymbolLimit::Count { limit } => format!("count:{limit}"),
            SymbolLimit::NamespaceDepth { limit } => format!("namespace-depth:{limit}"),
        };
        json!({ "at": format!("{}:{}", t.at.line, t.at.character), "limit": limit })
    });
    json!({ "symbols": symbols, "truncated": truncated })
}

#[test]
fn every_case_matches_its_expected_symbols() {
    let bless = std::env::var_os("CODEATLAS_BLESS").is_some();
    let names = case_files();
    assert!(
        names.len() >= 10,
        "expected the symbol cases, found {names:?}"
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
            failures.push(format!(
                "{name}:\n  expected {}\n  got      {}",
                serde_json::to_string_pretty(&want).unwrap(),
                serde_json::to_string_pretty(&got).unwrap()
            ));
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[test]
fn broken_cases_have_syntax_problems_and_the_others_do_not() {
    for name in case_files() {
        assert_eq!(
            !parse_case(&name).is_clean(),
            name.starts_with("broken-"),
            "{name}: the file name says whether it has syntax errors"
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
