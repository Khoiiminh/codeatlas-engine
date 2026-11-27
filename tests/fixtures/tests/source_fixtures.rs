//! FND-04: every source fixture under `tests/fixtures/<name>/` agrees with its `expected.json`.
//!
//! A fixture is any folder here other than `src` and `tests`; each must contain `expected.json`.
//! These checks keep the golden files honest before any analyser exists: every file, symbol,
//! edge and unresolved reference listed must point at real source text. Coverage of the FND-04
//! exit criteria is required of the fixture set as a whole, so a new fixture may stay small and
//! focus on one project shape. Whether the TypeScript compiler agrees is checked separately by
//! `packages/typescript-analyzer/scripts/check-fixtures.ts`.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};

use codeatlas_fixture_tests::{engine_root, read_json};
use serde_json::Value;

/// Analysable source extensions and their contract `Language`. Declaration files are excluded.
const SOURCE_EXTENSIONS: &[(&str, &str)] = &[
    ("ts", "typescript"),
    ("mts", "typescript"),
    ("cts", "typescript"),
    ("tsx", "tsx"),
    ("js", "javascript"),
    ("mjs", "javascript"),
    ("cjs", "javascript"),
    ("jsx", "jsx"),
];
const DECLARATION_SUFFIXES: &[&str] = &[".d.ts", ".d.mts", ".d.cts"];
/// Folders under `tests/fixtures` that hold this crate, not fixtures.
const NOT_FIXTURES: &[&str] = &["src", "tests", "target"];

struct Fixture {
    name: String,
    dir: PathBuf,
    expected: Value,
    sources: BTreeMap<String, Vec<String>>,
}

fn str_of<'a>(v: &'a Value, key: &str) -> &'a str {
    v[key]
        .as_str()
        .unwrap_or_else(|| panic!("missing '{key}' in {v}"))
}

fn language_of(path: &str) -> Option<&'static str> {
    if DECLARATION_SUFFIXES.iter().any(|s| path.ends_with(s)) {
        return None;
    }
    let ext = path.rsplit_once('.')?.1;
    SOURCE_EXTENSIONS
        .iter()
        .find(|(e, _)| *e == ext)
        .map(|(_, l)| *l)
}

fn fixtures_dir() -> PathBuf {
    engine_root().join("tests/fixtures")
}

/// Every fixture, sorted by name. Panics on a folder without `expected.json`, so a fixture
/// cannot be added and then silently skipped.
fn fixtures() -> Vec<Fixture> {
    let mut dirs: Vec<PathBuf> = fs::read_dir(fixtures_dir())
        .expect("tests/fixtures")
        .map(|e| e.expect("entry").path())
        .filter(|p| p.is_dir())
        .filter(|p| !NOT_FIXTURES.contains(&p.file_name().unwrap().to_str().unwrap()))
        .collect();
    dirs.sort();
    assert!(!dirs.is_empty(), "no source fixtures under tests/fixtures");

    dirs.into_iter()
        .map(|dir| {
            let name = dir.file_name().unwrap().to_string_lossy().into_owned();
            let path = dir.join("expected.json");
            assert!(
                path.is_file(),
                "tests/fixtures/{name} has no expected.json; every folder here is a fixture"
            );
            let expected = read_json(&path);
            assert_eq!(
                str_of(&expected, "fixture"),
                name,
                "[{name}] 'fixture' must equal the folder name"
            );
            let root = dir.join(str_of(&expected, "root"));
            let mut sources = BTreeMap::new();
            for file in expected["files"].as_array().unwrap() {
                let path = str_of(file, "path");
                let text = fs::read_to_string(root.join(path))
                    .unwrap_or_else(|e| panic!("[{name}] {path}: {e}"));
                assert!(
                    !text.contains('\r'),
                    "[{name}] {path}: must use LF line endings (lines are golden)"
                );
                sources.insert(
                    path.to_string(),
                    text.split('\n').map(str::to_string).collect(),
                );
            }
            Fixture {
                name,
                dir,
                expected,
                sources,
            }
        })
        .collect()
}

impl Fixture {
    fn root(&self) -> PathBuf {
        self.dir.join(str_of(&self.expected, "root"))
    }

