// ENG-02: cross-checks the Tree-sitter parser cases with the pinned TypeScript compiler.
//
//   node scripts/check-parser-cases.ts
//
// crates/adapters/codeatlas-parser-treesitter/tests/cases holds small source files, each with a
// `<file>.expected.json` listing the syntax problems Tree-sitter reports. The Rust test checks
// those expectations exactly. This script checks something the Rust test cannot: that the
// TypeScript compiler agrees on which files have syntax errors at all. A case Tree-sitter calls
// clean must have no syntactic diagnostics, and a case with problems must have at least one.
// The two parsers place recovery points differently, so positions are not compared.
// Exits non-zero on any disagreement.

import { readFileSync, readdirSync } from "node:fs";
import { extname, join, resolve } from "node:path";
import ts from "typescript";

const casesDir = resolve(import.meta.dirname, "../../../crates/adapters/codeatlas-parser-treesitter/tests/cases");
const expectedSuffix = ".expected.json";

const options: ts.CompilerOptions = {
    allowJs: true,
    noEmit: true,
    target: ts.ScriptTarget.ESNext,
    module: ts.ModuleKind.ESNext,
    jsx: ts.JsxEmit.Preserve,
    experimentalDecorators: true,
    types: [],
};

const sourceExtensions = new Set([".ts", ".mts", ".cts", ".tsx", ".js", ".mjs", ".cjs", ".jsx"]);

interface Expected {
    problems: unknown[];
    truncated: boolean;
}

const names = readdirSync(casesDir)
    .filter((name) => sourceExtensions.has(extname(name)))
    .sort();
if (names.length === 0) {
    console.error(`no parser cases in ${casesDir}`);
    process.exit(1);
}

let disagreements = 0;
for (const name of names) {
    const file = join(casesDir, name);
    const expected = JSON.parse(readFileSync(file + expectedSuffix, "utf8")) as Expected;
    const treeSitterBroken = expected.problems.length > 0 || expected.truncated;

    const program = ts.createProgram([file], options);
    const sourceFile = program.getSourceFile(file);
    if (sourceFile === undefined) {
        console.error(`${name}: the compiler did not load the file`);
        disagreements++;
        continue;
    }
    const diagnostics = program.getSyntacticDiagnostics(sourceFile);
    const compilerBroken = diagnostics.length > 0;

    const first = diagnostics[0];
    let detail = "";
    if (first !== undefined && first.start !== undefined) {
        const at = sourceFile.getLineAndCharacterOfPosition(first.start);
        const message = ts.flattenDiagnosticMessageText(first.messageText, " ");
        detail = ` (first: TS${first.code} at ${at.line}:${at.character}, ${message})`;
    }
    const agree = compilerBroken === treeSitterBroken;
    if (!agree) {
        disagreements++;
    }
    const verdict = (broken: boolean) => (broken ? "syntax errors" : "clean");
    console.log(
        `${agree ? "ok  " : "FAIL"} ${name}: compiler ${verdict(compilerBroken)}, Tree-sitter ${verdict(treeSitterBroken)}${detail}`,
    );
}

if (disagreements > 0) {
    console.error(`${disagreements} of ${names.length} parser cases disagree with the TypeScript compiler`);
    process.exit(1);
}
console.log(`all ${names.length} parser cases agree with the TypeScript compiler`);
