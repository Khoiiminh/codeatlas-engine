// What one source file refers to (ADR-0015; the conventions are in tests/fixtures/README.md).
//
// Module references: import and export-from declarations, `import x = require()`, `require()`
// calls, `import()` (computed specifiers are dynamic) and type `import()`.
//
// Symbol references, each resolved by the type checker to the declarations it reaches after
// following import aliases, barrels, namespace re-exports and CommonJS exports:
// - call: a call, `new`, a tagged template or a decorator, at the callee's name (`join` in
//   `room.join()`); `super(…)` reaches the base class;
// - reference: a name used as a value without a call, and a JSX element's tag;
// - extends and implements: heritage clauses.
// Not references: declaration names, import and export lists (`export { a }`, `export default
// a`, `module.exports = { a }`), type positions, member reads (`this.state`, `obj.prop`). A
// qualified name through a namespace (`Ui.Badge`, `ns.f()`) counts as the final name, and
// `Enum.Member` as a reference to the enum, whose members are not symbols.
//
// A reached declaration is reported only if it is in a scanned file and could be a symbol (at
// module scope, or a member of a class or interface); the engine keeps those that are symbols
// (codeatlas-core::link). Unresolved: a name nobody declares, a name imported from a module
// that is not found, and a name two `export *` sources export (ambiguous).

import ts from "typescript";
import type { DeclarationSite, ModuleFact, ReferenceFact, ReferenceTarget, WireRange } from "../protocol/messages.ts";
import type { Inventory } from "./inventory.ts";
import type { ModuleResolver } from "./modules.ts";
import { LineMap } from "./positions.ts";

const SK = ts.SyntaxKind;
/** A merged declaration lists at most this many sites; the first symbol among them wins. */
const MAX_SITES = 16;
/** Alias chains are short in practice; this only stops a cycle. */
const MAX_ALIAS_STEPS = 64;

export interface FileFacts {
    modules: ModuleFact[];
    references: ReferenceFact[];
}

/** Shared by every file of one program. */
export class ReferenceAnalyzer {
    private readonly lineMaps = new Map<ts.SourceFile, LineMap>();
    private readonly ambiguity = new Map<ts.Symbol, Map<string, boolean>>();
    private readonly checker: ts.TypeChecker;
    private readonly inventory: Inventory;
    private readonly modules: ModuleResolver;

    constructor(checker: ts.TypeChecker, inventory: Inventory, modules: ModuleResolver) {
        this.checker = checker;
        this.inventory = inventory;
        this.modules = modules;
    }

    lineMap(sf: ts.SourceFile): LineMap {
        let map = this.lineMaps.get(sf);
        if (!map) {
            map = new LineMap(sf.text);
            this.lineMaps.set(sf, map);
        }
        return map;
    }

    analyze(sf: ts.SourceFile): FileFacts {
        return new FileWalker(this, sf).run();
    }

    // ---- symbols ------------------------------------------------------------------------

    /** Follows aliases, CommonJS shorthand exports and `exports.x = x` to the final symbol. */
    follow(symbol: ts.Symbol): { symbol: ts.Symbol } | { unresolved: "not_found" | "ambiguous" | "drop" } {
        let current = symbol;
        for (let step = 0; step < MAX_ALIAS_STEPS; step++) {
            if (current.flags & ts.SymbolFlags.Alias) {
                const declaration = current.declarations?.[0];
                if (declaration && this.isAmbiguousAlias(declaration)) return { unresolved: "ambiguous" };
                const next = this.checker.getImmediateAliasedSymbol(current);
                if (!next || !next.declarations || next.declarations.length === 0) {
                    return { unresolved: declaration ? this.missingAliasReason(declaration) : "drop" };
                }
                current = next;
                continue;
            }
            const declaration = current.valueDeclaration ?? current.declarations?.[0];
            const value = declaration && this.exportedValue(declaration);
            if (value && value !== current) {
                current = value;
                continue;
            }
            break;
        }
        return { symbol: current };
    }