    fn array(&self, key: &str) -> &Vec<Value> {
        self.expected[key].as_array().unwrap()
    }

    /// 1-based line text, or a panic naming the entry.
    fn line(&self, entry: &Value) -> &str {
        let name = &self.name;
        let path = str_of(entry, "path");
        let line = entry["line"].as_u64().unwrap() as usize;
        let lines = self
            .sources
            .get(path)
            .unwrap_or_else(|| panic!("[{name}] {path} is not a listed file"));
        assert!(
            line >= 1 && line <= lines.len(),
            "[{name}] {path}:{line} is outside the file"
        );
        &lines[line - 1]
    }

    fn file_ids(&self) -> BTreeSet<String> {
        self.sources.keys().map(|p| format!("file:{p}")).collect()
    }

    fn symbol_ids(&self) -> BTreeSet<String> {
        self.array("symbols")
            .iter()
            .map(|s| str_of(s, "id").to_string())
            .collect()
    }

    fn external_ids(&self) -> BTreeSet<String> {
        self.array("externalModules")
            .iter()
            .map(|m| format!("external:{}", m.as_str().unwrap()))
            .collect()
    }
}

fn walk(root: &Path, dir: &Path, ignored: &[String], out: &mut Vec<String>) {
    let mut entries: Vec<_> = fs::read_dir(dir)
        .unwrap()
        .map(|e| e.unwrap().path())
        .collect();
    entries.sort();
    for path in entries {
        let rel = path
            .strip_prefix(root)
            .unwrap()
            .to_string_lossy()
            .replace('\\', "/");
        if path.is_dir() {
            if !ignored
                .iter()
                .any(|i| format!("{rel}/").starts_with(i.as_str()))
            {
                walk(root, &path, ignored, out);
            }
        } else if language_of(&rel).is_some() {
            out.push(rel);
        }
    }
}

fn collect_any(dir: &Path, out: &mut Vec<String>) {
    for entry in fs::read_dir(dir).unwrap_or_else(|e| panic!("{}: {e}", dir.display())) {
        let path = entry.unwrap().path();
        if path.is_dir() {
            collect_any(&path, out);
        } else {
            out.push(path.to_string_lossy().replace('\\', "/"));
        }
    }
}

#[test]
fn expectations_match_their_schema() {
    let schema = read_json(&fixtures_dir().join("expected.schema.json"));
    let validator = jsonschema::options()
        .should_validate_formats(true)
        .build(&schema)
        .expect("expectations schema compiles");
    for f in fixtures() {
        let problems: Vec<String> = validator
            .iter_errors(&f.expected)
            .map(|e| format!("{} at {}", e, e.instance_path()))
            .collect();
        assert!(problems.is_empty(), "[{}] {problems:#?}", f.name);
    }
}

#[test]
fn file_inventory_is_exact_and_ignored_folders_exist() {
    for f in fixtures() {
        let name = &f.name;
        let root = f.root();
        let ignored: Vec<String> = f
            .array("ignoredPaths")
            .iter()
            .map(|p| p.as_str().unwrap().to_string())
            .collect();

        let mut found = Vec::new();
        walk(&root, &root, &ignored, &mut found);
        let listed: Vec<String> = f.sources.keys().cloned().collect();
        assert_eq!(
            found, listed,
            "[{name}] files on disk and expected.json differ"
        );

        for file in f.array("files") {
            let path = str_of(file, "path");
            assert_eq!(
                Some(str_of(file, "language")),
                language_of(path),
                "[{name}] {path}: wrong language"
            );
        }

        // An ignored folder must exist and hold source-like files, or the scanner's ignore
        // rule (ENG-01) would be tested against nothing.
        for dir in &ignored {
            let mut inside = Vec::new();
            collect_any(&root.join(dir.trim_end_matches('/')), &mut inside);
            assert!(
                inside.iter().any(|p| SOURCE_EXTENSIONS
                    .iter()
                    .any(|(e, _)| p.ends_with(&format!(".{e}")))),
                "[{name}] ignored folder {dir} has no source-like files"
            );
        }
    }
}

