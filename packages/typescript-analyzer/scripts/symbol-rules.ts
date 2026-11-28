// ENG-03: the symbol rules, written a second time against the TypeScript compiler.
//
// The Rust extractor (crates/adapters/codeatlas-parser-treesitter/src/symbols.rs, with the
// identity rules in codeatlas-core/src/symbol.rs) reads Tree-sitter trees. This module applies
// the same written rules (tests/fixtures/README.md "Symbols", extended in
// crates/adapters/codeatlas-parser-treesitter/tests/symbols/README.md) to the compiler's syntax
// tree, and asks the type checker what each module exports. The two share no code, so when
// check-fixtures.ts and check-symbol-cases.ts find them in agreement, neither has misread a rule
// the other got right.

import ts from "typescript";

export type SymbolKind =
    "namespace" | "class" | "interface" | "type_alias" | "enum" | "function" | "method" | "property" | "variable";

export interface Declaration {
    kind: SymbolKind;
    /** The declaration node; for a variable its declarator or binding element. */
    node: ts.Node;
    /** The name, or the `default` keyword of an anonymous default export. */
    nameNode: ts.Node;
}

/** One declaration with its qualified name, before declarations with one name are merged. */
export interface Found {
    qualified: string[];
    declaration: Declaration;
    /** The node the module's export table would point at. */
    exportNode: ts.Node;
}

export interface SymbolRecord {
    id: string;
    name: string;
    qualifiedName: string;
    kind: SymbolKind;
    parent?: string;
    exported: boolean;
    /** In document order. */
    declarations: Declaration[];
}

/** Same limit as the Rust extractor. */
const MAX_NAMESPACE_DEPTH = 32;

/** The kind of a merged symbol is the first of these among its declarations. */
const RANK: SymbolKind[] = [
    "class",
    "enum",
    "function",
    "method",
    "variable",
    "property",
    "namespace",
    "interface",
    "type_alias",
];

/** A constructor parameter with one of these modifiers declares a property. */
const PROPERTY_MODIFIERS = new Set([
    ts.SyntaxKind.PublicKeyword,
    ts.SyntaxKind.PrivateKeyword,
    ts.SyntaxKind.ProtectedKeyword,
    ts.SyntaxKind.ReadonlyKeyword,
    ts.SyntaxKind.OverrideKeyword,
]);

export function symbolId(path: string, qualifiedName: string): string {
    return `symbol:${path}#${qualifiedName}`;
}

/** Every name a binding pattern binds, with its binding element. */
function bindings(pattern: ts.BindingPattern): [ts.Identifier, ts.BindingElement][] {
    const out: [ts.Identifier, ts.BindingElement][] = [];
    const stack: ts.BindingPattern[] = [pattern];
    while (stack.length > 0) {
        for (const element of stack.pop()!.elements) {
            if (ts.isOmittedExpression(element)) continue;
            if (ts.isIdentifier(element.name)) out.push([element.name, element]);
            else stack.push(element.name);
        }
    }
    return out;
}

/** The declared name, or `default` for an anonymous `export default function/class`. */
function nameOf(node: ts.FunctionDeclaration | ts.ClassDeclaration): { text: string; node: ts.Node } | undefined {
    if (node.name) return { text: node.name.text, node: node.name };
    const keyword = ts.getModifiers(node)?.find((m) => m.kind === ts.SyntaxKind.DefaultKeyword);
    return keyword ? { text: "default", node: keyword } : undefined;
}

