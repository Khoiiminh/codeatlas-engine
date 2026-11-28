//! ENG-04 with the real analyzer (`packages/typescript-analyzer`) on small repositories built
//! in a temporary folder: positions, deep import chains, configuration problems and unreadable
//! files. What the facts mean, with symbol ids, is tested end to end in `tests/fixtures`.

mod support;

use std::path::Path;

use codeatlas_application::{RunContext, SemanticFailureReason, SemanticOutput, SemanticResolver};
use codeatlas_core::{
    DeclarationSite, ModuleTarget, Position, Range, ReferenceKind, ReferenceTarget, RepoPath,
};
use codeatlas_semantic_typescript::{AnalyzerSettings, TypeScriptResolver};
use support::{analyzer, sources, write};

fn run(resolver: &TypeScriptResolver, root: &Path, files: &[&str]) -> SemanticOutput {
    resolver
        .resolve(root, &sources(files), &RunContext::detached())
        .unwrap()
}

fn range(sl: u32, sc: u32, el: u32, ec: u32) -> Range {
    Range::new(Position::new(sl, sc), Position::new(el, ec)).unwrap()
}

fn file<'a>(output: &'a SemanticOutput, path: &str) -> &'a codeatlas_application::ResolvedFile {
    output
        .files
        .iter()
        .find(|f| f.path.as_str() == path)
        .unwrap_or_else(|| panic!("no result for {path}: {:?}", output.failed))
}

#[test]
fn positions_use_utf16_characters_and_the_engines_line_breaks() {
    let Some(resolver) = analyzer() else { return };
    let dir = tempfile::tempdir().unwrap();
    // A byte order mark (not counted), CRLF, characters outside the BMP (two UTF-16 units each),
    // U+2028 inside a string (not a line break for the engine, unlike TypeScript), and a lone CR.
    write(
        dir.path(),
        "a.ts",
        "\u{feff}import { f } from \"./b\";\r\nconst s = \"😀😀\"; f();\r\nconst t = \"x\u{2028}y\"; f();\nf();\rf();\n",
    );
    write(dir.path(), "b.ts", "export function f() {}\n");
    let output = run(&resolver, dir.path(), &["a.ts", "b.ts"]);
    assert!(output.failed.is_empty(), "{:?}", output.failed);
    let a = file(&output, "a.ts");
    assert_eq!(a.facts.modules.len(), 1);
    assert_eq!(a.facts.modules[0].range, range(0, 18, 0, 23));
    assert_eq!(
        a.facts.modules[0].target,
        ModuleTarget::File(RepoPath::new("b.ts").unwrap())
    );
    let calls: Vec<Range> = a
        .facts
        .references
        .iter()
        .inspect(|r| {
            assert_eq!(r.kind, ReferenceKind::Call);
            assert_eq!(
                r.target,
                ReferenceTarget::Declarations(vec![DeclarationSite {
                    path: RepoPath::new("b.ts").unwrap(),
                    name_start: Position::new(0, 16),
                }])
            );
        })
        .map(|r| r.range)
        .collect();
    assert_eq!(
        calls,
        [
            range(1, 18, 1, 19),
            range(2, 17, 2, 18),
            range(3, 0, 3, 1),
            range(4, 0, 4, 1)
        ]
    );
}

/// `c0000.ts` imports `c0001.ts`, which imports `c0002.ts`, and so on: the type of each `box`
/// needs the type of the next one, so the checker recurses once per file.
fn chain(root: &Path, length: usize) -> Vec<String> {
    let mut names = Vec::new();
    for i in 0..length {
        let name = format!("c{i:04}.ts");
        let text = if i + 1 == length {
            "export const box = { depth: 0 };\n".to_string()
        } else {
            format!(
                "import {{ box as next }} from \"./c{:04}\";\nexport const box = {{ next, depth: next.depth + 1 }};\nexport function read(): number {{\n    return box.next.depth;\n}}\n",
                i + 1
            )
        };
        write(root, &name, &text);
        names.push(name);
    }
    names
}

#[test]
fn a_3000_file_import_chain_does_not_overflow_the_stack() {
    let Some(resolver) = analyzer() else { return };
    let dir = tempfile::tempdir().unwrap();
    let names = chain(dir.path(), 3000);
    let files: Vec<&str> = names.iter().map(String::as_str).collect();
    let output = run(&resolver, dir.path(), &files);
    assert!(
        output.failed.is_empty(),
        "{} failed, first: {:?}",
        output.failed.len(),
        output.failed.first()
    );
    assert_eq!(output.files.len(), 3000);
    // c0000.ts imports c0001.ts.
    assert_eq!(
        file(&output, "c0000.ts").facts.modules[0].target,
        ModuleTarget::File(RepoPath::new("c0001.ts").unwrap())
    );

    // The stack is what makes it work: on the default thread stack, the checker overflows.
    let small = resolver.with_settings(AnalyzerSettings {
        stack_size_mb: Some(1),
        ..AnalyzerSettings::default()
    });
    let output = run(&small, dir.path(), &files);
    assert!(!output.failed.is_empty(), "expected a stack overflow");
    assert!(
        output.failed.iter().all(|f| matches!(
            &f.reason,
            SemanticFailureReason::AnalyzerFailed(m) if m.contains("RangeError")
        )),
        "{:?}",
        output.failed.first()
    );
}

#[test]
fn configuration_problems_are_reported_and_the_files_still_analysed() {
    let Some(resolver) = analyzer() else { return };
    let dir = tempfile::tempdir().unwrap();
    write(
        dir.path(),
        "tsconfig.json",
        "{\n  \"compilerOptions\": {\n    \"strictt\": true\n  }\n}\n",
    );
    write(dir.path(), "a.ts", "import { b } from \"./b\";\nb();\n");
    write(dir.path(), "b.ts", "export function b() {}\n");
    let output = run(&resolver, dir.path(), &["a.ts", "b.ts"]);
    assert!(output.failed.is_empty(), "{:?}", output.failed);
    assert_eq!(output.files.len(), 2);
    assert_eq!(output.problems.len(), 1, "{:?}", output.problems);
    let problem = &output.problems[0];
    assert!(problem.message.contains("strictt"), "{}", problem.message);
    let location = problem.location.as_ref().unwrap();
    assert_eq!(location.path.as_str(), "tsconfig.json");
    assert_eq!(location.range.start().line, 2);
}

#[test]
fn a_file_that_cannot_be_read_fails_alone() {
    let Some(resolver) = analyzer() else { return };
    let dir = tempfile::tempdir().unwrap();
    write(dir.path(), "a.ts", "export const a = 1;\n");
    let output = run(&resolver, dir.path(), &["a.ts", "gone.ts"]);
    assert_eq!(output.files.len(), 1);
    assert_eq!(output.failed.len(), 1);
    assert_eq!(output.failed[0].path.as_str(), "gone.ts");
    assert!(
        matches!(
            output.failed[0].reason,
            SemanticFailureReason::Unreadable(_)
        ),
        "{:?}",
        output.failed[0]
    );
    assert!(output.peak_memory_bytes.is_some_and(|b| b > 1_000_000));
}
