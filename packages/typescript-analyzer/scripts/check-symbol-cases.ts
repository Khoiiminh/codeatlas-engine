// ENG-03: cross-checks the Tree-sitter symbol cases with the pinned TypeScript compiler.
//
//   node scripts/check-symbol-cases.ts
//
// crates/adapters/codeatlas-parser-treesitter/tests/symbols holds small source files, each with
// a `<file>.symbols.json` listing the symbols the Rust extractor reports: ids, kinds, parents,
// export flags and every declaration's range and name range. This script computes the same
// list with the TypeScript compiler (scripts/symbol-rules.ts) and requires it to be identical.
//
// Cases named `broken-…` have syntax errors on purpose. The two parsers recover from errors
// differently, so for those the script only checks that the compiler also reports a syntax
// error; every other case must have none. Exits non-zero on any disagreement.

import { readFileSync, readdirSync } from "node:fs";
import { extname, join, resolve } from "node:path";
import { isDeepStrictEqual } from "node:util";
import ts from "typescript";
import { symbolsOf } from "./symbol-rules.ts";

const casesDir = resolve(import.meta.dirname, "../../../crates/adapters/codeatlas-parser-treesitter/tests/symbols");
const expectedSuffix = ".symbols.json";
const sourceExtensions = new Set([".ts", ".mts", ".cts", ".tsx", ".js", ".mjs", ".cjs", ".jsx"]);

const options: ts.CompilerOptions = {
    allowJs: true,
    noEmit: true,
    noLib: true,
    types: [],
    target: ts.ScriptTarget.ESNext,
    module: ts.ModuleKind.ESNext,
    jsx: ts.JsxEmit.Preserve,
    experimentalDecorators: true,
};

const names = readdirSync(casesDir)
    .filter((name) => sourceExtensions.has(extname(name)))
    .sort();
if (names.length === 0) {
    console.error(`no symbol cases in ${casesDir}`);
    process.exit(1);
}

let disagreements = 0;
for (const name of names) {
    const file = join(casesDir, name);
    // One program per case, so script files cannot see each other's globals.
    const program = ts.createProgram([file], options);
    const sf = program.getSourceFile(file);
    if (sf === undefined) {
        console.error(`FAIL ${name}: the compiler did not load the file`);
        disagreements++;
        continue;
    }
    const syntaxErrors = program.getSyntacticDiagnostics(sf).length;
    const broken = name.startsWith("broken-");
    if (broken !== syntaxErrors > 0) {
        console.error(`FAIL ${name}: the compiler reports ${syntaxErrors} syntax errors`);
        disagreements++;
        continue;
    }
    if (broken) {
        console.log(`ok   ${name}: syntax errors in both; symbols not compared`);
        continue;
    }

    const at = (pos: number) => sf.getLineAndCharacterOfPosition(pos);
    const range = (node: ts.Node) => {
        const [s, e] = [at(node.getStart(sf)), at(node.getEnd())];
        return `${s.line}:${s.character}-${e.line}:${e.character}`;
    };
    const symbols = symbolsOf(name, sf, program.getTypeChecker()).map((s) => ({
        id: s.id,
        name: s.name,
        kind: s.kind,
        exported: s.exported,
        declarations: s.declarations.map((d) => ({ kind: d.kind, range: range(d.node), name: range(d.nameNode) })),
        ...(s.parent ? { parent: s.parent } : {}),
    }));
    const expected = JSON.parse(readFileSync(file + expectedSuffix, "utf8")) as { symbols: unknown[] };
    if (isDeepStrictEqual(symbols, expected.symbols)) {
        console.log(`ok   ${name}: ${symbols.length} symbols agree`);
        continue;
    }
    disagreements++;
    console.error(`FAIL ${name}: the compiler finds different symbols`);
    const show = (list: unknown[]) => list.map((s) => JSON.stringify(s));
    const want = new Set(show(expected.symbols));
    const got = new Set(show(symbols));
    for (const s of got) if (!want.has(s)) console.error(`  compiler only: ${s}`);
    for (const s of want) if (!got.has(s)) console.error(`  Tree-sitter only: ${s}`);
}

if (disagreements > 0) {
    console.error(`${disagreements} of ${names.length} symbol cases disagree with the TypeScript compiler`);
    process.exit(1);
}
console.log(`all ${names.length} symbol cases agree with the TypeScript compiler`);