/** Every declaration the rules count as a symbol, in document order. */
export function declarationsOf(sf: ts.SourceFile): Found[] {
    const found: Found[] = [];
    const add = (
        prefix: string[],
        name: string,
        kind: SymbolKind,
        node: ts.Node,
        nameNode: ts.Node,
        exportNode = node,
    ) => {
        const qualified = [...prefix, name];
        found.push({ qualified, declaration: { kind, node, nameNode }, exportNode });
        return qualified;
    };
    const members = (owner: string[], list: ts.NodeArray<ts.ClassElement | ts.TypeElement>) => {
        for (const m of list) {
            if (ts.isConstructorDeclaration(m)) {
                if (!m.body) continue;
                for (const p of m.parameters) {
                    const modified = (ts.getModifiers(p) ?? []).some((x) => PROPERTY_MODIFIERS.has(x.kind));
                    if (modified && ts.isIdentifier(p.name)) add(owner, p.name.text, "property", p, p.name);
                }
                continue;
            }
            if (!m.name || !(ts.isIdentifier(m.name) || ts.isPrivateIdentifier(m.name))) continue;
            let kind: SymbolKind;
            if (ts.isMethodDeclaration(m) || ts.isMethodSignature(m)) kind = "method";
            else if (
                ts.isPropertyDeclaration(m) ||
                ts.isPropertySignature(m) ||
                ts.isGetAccessorDeclaration(m) ||
                ts.isSetAccessorDeclaration(m)
            )
                kind = "property";
            else continue;
            add(owner, m.name.text, kind, m, m.name);
        }
    };
    const namespace = (declaration: ts.ModuleDeclaration, prefix: string[]) => {
        // `declare module "x"` and `declare global` describe other scopes.
        if (!ts.isIdentifier(declaration.name) || declaration.flags & ts.NodeFlags.GlobalAugmentation) return;
        let node = declaration;
        let qualified = prefix;
        for (;;) {
            if (qualified.length >= MAX_NAMESPACE_DEPTH) return;
            qualified = add(qualified, (node.name as ts.Identifier).text, "namespace", node, node.name);
            const body = node.body;
            if (body && ts.isModuleDeclaration(body)) {
                node = body; // namespace A.B { … }
                continue;
            }
            if (body && ts.isModuleBlock(body)) statements(body.statements, qualified);
            return;
        }
    };
    const statement = (st: ts.Statement, prefix: string[]) => {
        if (ts.isFunctionDeclaration(st)) {
            const name = nameOf(st);
            if (name) add(prefix, name.text, "function", st, name.node);
        } else if (ts.isClassDeclaration(st)) {
            const name = nameOf(st);
            if (name) members(add(prefix, name.text, "class", st, name.node), st.members);
        } else if (ts.isInterfaceDeclaration(st)) {
            members(add(prefix, st.name.text, "interface", st, st.name), st.members);
        } else if (ts.isTypeAliasDeclaration(st)) {
            add(prefix, st.name.text, "type_alias", st, st.name);
        } else if (ts.isEnumDeclaration(st)) {
            add(prefix, st.name.text, "enum", st, st.name);
        } else if (ts.isVariableStatement(st)) {
            for (const d of st.declarationList.declarations) {
                if (ts.isIdentifier(d.name)) add(prefix, d.name.text, "variable", d, d.name);
                else for (const [name, element] of bindings(d.name)) add(prefix, name.text, "variable", element, name);
            }
        } else if (
            ts.isExportDeclaration(st) &&
            prefix.length === 0 &&
            st.exportClause &&
            ts.isNamespaceExport(st.exportClause) &&
            ts.isIdentifier(st.exportClause.name)
        ) {
            add(prefix, st.exportClause.name.text, "namespace", st, st.exportClause.name, st.exportClause);
        } else if (ts.isModuleDeclaration(st)) {
            namespace(st, prefix);
        }
    };
    const statements = (list: readonly ts.Statement[], prefix: string[]) => {
        for (const st of list) statement(st, prefix);
    };
    statements(sf.statements, []);
    return found;
}

