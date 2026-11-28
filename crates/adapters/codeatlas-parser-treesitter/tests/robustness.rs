//! ENG-02: the parser returns a result for any text and stays within its limits.

use std::time::Duration;

use codeatlas_application::{LanguageParser, ParseError, ParseInput, ParseLimits};
use codeatlas_core::{Language, ParsedSource, Position, RepoPath, SyntaxProblemKind};
use codeatlas_parser_treesitter::TreeSitterParser;

const LANGUAGES: [(Language, &str); 4] = [
    (Language::TypeScript, "case.ts"),
    (Language::Tsx, "case.tsx"),
    (Language::JavaScript, "case.js"),
    (Language::Jsx, "case.jsx"),
];

fn parse(
    language: Language,
    name: &str,
    text: &str,
    limits: ParseLimits,
) -> Result<ParsedSource, ParseError> {
    let path = RepoPath::new(name).unwrap();
    TreeSitterParser.parse(
        &ParseInput {
            path: &path,
            language,
            text,
        },
        &limits,
    )
}

fn no_timeout() -> ParseLimits {
    ParseLimits {
        timeout: None,
        ..ParseLimits::default()
    }
}

#[test]
fn any_text_gives_a_result_in_every_language() {
    let inputs = [
        "".to_string(),
        "\u{feff}".to_string(),
        "}}}{{{".to_string(),
        "\0\0\0".to_string(),
        "<<<<>>>>".repeat(200),
        "\"unterminated".to_string(),
        "/* unterminated comment".to_string(),
        "`${`${`${".to_string(),
        "\r\r\n\n\u{2028}\u{2029}".to_string(),
        "😀".repeat(500),
        (0u32..2000).filter_map(char::from_u32).collect(),
    ];
    for (language, name) in LANGUAGES {
        for text in &inputs {
            let parsed = parse(language, name, text, no_timeout())
                .unwrap_or_else(|e| panic!("{name} {text:?}: {e}"));
            assert_eq!(parsed.language(), language);
            for problem in parsed.problems() {
                let SyntaxProblemKind::Missing { expected } = &problem.kind else {
                    continue;
                };
                assert!(!expected.is_empty(), "{name}: missing node without a kind");
                assert!(
                    problem.range.is_empty(),
                    "{name}: missing node with a width"
                );
            }
        }
    }
}

#[test]
fn deep_nesting_does_not_overflow_the_stack() {
    let depth = 100_000;
    let text = format!("const x = {}1{};", "(".repeat(depth), ")".repeat(depth));
    for (language, name) in LANGUAGES {
        let parsed = parse(language, name, &text, no_timeout()).unwrap();
        assert!(parsed.is_clean(), "{name}: {:?}", parsed.problems().first());
    }
    let unbalanced = "[".repeat(depth);
    let parsed = parse(Language::TypeScript, "case.ts", &unbalanced, no_timeout()).unwrap();
    assert!(!parsed.is_clean());
}

#[test]
fn problems_stop_at_the_limit_and_say_so() {
    let text = "let = ;\n".repeat(50).replace("let", "const");
    let limits = ParseLimits {
        max_problems: 3,
        timeout: None,
        ..ParseLimits::default()
    };
    let parsed = parse(Language::TypeScript, "case.ts", &text, limits).unwrap();
    assert_eq!(parsed.problems().len(), 3);
    assert!(parsed.problems_truncated());
    // The first three in document order.
    let lines: Vec<u32> = parsed
        .problems()
        .iter()
        .map(|p| p.range.start().line)
        .collect();
    assert_eq!(lines, [0, 1, 2]);

    let all = parse(Language::TypeScript, "case.ts", &text, no_timeout()).unwrap();
    assert_eq!(all.problems().len(), 50);
    assert!(!all.problems_truncated());
}

#[test]
fn a_zero_timeout_on_a_large_file_times_out() {
    let text = "export const value = [1, 2, 3].map((n) => n * 2);\n".repeat(20_000);
    let limits = ParseLimits {
        max_problems: 10,
        timeout: Some(Duration::ZERO),
        ..ParseLimits::default()
    };
    assert_eq!(
        parse(Language::TypeScript, "case.ts", &text, limits),
        Err(ParseError::TimedOut {
            after: Duration::ZERO
        })
    );
    // The default limit is far above what this file needs.
    let parsed = parse(
        Language::TypeScript,
        "case.ts",
        &text,
        ParseLimits::default(),
    )
    .unwrap();
    assert!(parsed.is_clean());
}

#[test]
fn parsing_is_deterministic() {
    let text = "export function f(a: number { return a +; }\nconst = 1;\n<div>";
    for (language, name) in LANGUAGES {
        let first = parse(language, name, text, no_timeout()).unwrap();
        for _ in 0..5 {
            assert_eq!(parse(language, name, text, no_timeout()).unwrap(), first);
        }
    }
}

#[test]
fn positions_follow_line_breaks_and_skip_the_byte_order_mark() {
    for (prefix, line_break) in [("", "\n"), ("\u{feff}", "\r\n"), ("", "\r")] {
        let text = format!("{prefix}const a = 1;{line_break}const = 2;{line_break}");
        let parsed = parse(Language::TypeScript, "case.ts", &text, no_timeout()).unwrap();
        let start = parsed.problems()[0].range.start();
        assert_eq!(start, Position::new(1, 0), "{prefix:?} {line_break:?}");
    }
}