    /** `module.exports = { a }`, `exports.a = a` and `module.exports = { b: a }` stand for `a`. */
    private exportedValue(declaration: ts.Declaration): ts.Symbol | undefined {
        if (ts.isShorthandPropertyAssignment(declaration)) {
            return this.checker.getShorthandAssignmentValueSymbol(declaration);
        }
        if (ts.isPropertyAssignment(declaration) && ts.isIdentifier(declaration.initializer)) {
            const literal = declaration.parent;
            if (ts.isObjectLiteralExpression(literal) && isCommonJsExportAssignment(literal.parent)) {
                return this.checker.getSymbolAtLocation(declaration.initializer);
            }
        }
        const assignment = ts.isBinaryExpression(declaration)
            ? declaration
            : declaration.parent && ts.isBinaryExpression(declaration.parent)
              ? declaration.parent
              : undefined;
        if (assignment && isCommonJsExportAssignment(assignment) && ts.isIdentifier(skipOuter(assignment.right))) {
            return this.checker.getSymbolAtLocation(skipOuter(assignment.right));
        }
        return undefined;
    }

    /** An import of a name from a module two `export *` sources export it from. */
    private isAmbiguousAlias(declaration: ts.Declaration): boolean {
        let moduleSpecifier: ts.Expression | undefined;
        let name: string;
        if (ts.isImportSpecifier(declaration)) {
            moduleSpecifier = declaration.parent.parent.parent.moduleSpecifier;
            name = (declaration.propertyName ?? declaration.name).text;
        } else if (ts.isExportSpecifier(declaration) && declaration.parent.parent.moduleSpecifier) {
            moduleSpecifier = declaration.parent.parent.moduleSpecifier;
            name = (declaration.propertyName ?? declaration.name).text;
        } else {
            return false;
        }
        const module = this.checker.getSymbolAtLocation(moduleSpecifier);
        return module ? this.exportsAmbiguously(module, name) : false;
    }

    exportsAmbiguously(module: ts.Symbol, name: string): boolean {
        let byName = this.ambiguity.get(module);
        if (!byName) {
            byName = new Map();
            this.ambiguity.set(module, byName);
        }
        let ambiguous = byName.get(name);
        if (ambiguous === undefined) {
            ambiguous = this.exportCandidates(module, name, new Set([module])).size > 1;
            byName.set(name, ambiguous);
        }
        return ambiguous;
    }

    /** The final symbols `name` can mean when imported from `module`. */
    private exportCandidates(module: ts.Symbol, name: string, visited: Set<ts.Symbol>): Set<ts.Symbol> {
        const exports = module.exports;
        const direct = exports?.get(ts.escapeLeadingUnderscores(name));
        if (direct) {
            const final = direct.flags & ts.SymbolFlags.Alias ? this.checker.getAliasedSymbol(direct) : direct;
            return new Set([final]);
        }
        const found = new Set<ts.Symbol>();
        if (name === "default") return found;
        const stars = exports?.get(ts.InternalSymbolName.ExportStar as ts.__String)?.declarations ?? [];
        for (const star of stars) {
            if (!ts.isExportDeclaration(star) || !star.moduleSpecifier) continue;
            const source = this.checker.getSymbolAtLocation(star.moduleSpecifier);
            if (!source || visited.has(source)) continue;
            visited.add(source);
            for (const candidate of this.exportCandidates(source, name, visited)) found.add(candidate);
        }
        return found;
    }

    /** A name imported from a module that does not resolve: not found, or not ours to judge. */
    private missingAliasReason(declaration: ts.Declaration): "not_found" | "drop" {
        const specifier = importSpecifierOf(declaration);
        if (!specifier) return "drop";
        const target = this.modules.target(specifier.text, specifier, declaration.getSourceFile());
        // A package or a JSON module: its names are not project symbols. A missing module, or a
        // scanned file that does not export the name: not found.
        if (target.kind === "external") return "drop";
        if (target.kind === "unresolved" && target.reason !== "not_found") return "drop";
        return "not_found";
    }

