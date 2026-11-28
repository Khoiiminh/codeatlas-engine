// FND-04: cross-checks tests/fixtures/<name>/expected.json with the pinned TypeScript compiler.
//
//   node scripts/check-fixtures.ts
//
// The Rust tests in tests/fixtures check that every expectation points at real source text.
// This script checks what only a compiler can: the file inventory, the exact diagnostics, where
// every import resolves, which declaration every call, reference and heritage edge reaches,
// and that the symbol list is exactly right (scripts/symbol-rules.ts, ENG-03). Every folder under tests/fixtures except src, tests and
// target is a fixture and must have expected.json. Exits non-zero if any fixture disagrees.
//
// TypeScript projects: expected.json may list "typescriptProjects", tsconfig paths relative to
// the fixture folder (for a monorepo, one per package; for a plain JavaScript project, a
// checker-only tsconfig placed next to expected.json, outside the analysed repository). The
// default is <root>/tsconfig.json. A file belongs to the first project that includes it.
// Programs are built as the analyzer builds them (ADR-0015): a jsconfig.json starts from the
// defaults tsserver gives it, and project references are not passed to the compiler, so imports
// resolve to the referenced project's source rather than to build output.

import { existsSync, readFileSync, readdirSync } from "node:fs";
import { dirname, join, relative, resolve, sep } from "node:path";
import ts from "typescript";
import { primary, symbolsOf } from "./symbol-rules.ts";

/** What tsserver assumes for a jsconfig.json before reading it (as src/analysis/projects.ts). */
const JSCONFIG_DEFAULTS: ts.CompilerOptions = {
    allowJs: true,
    maxNodeModuleJsDepth: 2,
    allowSyntheticDefaultImports: true,
    skipLibCheck: true,
    noEmit: true,
};

interface Expected {
    root: string;
    typescriptProjects?: string[];
    files: { path: string; language: string }[];
    externalModules: string[];
    symbols: ExpectedSymbol[];
    edges: {
        kind: string;
        from: string;
        to: string;
        path: string;
        line: number;
        text: string;
        specifier?: string;
    }[];
    unresolved: {
        kind: string;
        from: string;
        path: string;
        line: number;
        text: string;
        specifier: string;
        reason: string;
    }[];
    typescriptDiagnostics: { path: string; line: number; code: number }[];
}

interface ExpectedSymbol {
    id: string;
    kind: string;
    name: string;
    line: number;
    parent?: string;
    exported: boolean;
}

function symbolKey(s: ExpectedSymbol): string {
    return `${s.id} ${s.kind} name=${s.name} @${s.line} parent=${s.parent ?? "-"} ${s.exported ? "exported" : "local"}`;
}

interface Decl {
    id: string;
    kind: string;
    node: ts.Node;
    path: string;
    line: number;
}

const engineRoot = resolve(import.meta.dirname, "../../..");
const fixturesDir = join(engineRoot, "tests", "fixtures");
const SOURCE = /\.(ts|mts|cts|tsx|js|jsx|mjs|cjs)$/;
const DECLARATION = /\.d\.(m|c)?ts$/;
const NOT_FIXTURES = new Set(["src", "tests", "target"]);

interface Owner {
    sf: ts.SourceFile;
    checker: ts.TypeChecker;
    options: ts.CompilerOptions;
}

let problems = 0;
function fail(message: string): void {
    problems++;
    console.error(`  ✗ ${message}`);
}

function sameSet(label: string, actual: string[], expected: string[]): void {
    const a = new Set(actual);
    const e = new Set(expected);
    for (const x of [...a].filter((x) => !e.has(x)).sort()) fail(`${label}: unexpected ${x}`);
    for (const x of [...e].filter((x) => !a.has(x)).sort()) fail(`${label}: missing ${x}`);
}

