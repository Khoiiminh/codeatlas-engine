//! ENG-04 end to end: scan each FND-04 fixture, parse it, resolve it with the TypeScript
//! analyzer through the `ResolveSemantics` use case, and compare the relationships with the
//! `edges` and `unresolved` lists of its `expected.json`: the same kinds, sources, targets,
//! files, lines, type-only flags, specifiers and reasons, and nothing more or less. Each
//! relationship must also point at the name or specifier inside the golden `text` on its line,
//! and the external modules must be exactly `externalModules`.
//!
//! The analyzer needs Node and `npm ci` in `packages/typescript-analyzer`. Without them the
//! test skips on a developer's machine and fails in CI (ADR-0014 decision 7).

mod semantic_support;

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use codeatlas_core::{Endpoint, RelationTarget, Relationship};
use semantic_support::{analyse, analyzer};
use serde_json::Value;

const NOT_FIXTURES: &[&str] = &["src", "tests", "target"];

fn fixtures() -> Vec<(String, Value, PathBuf)> {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    let mut found: Vec<(String, Value, PathBuf)> = fs::read_dir(dir)
        .unwrap()
        .map(|e| e.unwrap())
        .filter(|e| e.path().is_dir())
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .filter(|n| !NOT_FIXTURES.contains(&n.as_str()))
        .map(|name| {
            let expected: Value = serde_json::from_str(
                &fs::read_to_string(dir.join(&name).join("expected.json")).unwrap(),
            )
            .unwrap();
            let root = dir.join(&name).join(expected["root"].as_str().unwrap());
            (name, expected, root)
        })
        .collect();
    found.sort_by(|a, b| a.0.cmp(&b.0));
    assert!(!found.is_empty(), "no fixtures");
    found
}

/// A relationship in the golden file's terms: one line of `edges` or of `unresolved`.
fn key(r: &Relationship) -> String {
    let line = r.location.range.start().line + 1;
    let path = &r.location.path;
    match &r.target {
        RelationTarget::Resolved(to) => format!(
            "{} {} -> {} at {path}:{line}{}",
            r.kind,
            r.from.id(),
            to.id(),
            if r.type_only { " (type only)" } else { "" }
        ),
        RelationTarget::Unresolved { specifier, reason } => format!(
            "{} {} -> ? {specifier} ({reason}) at {path}:{line}",
            r.kind,
            r.from.id()
        ),
    }
}

fn golden_keys(expected: &Value) -> Vec<(String, String)> {
    let mut keys = Vec::new();
    for e in expected["edges"].as_array().unwrap() {
        let type_only = e.get("typeOnly").is_some_and(|t| t == true);
        keys.push((
            format!(
                "{} {} -> {} at {}:{}{}",
                e["kind"].as_str().unwrap(),
                e["from"].as_str().unwrap(),
                e["to"].as_str().unwrap(),
                e["path"].as_str().unwrap(),
                e["line"],
                if type_only { " (type only)" } else { "" }
            ),
            e["text"].as_str().unwrap().to_string(),
        ));
    }
    for u in expected["unresolved"].as_array().unwrap() {
        keys.push((
            format!(
                "{} {} -> ? {} ({}) at {}:{}",
                u["kind"].as_str().unwrap(),
                u["from"].as_str().unwrap(),
                u["specifier"].as_str().unwrap(),
                u["reason"].as_str().unwrap(),
                u["path"].as_str().unwrap(),
                u["line"]
            ),
            u["text"].as_str().unwrap().to_string(),
        ));
    }
    keys
}

/// The UTF-16 span of `text` on the given zero-based line of a fixture file, if it is there.
fn span_on_line(root: &Path, path: &str, line: u32, text: &str) -> Option<(u32, u32)> {
    let source = fs::read_to_string(root.join(path)).unwrap();
    let source = source.strip_prefix('\u{feff}').unwrap_or(&source);
    let line_text = source
        .split("\r\n")
        .flat_map(|l| l.split(['\n', '\r']))
        .nth(line as usize)?;
    let at = line_text.find(text)?;
    let start = line_text[..at].encode_utf16().count() as u32;
    Some((start, start + text.encode_utf16().count() as u32))
}

#[test]
fn every_fixture_relationship_is_resolved_exactly() {
    let Some(resolver) = analyzer() else { return };
    let mut compared = 0;
    for (name, expected, root) in fixtures() {
        let report = analyse(&root, &resolver);
        assert_eq!(report.unavailable, None, "[{name}]");
        assert!(report.failed.is_empty(), "[{name}] {:?}", report.failed);
        assert!(report.problems.is_empty(), "[{name}] {:?}", report.problems);

        let mut got: BTreeMap<String, Vec<&Relationship>> = BTreeMap::new();
        for r in &report.relationships {
            got.entry(key(r)).or_default().push(r);
        }
        let mut want: BTreeMap<String, Vec<String>> = BTreeMap::new();
        for (k, text) in golden_keys(&expected) {
            want.entry(k).or_default().push(text);
        }
        let mut missing = Vec::new();
        let mut extra = Vec::new();
        for (k, texts) in &want {
            let n = got.get(k).map_or(0, Vec::len);
            if n < texts.len() {
                missing.push(format!("{k} [{}x]", texts.len() - n));
            }
        }
        for (k, rels) in &got {
            let n = want.get(k).map_or(0, Vec::len);
            if rels.len() > n {
                extra.push(format!("{k} [{}x]", rels.len() - n));
            }
        }
        assert!(
            missing.is_empty() && extra.is_empty(),
            "[{name}]\n  missing:\n    {}\n  extra:\n    {}",
            missing.join("\n    "),
            extra.join("\n    ")
        );

        // Every relationship points into the golden text on its line.
        for (k, rels) in &got {
            let texts = &want[k];
            for r in rels {
                let range = r.location.range;
                assert_eq!(range.start().line, range.end().line, "[{name}] {k}");
                let inside = texts.iter().any(|text| {
                    span_on_line(&root, r.location.path.as_str(), range.start().line, text)
                        .is_some_and(|(from, to)| {
                            from <= range.start().character && range.end().character <= to
                        })
                });
                assert!(
                    inside,
                    "[{name}] {k}: {}..{} is not inside {texts:?}",
                    range.start(),
                    range.end()
                );
            }
        }

        let mut externals: Vec<String> = report
            .relationships
            .iter()
            .filter_map(|r| match &r.target {
                RelationTarget::Resolved(Endpoint::External(s)) => Some(s.clone()),
                _ => None,
            })
            .collect();
        externals.sort();
        externals.dedup();
        let mut want_externals: Vec<String> = expected["externalModules"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_str().unwrap().to_string())
            .collect();
        want_externals.sort();
        assert_eq!(externals, want_externals, "[{name}] external modules");
        compared += report.relationships.len();
    }
    assert!(
        compared >= 96,
        "expected the fixture relationships, compared {compared}"
    );
}