#[test]
fn symbols_point_at_their_declarations() {
    for f in fixtures() {
        let name = &f.name;
        let ids = f.symbol_ids();
        assert_eq!(
            ids.len(),
            f.array("symbols").len(),
            "[{name}] duplicate symbol id"
        );
        for s in f.array("symbols") {
            let id = str_of(s, "id");
            let path = str_of(s, "path");
            let symbol = str_of(s, "name");
            let qualified = id
                .strip_prefix(&format!("symbol:{path}#"))
                .unwrap_or_else(|| {
                    panic!("[{name}] {id}: id must be symbol:<path>#<qualifiedName>")
                });
            assert_eq!(
                qualified.rsplit('.').next().unwrap(),
                symbol,
                "[{name}] {id}: name is not the last segment"
            );
            match s.get("parent").and_then(Value::as_str) {
                Some(parent) => {
                    assert!(
                        ids.contains(parent),
                        "[{name}] {id}: unknown parent {parent}"
                    );
                    assert!(
                        parent.starts_with(&format!("symbol:{path}#")),
                        "[{name}] {id}: parent in another file"
                    );
                    assert_eq!(
                        s["exported"],
                        Value::Bool(false),
                        "[{name}] {id}: members are never module exports"
                    );
                }
                None => assert!(
                    !qualified.contains('.'),
                    "[{name}] {id}: dotted name without a parent"
                ),
            }
            assert!(
                f.line(s).contains(symbol),
                "[{name}] {id}: line {} does not mention {symbol}",
                s["line"]
            );
        }
    }
}

#[test]
fn edges_point_at_real_text_and_known_endpoints() {
    for f in fixtures() {
        let name = &f.name;
        let files = f.file_ids();
        let symbols = f.symbol_ids();
        let externals = f.external_ids();
        let known = |id: &str| files.contains(id) || symbols.contains(id) || externals.contains(id);

        let mut seen = BTreeSet::new();
        let mut used_externals = BTreeSet::new();
        for e in f.array("edges") {
            let (kind, from, to, text) = (
                str_of(e, "kind"),
                str_of(e, "from"),
                str_of(e, "to"),
                str_of(e, "text"),
            );
            assert!(known(from), "[{name}] {kind} edge from unknown {from}");
            assert!(known(to), "[{name}] {kind} edge to unknown {to}");
            assert!(
                f.line(e).contains(text),
                "[{name}] {kind} {from} -> {to}: line does not contain {text:?}"
            );
            assert!(seen.insert(e.to_string()), "[{name}] duplicate edge {e}");
            if externals.contains(to) {
                used_externals.insert(to.to_string());
            }
            match kind {
                "imports" | "exports" => {
                    let spec = str_of(e, "specifier");
                    assert_eq!(
                        from,
                        format!("file:{}", str_of(e, "path")),
                        "[{name}] {kind} edges start at the importing file"
                    );
                    assert!(
                        text.contains(&format!("\"{spec}\""))
                            || text.contains(&format!("'{spec}'")),
                        "[{name}] {kind}: text must contain the quoted specifier {spec:?}"
                    );
                    assert!(
                        !to.starts_with("symbol:"),
                        "[{name}] {kind} edges end at files or external modules"
                    );
                }
                "extends" | "implements" => {
                    assert!(
                        symbols.contains(from) && symbols.contains(to),
                        "[{name}] {kind} joins two symbols"
                    );
                    let target = to.rsplit(['#', '.']).next().unwrap();
                    assert!(
                        text.contains(target),
                        "[{name}] {kind}: text must name {target}"
                    );
                }
                "calls" | "references" => {
                    assert!(
                        symbols.contains(to),
                        "[{name}] {kind} edges end at symbols: {to}"
                    );
                    let from_path = from
                        .trim_start_matches("file:")
                        .trim_start_matches("symbol:");
                    assert!(
                        from_path.starts_with(str_of(e, "path")),
                        "[{name}] {kind}: the line must be in the file of {from}"
                    );
                }
                other => panic!("[{name}] unexpected edge kind {other}"),
            }
        }
        assert_eq!(
            used_externals, externals,
            "[{name}] externalModules must list exactly the external import targets"
        );
    }
}

