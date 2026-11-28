//! ENG-03: symbol extraction holds its invariants on any text, stays within its limits, and
//! gives ids that survive edits elsewhere in the file.

use std::collections::BTreeSet;
use std::time::{Duration, Instant};

use codeatlas_application::{LanguageParser, ParseInput, ParseLimits};
use codeatlas_core::{Language, ParsedSource, Position, RepoPath, SymbolLimit};
use codeatlas_parser_treesitter::TreeSitterParser;
use codeatlas_parser_treesitter::symbols::MAX_NAMESPACE_DEPTH;

const LANGUAGES: [(Language, &str); 4] = [
    (Language::TypeScript, "case.ts"),
    (Language::Tsx, "case.tsx"),
    (Language::JavaScript, "case.js"),
    (Language::Jsx, "case.jsx"),
];

fn parse_with(language: Language, name: &str, text: &str, limits: ParseLimits) -> ParsedSource {
    let path = RepoPath::new(name).unwrap();
    TreeSitterParser
        .parse(
            &ParseInput {
                path: &path,
                language,
                text,
            },
            &limits,
        )
        .unwrap_or_else(|e| panic!("{name}: {e}"))
}

fn parse(name: &str, text: &str) -> ParsedSource {
    let language = Language::detect(&RepoPath::new(name).unwrap()).unwrap();
    let limits = ParseLimits {
        timeout: None,
        ..ParseLimits::default()
    };
    parse_with(language, name, text, limits)
}

/// The invariants every result must keep, whatever the input.
fn assert_invariants(parsed: &ParsedSource, label: &str) {
    let ids: BTreeSet<&str> = parsed.symbols().iter().map(|s| s.id().as_str()).collect();
    assert_eq!(ids.len(), parsed.symbols().len(), "{label}: ids are unique");
    for s in parsed.symbols() {
        assert!(
            s.id().as_str().chars().count() <= 512,
            "{label}: {}",
            s.id()
        );
        assert!(!s.declarations().is_empty(), "{label}: {}", s.id());
        assert_eq!(s.primary().kind, s.kind(), "{label}: {}", s.id());
        assert_eq!(s.qualified_name().rsplit('.').next(), Some(s.name()));
        for d in s.declarations() {
            assert!(
                d.range.start() <= d.name_range.start() && d.name_range.end() <= d.range.end(),
                "{label}: {} name {:?} outside {:?}",
                s.id(),
                d.name_range,
                d.range
            );
        }
        match s.parent() {
            Some(parent) => {
                assert!(
                    ids.contains(parent.as_str()),
                    "{label}: {} has no parent",
                    s.id()
                );
                assert!(!s.exported(), "{label}: member {} is exported", s.id());
            }
            None => assert!(!s.qualified_name().contains('.') || s.name().contains('.')),
        }
    }
    let starts: Vec<Position> = parsed
        .symbols()
        .iter()
        .map(|s| s.declarations()[0].range.start())
        .collect();
    assert!(starts.is_sorted(), "{label}: symbols in document order");
}

#[test]
fn invariants_hold_on_any_text_in_every_language() {
    let inputs = [
        String::new(),
        "\u{feff}".to_string(),
        "class { } } interface I { a b c } namespace .. { }".to_string(),
        "export default; export = ; export { , } ; export * as from".to_string(),
        "const { a: { b: [c, ...{ d }] = 1 } = {}, ...e } = f;".to_string(),
        "class A { constructor(public a, private, @x readonly) {} get; set; static; #; }"
            .to_string(),
        "namespace A.B.C. { export namespace .D { } }".to_string(),
        "module.exports = { a, b: c, ...d, [e]: f }; exports. = g; module.exports.h = ;"
            .to_string(),
        "function f(): void; function f(); function f() {} function f".to_string(),
        "interface X { (): void; new (): X; [k: string]: X; get a(): number; set a(v) }"
            .to_string(),
        "😀 class 😀 { 😀() {} } const 😀 = 1;".to_string(),
        (0u32..3000).filter_map(char::from_u32).collect(),
    ];
    for (language, name) in LANGUAGES {
        for text in &inputs {
            let limits = ParseLimits {
                timeout: None,
                ..ParseLimits::default()
            };
            let parsed = parse_with(language, name, text, limits);
            assert_invariants(&parsed, &format!("{name} {text:.40?}"));
        }
    }
}

#[test]
fn the_fixture_style_code_keeps_its_invariants() {
    let text = "export class A { m() {} p = 1; get g() { return 1 } set g(v) {} }\n\
                export interface A { q: string }\n\
                export namespace N.M { export function f() {} class C { x = 1 } }\n\
                export const { a, b: [c] } = o; var a;\n";
    assert_invariants(&parse("case.ts", text), "mixed");
}

#[test]
fn deep_namespaces_stop_at_the_limit_and_say_where() {
    let depth = 100_000;
    let nested = format!("{}{}", "namespace N { ".repeat(depth), "} ".repeat(depth));
    let dotted = format!("namespace {} {{}}", vec!["N"; depth].join("."));
    for (label, text) in [("nested", nested), ("dotted", dotted)] {
        let started = Instant::now();
        let parsed = parse("case.ts", &text);
        assert!(started.elapsed() < Duration::from_secs(20), "{label}");
        assert_eq!(parsed.symbols().len(), MAX_NAMESPACE_DEPTH, "{label}");
        let deepest = parsed.symbols().last().unwrap();
        assert_eq!(
            deepest.qualified_name(),
            vec!["N"; MAX_NAMESPACE_DEPTH].join("."),
            "{label}"
        );
        let truncated = parsed.symbols_truncated().expect(label);
        assert_eq!(
            truncated.limit,
            SymbolLimit::NamespaceDepth {
                limit: MAX_NAMESPACE_DEPTH
            }
        );
        // At the first namespace not extracted, the 33rd.
        let expected_at = if label == "nested" {
            Position::new(
                0,
                (MAX_NAMESPACE_DEPTH * "namespace N { ".len() + 10) as u32,
            )
        } else {
            Position::new(0, (10 + MAX_NAMESPACE_DEPTH * 2) as u32)
        };
        assert_eq!(truncated.at, expected_at, "{label}");
        assert_eq!(parsed.diagnostics()[0].code.as_str(), "CA1003");
        assert_invariants(&parsed, label);
    }
}