    /** Where `symbol` is declared in scanned files, as candidates for a symbol. */
    sitesOf(symbol: ts.Symbol): DeclarationSite[] {
        const sites: DeclarationSite[] = [];
        for (const declaration of symbol.declarations ?? []) {
            const sf = declaration.getSourceFile();
            const file = this.inventory.get(sf.fileName);
            if (!file || !couldBeSymbol(declaration)) continue;
            sites.push({ path: file.path, at: this.lineMap(sf).position(namePosition(declaration, sf)) });
            if (sites.length === MAX_SITES) break;
        }
        return sites;
    }

    /** What a symbol found at a name means: declarations, unresolved, or nothing to report. */
    targetOf(symbol: ts.Symbol | undefined, specifier: string): ReferenceTarget | undefined {
        if (!symbol) return { kind: "unresolved", specifier, reason: "not_found" };
        const followed = this.follow(symbol);
        if ("unresolved" in followed) {
            return followed.unresolved === "drop"
                ? undefined
                : { kind: "unresolved", specifier, reason: followed.unresolved };
        }
        const sites = this.sitesOf(followed.symbol);
        return sites.length > 0 ? { kind: "declarations", sites } : undefined;
    }

    symbolAt(node: ts.Node): ts.Symbol | undefined {
        return this.checker.getSymbolAtLocation(node);
    }

    /** The value `{ a }` means: the `a` in scope, not the object's property. */
    symbolAtShorthand(node: ts.ShorthandPropertyAssignment): ts.Symbol | undefined {
        return this.checker.getShorthandAssignmentValueSymbol(node);
    }

    /** What the head of `head.name` is: a namespace or module, an enum, or anything else. */
    headKind(
        head: ts.Expression,
    ):
        | { kind: "namespace"; symbol: ts.Symbol }
        | { kind: "enum"; at: ts.Node; symbol: ts.Symbol }
        | { kind: "missing" }
        | { kind: "other" } {
        const node = skipOuter(head);
        const nameNode = ts.isIdentifier(node) ? node : ts.isPropertyAccessExpression(node) ? node.name : undefined;
        if (!nameNode) return { kind: "other" };
        const symbol = this.checker.getSymbolAtLocation(nameNode);
        if (!symbol) return { kind: "other" };
        const followed = this.follow(symbol);
        if ("unresolved" in followed)
            return followed.unresolved === "not_found" ? { kind: "missing" } : { kind: "other" };
        const final = followed.symbol;
        if (final.flags & (ts.SymbolFlags.ValueModule | ts.SymbolFlags.NamespaceModule)) {
            return { kind: "namespace", symbol: final };
        }
        if (final.declarations?.some((d) => ts.isSourceFile(d))) return { kind: "namespace", symbol: final };
        if (final.flags & ts.SymbolFlags.Enum) return { kind: "enum", at: nameNode, symbol: final };
        return { kind: "other" };
    }

    /** `require` as the CommonJS loader, not a project function of that name. */
    isRequire(call: ts.CallExpression): boolean {
        const callee = call.expression;
        if (!ts.isIdentifier(callee) || callee.text !== "require" || call.arguments.length !== 1) return false;
        const symbol = this.checker.getSymbolAtLocation(callee);
        return !symbol?.declarations?.some((d) => this.inventory.get(d.getSourceFile().fileName) !== undefined);
    }

    moduleTarget(literal: ts.StringLiteralLike, sf: ts.SourceFile) {
        return this.modules.target(literal.text, literal, sf);
    }
}

class FileWalker {
    private readonly modules: ModuleFact[] = [];
    private readonly references: ReferenceFact[] = [];
    private readonly lines: LineMap;
    private readonly a: ReferenceAnalyzer;
    private readonly sf: ts.SourceFile;

    constructor(analyzer: ReferenceAnalyzer, sf: ts.SourceFile) {
        this.a = analyzer;
        this.sf = sf;
        this.lines = analyzer.lineMap(sf);
    }