#[test]
fn unresolved_references_point_at_real_text() {
    for f in fixtures() {
        let name = &f.name;
        let files = f.file_ids();
        let symbols = f.symbol_ids();
        for u in f.array("unresolved") {
            let (from, text, spec) = (str_of(u, "from"), str_of(u, "text"), str_of(u, "specifier"));
            assert!(
                files.contains(from) || symbols.contains(from),
                "[{name}] unresolved from unknown {from}"
            );
            assert!(
                f.line(u).contains(text),
                "[{name}] {from}: line does not contain {text:?}"
            );
            assert!(
                text.contains(spec),
                "[{name}] {from}: text {text:?} must contain the specifier {spec:?}"
            );
            if str_of(u, "kind") == "imports" {
                assert_eq!(
                    from,
                    format!("file:{}", str_of(u, "path")),
                    "[{name}] unresolved imports start at the importing file"
                );
            }
        }
        for d in f.array("typescriptDiagnostics") {
            f.line(d);
        }
    }
}

type EdgePredicate = dyn Fn(&Value) -> bool;

/// The FND-04 exit criteria hold for the fixture set as a whole.
#[test]
fn fixture_set_covers_the_fnd04_exit_criteria() {
    let all = fixtures();
    let collect = |key: &str, field: &str| -> BTreeSet<String> {
        all.iter()
            .flat_map(|f| f.array(key).iter().map(|v| str_of(v, field).to_string()))
            .collect()
    };
    let set = |items: &[&str]| items.iter().map(|s| s.to_string()).collect::<BTreeSet<_>>();
    let missing = |got: BTreeSet<String>, want: BTreeSet<String>| -> Vec<String> {
        want.difference(&got).cloned().collect()
    };

    assert_eq!(
        missing(
            collect("files", "language"),
            set(&["javascript", "jsx", "tsx", "typescript"])
        ),
        Vec::<String>::new(),
        "languages not covered"
    );
    assert_eq!(
        missing(
            collect("symbols", "kind"),
            set(&[
                "class",
                "enum",
                "function",
                "interface",
                "method",
                "namespace",
                "property",
                "type_alias",
                "variable"
            ])
        ),
        Vec::<String>::new(),
        "symbol kinds not covered"
    );
    assert_eq!(
        missing(
            collect("edges", "kind"),
            set(&[
                "calls",
                "exports",
                "extends",
                "implements",
                "imports",
                "references"
            ])
        ),
        Vec::<String>::new(),
        "edge kinds not covered"
    );
    assert_eq!(
        missing(
            collect("unresolved", "reason"),
            set(&["ambiguous", "dynamic", "not_found", "unsupported"])
        ),
        Vec::<String>::new(),
        "unresolved reasons not covered"
    );

    let edges: Vec<&Value> = all.iter().flat_map(|f| f.array("edges")).collect();
    let has = |pred: &dyn Fn(&Value) -> bool| edges.iter().any(|e| pred(e));
    let cases: [(&str, &EdgePredicate); 8] = [
        ("type-only import", &|e| {
            e.get("typeOnly").is_some() && str_of(e, "kind") == "imports"
        }),
        ("path-alias import", &|e| {
            str_of(e, "kind") == "imports" && str_of(e, "specifier").starts_with("@/")
        }),
        ("namespace re-export", &|e| {
            str_of(e, "kind") == "exports" && str_of(e, "text").contains("* as")
        }),
        ("ESM importing CommonJS", &|e| {
            str_of(e, "kind") == "imports"
                && str_of(e, "path").ends_with(".mjs")
                && str_of(e, "to").ends_with(".cjs")
        }),
        ("JSX element in TSX", &|e| {
            str_of(e, "kind") == "references"
                && str_of(e, "text").starts_with('<')
                && str_of(e, "path").ends_with(".tsx")
        }),
        ("JSX component declared in a .jsx file", &|e| {
            str_of(e, "kind") == "references" && str_of(e, "to").contains(".jsx#")
        }),
        ("TypeScript calling JavaScript", &|e| {
            str_of(e, "kind") == "calls"
                && str_of(e, "to").contains(".js#")
                && str_of(e, "path").ends_with(".tsx")
        }),
        ("top-level call", &|e| {
            str_of(e, "kind") == "calls" && str_of(e, "from").starts_with("file:")
        }),
    ];
    for (case, pred) in cases {
        assert!(has(pred), "no fixture covers: {case}");
    }
}