function checkFixture(name: string): void {
    const fixtureDir = join(fixturesDir, name);
    const expected = JSON.parse(readFileSync(join(fixtureDir, "expected.json"), "utf8")) as Expected;
    const root = join(fixtureDir, expected.root);
    const rel = (file: string) => relative(root, file).split(sep).join("/");

    // One program per TypeScript project; each file is owned by the first project that has it.
    const owners = new Map<string, Owner>();
    const diagnosticKeys: string[] = [];
    const lineOf = (sf: ts.SourceFile, pos: number) => sf.getLineAndCharacterOfPosition(pos).line + 1;
    for (const project of expected.typescriptProjects ?? [join(expected.root, "tsconfig.json")]) {
        const configPath = join(fixtureDir, project);
        if (!existsSync(configPath)) {
            fail(`TypeScript project ${project} not found (add a tsconfig or list "typescriptProjects")`);
            continue;
        }
        const config = ts.readConfigFile(configPath, (p) => ts.sys.readFile(p));
        const defaults = configPath.endsWith("jsconfig.json") ? JSCONFIG_DEFAULTS : {};
        const parsed = ts.parseJsonConfigFileContent(config.config, ts.sys, dirname(configPath), defaults, configPath);
        for (const d of [config.error, ...parsed.errors].filter((d) => d !== undefined)) {
            fail(`${project}: ${ts.flattenDiagnosticMessageText(d.messageText, " ")}`);
        }
        const program = ts.createProgram({ rootNames: parsed.fileNames, options: parsed.options });
        const checker = program.getTypeChecker();
        const own = program
            .getSourceFiles()
            .filter((f) => !program.isSourceFileDefaultLibrary(f) && !f.fileName.includes("/node_modules/"))
            .filter((f) => SOURCE.test(f.fileName) && !DECLARATION.test(f.fileName))
            .filter((f) => !rel(f.fileName).startsWith(".."));
        for (const sf of own) {
            if (!owners.has(rel(sf.fileName))) owners.set(rel(sf.fileName), { sf, checker, options: parsed.options });
        }
        for (const d of ts.getPreEmitDiagnostics(program)) {
            if (d.file && own.includes(d.file)) {
                diagnosticKeys.push(`${rel(d.file.fileName)}:${lineOf(d.file, d.start ?? 0)} TS${d.code}`);
            }
        }
    }
    const byPath = new Map([...owners].map(([path, o]) => [path, o.sf]));
    const owner = (path: string): Owner => owners.get(path)!;

    // 1. Inventory: the compiler sees exactly the listed files.
    sameSet(
        "files",
        [...byPath.keys()],
        expected.files.map((f) => f.path),
    );

    // 2. Diagnostics: exactly the listed ones.
    sameSet(
        "diagnostics",
        [...new Set(diagnosticKeys)],
        expected.typescriptDiagnostics.map((d) => `${d.path}:${d.line} TS${d.code}`),
    );

    // 3. Symbols, by the rules in symbol-rules.ts (shared with check-symbol-cases.ts): exactly
    //    the expected list, with the same ids, kinds, names, name lines, parents and export flags.
    const decls: Decl[] = [];
    const symbolKeys: string[] = [];
    for (const [path, sf] of byPath) {
        for (const s of symbolsOf(path, sf, owner(path).checker)) {
            for (const d of s.declarations) {
                decls.push({ id: s.id, kind: s.kind, node: d.node, path, line: lineOf(sf, d.nameNode.getStart(sf)) });
            }
            const line = lineOf(sf, primary(s).nameNode.getStart(sf));
            symbolKeys.push(symbolKey({ ...s, line }));
        }
    }
    const declById = new Map(decls.map((d) => [d.id, d]));
    sameSet("symbols", symbolKeys, expected.symbols.map(symbolKey));

    // Innermost listed symbol whose declaration contains a position, else the file.
    const enclosing = (path: string, pos: number): string => {
        let best: Decl | undefined;
        for (const d of decls) {
            if (d.path !== path || pos < d.node.getFullStart() || pos >= d.node.getEnd()) continue;
            if (!best || d.node.getEnd() - d.node.getFullStart() < best.node.getEnd() - best.node.getFullStart())
                best = d;
        }
        return best ? best.id : `file:${path}`;
    };

    // Declaration a name resolves to, following import aliases and CommonJS shorthand exports.
    const target = (node: ts.Node, path: string): string | undefined => {
        const { checker } = owner(path);
        let symbol = checker.getSymbolAtLocation(node);
        for (let i = 0; symbol && i < 10; i++) {
            if (symbol.flags & ts.SymbolFlags.Alias) {
                symbol = checker.getAliasedSymbol(symbol);
                continue;
            }
            const decl = symbol.declarations?.[0];
            if (decl && ts.isShorthandPropertyAssignment(decl)) {
                symbol = checker.getShorthandAssignmentValueSymbol(decl);
                continue;
            }
            break;
        }
        for (const decl of symbol?.declarations ?? []) {
            const sf = decl.getSourceFile();
            const nameNode = ts.getNameOfDeclaration(decl as ts.Declaration) ?? decl;
            const hit = decls.find((d) => d.path === rel(sf.fileName) && d.line === lineOf(sf, nameNode.getStart(sf)));
            if (hit) return hit.id;
        }
        return undefined;
    };

    const identifierAt = (sf: ts.SourceFile, pos: number): ts.Node | undefined => {
        const visit = (n: ts.Node): ts.Node | undefined =>
            n.getStart(sf) <= pos && pos < n.getEnd()
                ? ts.isIdentifier(n) || n.kind === ts.SyntaxKind.SuperKeyword
                    ? n
                    : ts.forEachChild(n, visit)
                : undefined;
        return visit(sf);
    };

    const positionOf = (path: string, line: number, text: string, needle: string): number | undefined => {
        const sf = byPath.get(path)!;
        const lineStart = sf.getPositionOfLineAndCharacter(line - 1, 0);
        const lineText = sf.text.slice(lineStart).split("\n")[0]!;
        const at = lineText.indexOf(text);
        if (at < 0) return undefined;
        const inner = text.lastIndexOf(needle);
        return inner < 0 ? undefined : lineStart + at + inner;
    };

    // 4. Imports and re-exports resolve where expected.
    for (const e of expected.edges.filter((e) => e.kind === "imports" || e.kind === "exports")) {
        const sf = byPath.get(e.path)!;
        const resolved = ts.resolveModuleName(e.specifier!, sf.fileName, owner(e.path).options, ts.sys).resolvedModule;
        const label = `${e.kind} ${e.path} "${e.specifier}"`;
        if (e.to.startsWith("external:")) {
            if (e.specifier!.startsWith("node:")) continue; // built-in; the fixture has no @types/node
            if (!resolved?.isExternalLibraryImport) fail(`${label}: expected an external package`);
        } else if (!resolved || `file:${rel(resolved.resolvedFileName)}` !== e.to) {
            fail(`${label}: resolves to ${resolved ? rel(resolved.resolvedFileName) : "nothing"}, expected ${e.to}`);
        }
    }
    for (const u of expected.unresolved.filter((u) => u.kind === "imports" && u.reason !== "dynamic")) {
        const sf = byPath.get(u.path)!;
        const resolved = ts.resolveModuleName(u.specifier, sf.fileName, owner(u.path).options, ts.sys).resolvedModule;
        const ok = u.reason === "unsupported" ? !resolved || !SOURCE.test(resolved.resolvedFileName) : !resolved;
        if (!ok)
            fail(
                `unresolved import ${u.path} "${u.specifier}" (${u.reason}) resolves to ${resolved?.resolvedFileName}`,
            );
    }

    // 5. Calls, references and heritage reach the expected declaration from the expected symbol.
    for (const e of expected.edges.filter((e) => !["imports", "exports"].includes(e.kind))) {
        const sf = byPath.get(e.path)!;
        const label = `${e.kind} ${e.from} -> ${e.to} (${e.path}:${e.line})`;
        const isSuper = e.text.startsWith("super(");
        const needle = isSuper ? "super" : e.to.split("#")[1]!.split(".").pop()!;
        const pos = positionOf(e.path, e.line, e.text, needle);
        if (pos === undefined) {
            fail(`${label}: cannot find "${needle}" in "${e.text}"`);
            continue;
        }
        if (enclosing(e.path, pos) !== e.from) fail(`${label}: enclosing symbol is ${enclosing(e.path, pos)}`);
        if (isSuper) {
            const heritage = expected.edges.some((h) => h.kind === "extends" && h.from === e.from && h.to === e.to);
            if (!heritage) fail(`${label}: super call without a matching extends edge`);
            continue;
        }
        const id = identifierAt(sf, pos);
        const got = id && target(id, e.path);
        if (got !== e.to) fail(`${label}: compiler resolves it to ${got ?? "nothing"}`);
        if (!declById.has(e.to)) fail(`${label}: target is not a listed symbol`);
    }

    // 6. Unresolved calls really do not resolve to a project declaration (ambiguity is checked
    //    through its TS2308 diagnostic instead: the compiler silently picks one export).
    for (const u of expected.unresolved.filter((u) => u.kind === "calls" && u.reason !== "ambiguous")) {
        const sf = byPath.get(u.path)!;
        const pos = positionOf(u.path, u.line, u.text, u.specifier);
        const id = pos === undefined ? undefined : identifierAt(sf, pos);
        const got = id && target(id, u.path);
        if (!id) fail(`unresolved call ${u.specifier}: not found on ${u.path}:${u.line}`);
        else if (got) fail(`unresolved call ${u.specifier} resolves to ${got}`);
        if (pos !== undefined && enclosing(u.path, pos) !== u.from)
            fail(`unresolved call ${u.specifier}: enclosing symbol is ${enclosing(u.path, pos)}`);
    }
}

const fixtures = readdirSync(fixturesDir, { withFileTypes: true })
    .filter((d) => d.isDirectory() && !NOT_FIXTURES.has(d.name))
    .map((d) => d.name)
    .sort();
if (fixtures.length === 0) {
    console.error(`no fixtures under ${fixturesDir}`);
    process.exit(1);
}
const incomplete = fixtures.filter((name) => !existsSync(join(fixturesDir, name, "expected.json")));
if (incomplete.length > 0) {
    console.error(`every folder under tests/fixtures is a fixture and needs expected.json: ${incomplete.join(", ")}`);
    process.exit(1);
}
for (const name of fixtures) {
    const before = problems;
    console.log(`fixture ${name}`);
    checkFixture(name);
    console.log(problems === before ? "  ✓ agrees with TypeScript " + ts.version : `  ${problems - before} problem(s)`);
}
process.exitCode = problems === 0 ? 0 : 1;
