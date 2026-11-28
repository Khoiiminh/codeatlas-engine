// Where a module specifier leads (ADR-0015, tests/fixtures/README.md "Edges" and "Unresolved").
//
// - A scanned source file: the file. A declaration file whose JavaScript source was scanned
//   (legacy.d.ts next to legacy.js) stands for that source.
// - A package or built-in: external, whether or not it is installed (the engine never installs
//   dependencies, ADR-0004). `node:` specifiers are always external.
// - unsupported: the module exists but is not scanned source (JSON, CSS, a declaration file, a
//   file outside the root or in an ignored folder).
// - not_found: a relative or absolute specifier, or one that matches a `paths` alias, that
//   leads nowhere.

import { dirname, resolve } from "node:path";
import ts from "typescript";
import type { ModuleTarget } from "../protocol/messages.ts";
import type { Inventory } from "./inventory.ts";

export interface ModuleResolver {
    target(specifier: string, literal: ts.StringLiteralLike, sourceFile: ts.SourceFile): ModuleTarget;
}

const DECLARATION_SOURCES: [RegExp, string[]][] = [
    [/\.d\.ts$/, [".js", ".jsx"]],
    [/\.d\.mts$/, [".mjs"]],
    [/\.d\.cts$/, [".cjs"]],
];

export function createModuleResolver(
    program: ts.Program,
    options: ts.CompilerOptions,
    host: ts.CompilerHost,
    inventory: Inventory,
): ModuleResolver {
    const cache = ts.createModuleResolutionCache(
        host.getCurrentDirectory(),
        (f) => host.getCanonicalFileName(f),
        options,
    );
    return {
        target(specifier, literal, sourceFile) {
            if (specifier.startsWith("node:")) return { kind: "external" };
            const mode = program.getModeForUsageLocation(sourceFile, literal);
            const resolved = ts.resolveModuleName(
                specifier,
                sourceFile.fileName,
                options,
                host,
                cache,
                undefined,
                mode,
            ).resolvedModule;
            if (resolved) {
                if (resolved.isExternalLibraryImport) return { kind: "external" };
                const file =
                    inventory.get(resolved.resolvedFileName) ?? declarationSource(resolved.resolvedFileName, inventory);
                return file ? { kind: "file", path: file.path } : { kind: "unresolved", reason: "unsupported" };
            }
            if (isPathLike(specifier)) {
                const candidate = resolve(dirname(sourceFile.fileName), specifier);
                return { kind: "unresolved", reason: ts.sys.fileExists(candidate) ? "unsupported" : "not_found" };
            }
            if (matchesPaths(options.paths, specifier) || !isPackageName(specifier)) {
                return { kind: "unresolved", reason: "not_found" };
            }
            return { kind: "external" };
        },
    };
}

/**
 * A specifier a package could answer to: `name` or `@scope/name`, then any subpath. `@/x`,
 * `~lib/x` and `#x` are not package names, so when they do not resolve they are aliases
 * that lead nowhere, not packages that are missing.
 */
function isPackageName(specifier: string): boolean {
    return /^(@[a-z0-9][a-z0-9._-]*\/)?[a-z0-9][a-z0-9._-]*(\/.*)?$/i.test(specifier);
}

function declarationSource(fileName: string, inventory: Inventory) {
    for (const [suffix, extensions] of DECLARATION_SOURCES) {
        if (!suffix.test(fileName)) continue;
        for (const extension of extensions) {
            const source = inventory.get(fileName.replace(suffix, extension));
            if (source) return source;
        }
    }
    return undefined;
}

function isPathLike(specifier: string): boolean {
    return (
        ts.isExternalModuleNameRelative(specifier) || /^[A-Za-z]:[\\/]/.test(specifier) || specifier.startsWith("file:")
    );
}

function matchesPaths(paths: ts.MapLike<string[]> | undefined, specifier: string): boolean {
    for (const pattern of Object.keys(paths ?? {})) {
        const star = pattern.indexOf("*");
        if (
            star < 0
                ? pattern === specifier
                : specifier.startsWith(pattern.slice(0, star)) && specifier.endsWith(pattern.slice(star + 1))
        )
            return true;
    }
    return false;
}