    run(): FileFacts {
        this.visit(this.sf);
        return { modules: this.modules, references: this.references };
    }

    private range(node: ts.Node): WireRange {
        return [...this.lines.position(node.getStart(this.sf)), ...this.lines.position(node.getEnd())];
    }

    private module(kind: ModuleFact["kind"], node: ts.Expression, typeOnly: boolean): void {
        if (ts.isStringLiteralLike(node)) {
            this.modules.push({
                kind,
                specifier: node.text,
                range: this.range(node),
                typeOnly,
                target: this.a.moduleTarget(node, this.sf),
            });
        } else {
            this.modules.push({
                kind,
                specifier: node.getText(this.sf),
                range: this.range(node),
                typeOnly,
                target: { kind: "unresolved", reason: "dynamic" },
            });
        }
    }

    private add(kind: ReferenceFact["kind"], at: ts.Node, target: ReferenceTarget | undefined): void {
        if (target) this.references.push({ kind, range: this.range(at), target });
    }

    private visit = (node: ts.Node): void => {
        switch (node.kind) {
            case SK.ImportDeclaration: {
                const n = node as ts.ImportDeclaration;
                this.module("import", n.moduleSpecifier, n.importClause?.isTypeOnly ?? false);
                return;
            }
            case SK.ImportEqualsDeclaration: {
                const n = node as ts.ImportEqualsDeclaration;
                if (ts.isExternalModuleReference(n.moduleReference))
                    this.module("import", n.moduleReference.expression, n.isTypeOnly);
                return;
            }
            case SK.ExportDeclaration: {
                const n = node as ts.ExportDeclaration;
                if (n.moduleSpecifier) this.module("re_export", n.moduleSpecifier, n.isTypeOnly);
                return;
            }
            case SK.ExportAssignment: {
                // `export default a` and `export = a` list an export; anything else is code.
                const n = node as ts.ExportAssignment;
                if (!ts.isIdentifier(skipOuter(n.expression))) this.visit(n.expression);
                return;
            }
            case SK.ImportType: {
                const n = node as ts.ImportTypeNode;
                if (ts.isLiteralTypeNode(n.argument) && ts.isStringLiteral(n.argument.literal))
                    this.module("import", n.argument.literal, true);
                return;
            }
            case SK.ModuleDeclaration: {
                // `declare module "x" { … }` describes another module; it refers to nothing here.
                if (ts.isStringLiteral((node as ts.ModuleDeclaration).name)) return;
                break;
            }
            case SK.CallExpression:
                return this.call(node as ts.CallExpression);
            case SK.NewExpression: {
                const n = node as ts.NewExpression;
                this.callee(n.expression);
                n.arguments?.forEach(this.visit);
                return;
            }
            case SK.TaggedTemplateExpression: {
                const n = node as ts.TaggedTemplateExpression;
                this.callee(n.tag);
                this.visit(n.template);
                return;
            }
            case SK.Decorator: {
                // `@Foo()` is a call; `@Foo` applies Foo, which is a call too.
                const e = (node as ts.Decorator).expression;
                if (ts.isCallExpression(e)) this.visit(e);
                else this.callee(e);
                return;
            }
            case SK.HeritageClause:
                return this.heritage(node as ts.HeritageClause);
            case SK.ExpressionWithTypeArguments:
                // Outside a heritage clause, an instantiation expression `f<T>`: a use of `f`.
                this.visit((node as ts.ExpressionWithTypeArguments).expression);
                return;
            case SK.JsxOpeningElement:
            case SK.JsxSelfClosingElement: {
                const n = node as ts.JsxOpeningElement | ts.JsxSelfClosingElement;
                this.jsxTag(n.tagName);
                this.visit(n.attributes);
                return;
            }
            case SK.JsxClosingElement:
                return;
            case SK.PropertyAccessExpression:
                return this.memberRead(node as ts.PropertyAccessExpression);
            case SK.ShorthandPropertyAssignment: {
                const n = node as ts.ShorthandPropertyAssignment;
                if (!isCommonJsExportList(n.parent)) {
                    this.add("reference", n.name, this.a.targetOf(this.a.symbolAtShorthand(n), n.name.text));
                }
                if (n.objectAssignmentInitializer) this.visit(n.objectAssignmentInitializer);
                return;
            }
            case SK.PropertyAssignment: {
                const n = node as ts.PropertyAssignment;
                if (ts.isComputedPropertyName(n.name)) this.visit(n.name);
                // `module.exports = { b: a }` lists an export.
                if (isCommonJsExportList(n.parent) && ts.isIdentifier(skipOuter(n.initializer))) return;
                this.visit(n.initializer);
                return;
            }
            case SK.BinaryExpression: {
                const n = node as ts.BinaryExpression;
                if (isCommonJsExportAssignment(n)) {
                    // `module.exports = a` and `exports.a = a` list an export.
                    if (!ts.isIdentifier(skipOuter(n.right))) this.visit(n.right);
                    return;
                }
                break;
            }
            case SK.Identifier:
                return this.identifier(node as ts.Identifier);
        }
        if (ts.isTypeNode(node)) return;
        ts.forEachChild(node, this.visit);
    };