/** Declarations the module exports, by any route: ESM, `export =`, or CommonJS in JavaScript. */
export function exportedNodes(sf: ts.SourceFile, checker: ts.TypeChecker): Set<ts.Node> {
    const out = new Set<ts.Node>();
    const addSymbol = (symbol: ts.Symbol | undefined) => {
        for (const d of symbol?.declarations ?? []) out.add(d);
    };
    const addValue = (expression: ts.Expression | undefined) => {
        if (expression && ts.isIdentifier(expression)) addSymbol(checker.getSymbolAtLocation(expression));
    };
    const addAssigned = (value: ts.Expression) => {
        if (!ts.isObjectLiteralExpression(value)) return addValue(value);
        for (const p of value.properties) {
            if (ts.isShorthandPropertyAssignment(p)) addSymbol(checker.getShorthandAssignmentValueSymbol(p));
            else if (ts.isPropertyAssignment(p)) addValue(p.initializer);
        }
    };
    // The checker returns no symbol for a `.js` file that is a CommonJS module only (no ESM
    // syntax, not `.cjs`), although the binder gave it one with an export table.
    const moduleSymbol = checker.getSymbolAtLocation(sf) ?? (sf as unknown as { symbol?: ts.Symbol }).symbol;
    moduleSymbol?.exports?.forEach((exported) => {
        addSymbol(exported);
        if (exported.flags & ts.SymbolFlags.Alias) addSymbol(checker.getAliasedSymbol(exported));
        for (const d of exported.declarations ?? []) {
            if (ts.isExportAssignment(d)) addValue(d.expression);
            else if (ts.isBinaryExpression(d)) addAssigned(d.right);
            else if (ts.isPropertyAccessExpression(d) && ts.isBinaryExpression(d.parent)) addAssigned(d.parent.right);
            else if (ts.isShorthandPropertyAssignment(d)) addSymbol(checker.getShorthandAssignmentValueSymbol(d));
            else if (ts.isPropertyAssignment(d)) addValue(d.initializer);
        }
    });
    return out;
}

/** Strings compared by UTF-8 bytes, as Rust compares them. */
function byteOrder(a: string, b: string): number {
    return Buffer.compare(Buffer.from(a), Buffer.from(b));
}

/** The file's symbols, merged and ordered as `codeatlas_core::build_symbols` does. */
export function symbolsOf(path: string, sf: ts.SourceFile, checker: ts.TypeChecker): SymbolRecord[] {
    const exported = exportedNodes(sf, checker);
    const groups = new Map<string, { segments: string[]; declarations: Declaration[]; exported: boolean }>();
    for (const f of declarationsOf(sf)) {
        const key = f.qualified.join(".");
        let group = groups.get(key);
        if (!group) {
            group = { segments: f.qualified, declarations: [], exported: false };
            groups.set(key, group);
        }
        group.declarations.push(f.declaration);
        group.exported ||= f.qualified.length === 1 && exported.has(f.exportNode);
    }
    const start = (d: Declaration) => d.node.getStart(sf);
    const records = [...groups].map(([qualifiedName, group]): SymbolRecord => {
        group.declarations.sort(
            (a, b) =>
                start(a) - start(b) ||
                a.node.getEnd() - b.node.getEnd() ||
                a.nameNode.getStart(sf) - b.nameNode.getStart(sf),
        );
        const kind = RANK.find((k) => group.declarations.some((d) => d.kind === k))!;
        const segments = group.segments;
        return {
            id: symbolId(path, qualifiedName),
            name: segments[segments.length - 1]!,
            qualifiedName,
            kind,
            ...(segments.length > 1 ? { parent: symbolId(path, segments.slice(0, -1).join(".")) } : {}),
            exported: group.exported,
            declarations: group.declarations,
        };
    });
    records.sort(
        (a, b) => start(a.declarations[0]!) - start(b.declarations[0]!) || byteOrder(a.qualifiedName, b.qualifiedName),
    );
    return records;
}

/** The declaration that locates a symbol: the first one of the symbol's kind. */
export function primary(symbol: SymbolRecord): Declaration {
    return symbol.declarations.find((d) => d.kind === symbol.kind) ?? symbol.declarations[0]!;
}