#[test]
fn deep_patterns_do_not_overflow_the_stack() {
    let depth = 50_000;
    let text = format!("const {}x{} = y;", "[".repeat(depth), "]".repeat(depth));
    for name in ["case.ts", "case.js"] {
        let parsed = parse(name, &text);
        assert_eq!(parsed.symbols().len(), 1, "{name}");
        assert_eq!(parsed.symbols()[0].name(), "x");
    }
}

#[test]
fn the_symbol_limit_keeps_the_first_declarations_in_document_order() {
    let text: String = (0..50)
        .map(|i| format!("export function f{i}() {{}}\n"))
        .collect();
    let limits = ParseLimits {
        max_symbols: 10,
        timeout: None,
        ..ParseLimits::default()
    };
    let parsed = parse_with(Language::TypeScript, "case.ts", &text, limits);
    let names: Vec<&str> = parsed.symbols().iter().map(|s| s.name()).collect();
    assert_eq!(names, (0..10).map(|i| format!("f{i}")).collect::<Vec<_>>());
    let truncated = parsed.symbols_truncated().unwrap();
    assert_eq!(truncated.limit, SymbolLimit::Count { limit: 10 });
    assert_eq!(truncated.at, Position::new(10, 0));

    let none = ParseLimits {
        max_symbols: 0,
        timeout: None,
        ..ParseLimits::default()
    };
    let parsed = parse_with(Language::TypeScript, "case.ts", &text, none);
    assert!(parsed.symbols().is_empty());
    assert_eq!(parsed.symbols_truncated().unwrap().at, Position::new(0, 0));

    let parsed = parse("case.ts", &text);
    assert_eq!(parsed.symbols().len(), 50);
    assert!(parsed.symbols_truncated().is_none());
}

#[test]
fn a_minified_file_is_extracted_quickly() {
    // About 2 MB on one line: the scanner's default size limit.
    let text: String = (0..60_000)
        .map(|i| format!("var v{i}=function(){{return {i}}};"))
        .collect();
    let started = Instant::now();
    let parsed = parse("bundle.js", &text);
    assert_eq!(parsed.symbols().len(), 60_000);
    assert!(started.elapsed() < Duration::from_secs(30));
    let last = parsed.symbols().last().unwrap();
    let column = text.rfind("var v59999").unwrap() + 4;
    assert_eq!(
        last.primary().name_range.start(),
        Position::new(0, column as u32)
    );
}

/// Ids by qualified name, for the stability checks.
fn ids(name: &str, text: &str) -> Vec<(String, String)> {
    let mut out: Vec<(String, String)> = parse(name, text)
        .symbols()
        .iter()
        .map(|s| (s.qualified_name().to_string(), s.id().to_string()))
        .collect();
    out.sort();
    out
}

#[test]
fn ids_survive_edits_elsewhere_in_the_file() {
    let original = "import { x } from \"./x\";\n\n\
                    export class Room {\n    open(): void {}\n    size = 1;\n}\n\n\
                    export function join(a: number): number {\n    return a;\n}\n";
    let before = ids("src/room.ts", original);
    assert_eq!(before.len(), 4);
    let edits = [
        // Lines and declarations added above and between.
        format!("// header\n\nconst extra = 1;\n{original}"),
        original.replace(
            "export function join",
            "function helper() {}\n\nexport function join",
        ),
        // Bodies, comments, whitespace and line endings changed.
        original.replace("return a;", "const b = a * 2;\n    return b;"),
        original.replace(
            "open(): void {}",
            "/** Opens. */\n    open(): void {\n        x();\n    }",
        ),
        original.replace("    ", "\t"),
        original.replace('\n', "\r\n"),
        format!("\u{feff}{original}"),
        // Members reordered.
        original.replace(
            "    open(): void {}\n    size = 1;\n",
            "    size = 1;\n    open(): void {}\n",
        ),
    ];
    for (i, edited) in edits.iter().enumerate() {
        let after: Vec<(String, String)> = ids("src/room.ts", edited)
            .into_iter()
            .filter(|(q, _)| !["extra", "helper"].contains(&q.as_str()))
            .collect();
        assert_eq!(after, before, "edit {i}");
    }
}

#[test]
fn ids_change_with_the_name_and_the_file() {
    let text = "export class Room {\n    open(): void {}\n}\n";
    let renamed = ids("src/room.ts", &text.replace("open", "start"));
    assert!(
        renamed
            .iter()
            .any(|(_, id)| id == "symbol:src/room.ts#Room.start")
    );
    assert!(!renamed.iter().any(|(_, id)| id.ends_with("#Room.open")));
    let moved = ids("src/rooms/room.ts", text);
    assert_eq!(
        moved,
        [
            (
                "Room".to_string(),
                "symbol:src/rooms/room.ts#Room".to_string()
            ),
            (
                "Room.open".to_string(),
                "symbol:src/rooms/room.ts#Room.open".to_string()
            ),
        ]
    );
}