    private call(n: ts.CallExpression): void {
        const callee = n.expression;
        if (callee.kind === SK.ImportKeyword) {
            const specifier = n.arguments[0];
            if (specifier) this.module("import", specifier, false);
            return;
        }
        if (this.a.isRequire(n)) {
            this.module("import", n.arguments[0]!, false);
            return;
        }
        if (callee.kind === SK.SuperKeyword) {
            this.add("call", callee, this.superTarget(callee));
        } else {
            this.callee(callee);
        }
        n.arguments.forEach(this.visit);
    }

    /** The callee of a call, `new`, tagged template or decorator. */
    private callee(expression: ts.Expression): void {
        const e = skipOuter(expression);
        if (ts.isIdentifier(e)) {
            this.add("call", e, this.a.targetOf(this.a.symbolAt(e), e.text));
            return;
        }
        if (ts.isPropertyAccessExpression(e)) {
            const head = this.a.headKind(e.expression);
            if (head.kind === "missing") {
                this.add("call", e.name, { kind: "unresolved", specifier: e.getText(this.sf), reason: "not_found" });
                return;
            }
            if (head.kind === "namespace") {
                this.qualified("call", e, head.symbol);
                return;
            }
            this.receiver(e.expression);
            const symbol = this.a.symbolAt(e.name);
            if (symbol) this.add("call", e.name, this.a.targetOf(symbol, e.name.text));
            return;
        }
        if (ts.isElementAccessExpression(e)) {
            this.receiver(e.expression);
            this.visit(e.argumentExpression);
            return;
        }
        this.visit(e);
    }

    /** The object of a member access: visited for what it contains, but not a reference itself. */
    private receiver(expression: ts.Expression): void {
        const e = skipOuter(expression);
        if (ts.isIdentifier(e)) return;
        this.visit(e);
    }

    /** `a.b` read as a value. */
    private memberRead(n: ts.PropertyAccessExpression): void {
        const head = this.a.headKind(n.expression);
        if (head.kind === "namespace") {
            this.qualified("reference", n, head.symbol);
            return;
        }
        if (head.kind === "enum") {
            this.add("reference", head.at, this.siteTarget(head.symbol));
            return;
        }
        this.receiver(n.expression);
    }

    private identifier(id: ts.Identifier): void {
        if (!isValueName(id)) return;
        this.add("reference", id, this.a.targetOf(this.a.symbolAt(id), id.text));
    }

    private jsxTag(tag: ts.JsxTagNameExpression): void {
        if (ts.isIdentifier(tag)) {
            if (/^[a-z]/.test(tag.text) || tag.text.includes("-")) return; // an intrinsic element
            this.add("reference", tag, this.a.targetOf(this.a.symbolAt(tag), tag.text));
        } else if (ts.isPropertyAccessExpression(tag)) {
            const head = this.a.headKind(tag.expression);
            if (head.kind === "namespace") {
                this.qualified("reference", tag, head.symbol);
                return;
            }
            const symbol = this.a.symbolAt(tag.name);
            if (symbol) this.add("reference", tag.name, this.a.targetOf(symbol, tag.name.text));
        }
    }

    /**
     * `ns.name` through a namespace or module: the final name, unless two `export *` sources of
     * the module export it (TypeScript would silently pick one).
     */
    private qualified(kind: ReferenceFact["kind"], n: ts.PropertyAccessExpression, module: ts.Symbol): void {
        if (this.a.exportsAmbiguously(module, n.name.text)) {
            this.add(kind, n.name, { kind: "unresolved", specifier: n.getText(this.sf), reason: "ambiguous" });
            return;
        }
        const symbol = this.a.symbolAt(n.name);
        if (symbol) this.add(kind, n.name, this.a.targetOf(symbol, n.name.text));
    }

    private heritage(clause: ts.HeritageClause): void {
        const kind = clause.token === SK.ExtendsKeyword ? "extends" : "implements";
        for (const type of clause.types) {
            const e = skipOuter(type.expression);
            const name = ts.isIdentifier(e)
                ? e
                : ts.isPropertyAccessExpression(e) && isNameChain(e)
                  ? e.name
                  : undefined;
            if (!name) {
                this.visit(e); // `extends mixin(Base)`: the calls inside are calls.
                continue;
            }
            const target = this.a.targetOf(this.a.symbolAt(name), name.text);
            // Heritage edges exist between project symbols only; an unknown base is not reported.
            if (target?.kind === "declarations") this.add(kind, name, target);
        }
    }

    /** `super(…)` reaches the class named in `extends`. */
    private superTarget(superKeyword: ts.Expression): ReferenceTarget | undefined {
        const cls = ts.findAncestor(superKeyword, ts.isClassLike);
        const base = cls?.heritageClauses?.find((h) => h.token === SK.ExtendsKeyword)?.types[0]?.expression;
        if (!base) return undefined;
        const e = skipOuter(base);
        const name = ts.isIdentifier(e) ? e : ts.isPropertyAccessExpression(e) ? e.name : undefined;
        const target = name && this.a.targetOf(this.a.symbolAt(name), name.text);
        return target?.kind === "declarations" ? target : undefined;
    }

    private siteTarget(symbol: ts.Symbol): ReferenceTarget | undefined {
        const sites = this.a.sitesOf(symbol);
        return sites.length > 0 ? { kind: "declarations", sites } : undefined;
    }
}

// ---- syntax helpers -----------------------------------------------------------------------

function skipOuter(e: ts.Expression): ts.Expression {
    while (ts.isParenthesizedExpression(e) || ts.isNonNullExpression(e)) e = e.expression;
    return e;
}

function isNameChain(e: ts.Expression): boolean {
    return ts.isIdentifier(e) || (ts.isPropertyAccessExpression(e) && isNameChain(e.expression));
}

/** `module.exports`, `exports`, or a property of either. */
function isCommonJsExportTarget(e: ts.Expression): boolean {
    if (ts.isIdentifier(e)) return e.text === "exports";
    if (ts.isPropertyAccessExpression(e) || ts.isElementAccessExpression(e)) {
        const object = e.expression;
        if (ts.isIdentifier(object) && object.text === "module" && ts.isPropertyAccessExpression(e))
            return e.name.text === "exports";
        return isCommonJsExportTarget(object);
    }
    return false;
}

function isCommonJsExportAssignment(node: ts.Node | undefined): node is ts.BinaryExpression {
    return (
        node !== undefined &&
        ts.isBinaryExpression(node) &&
        node.operatorToken.kind === SK.EqualsToken &&
        isCommonJsExportTarget(node.left)
    );
}

function isCommonJsExportList(literal: ts.Node): boolean {
    return ts.isObjectLiteralExpression(literal) && isCommonJsExportAssignment(literal.parent);
}

/** An identifier read as a value: not a declaration name, label, property name or type. */
function isValueName(id: ts.Identifier): boolean {
    const parent = id.parent;
    if (!parent) return false;
    const named = parent as { name?: ts.Node; propertyName?: ts.Node; label?: ts.Node };
    if (named.name === id || named.propertyName === id || named.label === id) return false;
    if (ts.isQualifiedName(parent)) return false;
    return true;
}

/** The module specifier an alias declaration imports from. */
function importSpecifierOf(declaration: ts.Declaration): ts.StringLiteralLike | undefined {
    let node: ts.Node | undefined = declaration;
    for (let i = 0; node && i < 6; i++, node = node.parent) {
        if (ts.isImportDeclaration(node) || ts.isExportDeclaration(node)) {
            const s = node.moduleSpecifier;
            return s && ts.isStringLiteralLike(s) ? s : undefined;
        }
        if (ts.isImportEqualsDeclaration(node)) {
            const r = node.moduleReference;
            return ts.isExternalModuleReference(r) && ts.isStringLiteralLike(r.expression) ? r.expression : undefined;
        }
        if (ts.isVariableDeclaration(node) && node.initializer) {
            const init = node.initializer;
            const call = ts.isCallExpression(init)
                ? init
                : ts.isPropertyAccessExpression(init) && ts.isCallExpression(init.expression)
                  ? init.expression
                  : undefined;
            const arg = call?.arguments[0];
            return call &&
                ts.isIdentifier(call.expression) &&
                call.expression.text === "require" &&
                arg &&
                ts.isStringLiteralLike(arg)
                ? arg
                : undefined;
        }
    }
    return undefined;
}

/** Declarations that may be symbols: module scope, class and interface members, parameter properties. */
function couldBeSymbol(d: ts.Declaration): boolean {
    const atModuleScope = (n: ts.Node | undefined) => n !== undefined && (ts.isSourceFile(n) || ts.isModuleBlock(n));
    switch (d.kind) {
        case SK.FunctionDeclaration:
        case SK.ClassDeclaration:
        case SK.InterfaceDeclaration:
        case SK.TypeAliasDeclaration:
        case SK.EnumDeclaration:
        case SK.ModuleDeclaration:
            return atModuleScope(d.parent);
        case SK.VariableDeclaration:
            return atModuleScope(d.parent?.parent?.parent);
        case SK.BindingElement: {
            let n: ts.Node = d;
            while (ts.isBindingElement(n) || ts.isObjectBindingPattern(n) || ts.isArrayBindingPattern(n)) n = n.parent;
            return ts.isVariableDeclaration(n) && atModuleScope(n.parent?.parent?.parent);
        }
        case SK.MethodDeclaration:
        case SK.PropertyDeclaration:
        case SK.GetAccessor:
        case SK.SetAccessor:
            return ts.isClassLike(d.parent);
        case SK.MethodSignature:
        case SK.PropertySignature:
            return ts.isInterfaceDeclaration(d.parent);
        case SK.Parameter:
            return ts.isConstructorDeclaration(d.parent) && ts.isParameterPropertyDeclaration(d, d.parent);
        case SK.ExportAssignment:
        case SK.NamespaceExport:
        case SK.BinaryExpression:
        case SK.PropertyAccessExpression:
            return true;
        default:
            return false;
    }
}

/** The start of a declaration's name, or of `default` in an anonymous default export. */
function namePosition(d: ts.Declaration, sf: ts.SourceFile): number {
    const name = ts.getNameOfDeclaration(d);
    if (name) return name.getStart(sf);
    const modifiers = ts.canHaveModifiers(d) ? ts.getModifiers(d) : undefined;
    const keyword = modifiers?.find((m) => m.kind === SK.DefaultKeyword);
    if (keyword) return keyword.getStart(sf);
    if (ts.isExportAssignment(d)) {
        const token = d.getChildren(sf).find((c) => c.kind === SK.DefaultKeyword);
        if (token) return token.getStart(sf);
    }
    return d.getStart(sf);
}
