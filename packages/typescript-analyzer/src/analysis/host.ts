// A compiler host that reads scanned files the way the scanner did, and shares the parsed
// default library between programs.

import { dirname } from "node:path";
import ts from "typescript";
import { Inventory, canonical, toSlashes } from "./inventory.ts";

export interface ReadResult {
    sha256: string;
}

export interface AnalysisHost {
    host: ts.CompilerHost;
    /** What the host read for each inventory file of this program, by repository path. */
    reads: Map<string, ReadResult | { error: string }>;
}

/** Parsed default-library files, shared by every program of a run (they are the same text). */
export type LibraryCache = Map<string, ts.SourceFile>;

export function createAnalysisHost(
    options: ts.CompilerOptions,
    inventory: Inventory,
    libraries: LibraryCache,
): AnalysisHost {
    const host = ts.createCompilerHost(options, true);
    // As tsc does: JSDoc is parsed where it carries types (JavaScript files), not everywhere.
    host.jsDocParsingMode = ts.JSDocParsingMode.ParseForTypeErrors;
    const reads = new Map<string, ReadResult | { error: string }>();
    const libraryDir = canonical(toSlashes(dirname(ts.getDefaultLibFilePath(options)))) + "/";
    const original = host.getSourceFile.bind(host);

    host.getSourceFile = (fileName, languageVersionOrOptions, onError, shouldCreateNewSourceFile) => {
        const file = inventory.get(fileName);
        if (file) {
            try {
                const { text, sha256 } = Inventory.read(file.fileName);
                reads.set(file.path, { sha256 });
                return ts.createSourceFile(fileName, text, languageVersionOrOptions, true);
            } catch (error) {
                const message = error instanceof Error ? error.message : String(error);
                reads.set(file.path, { error: message });
                onError?.(message);
                return undefined;
            }
        }
        const key = canonical(toSlashes(fileName));
        if (key.startsWith(libraryDir)) {
            const version =
                typeof languageVersionOrOptions === "object"
                    ? `${languageVersionOrOptions.languageVersion}:${languageVersionOrOptions.impliedNodeFormat}`
                    : String(languageVersionOrOptions);
            const cacheKey = `${key}|${version}`;
            const cached = libraries.get(cacheKey);
            if (cached) return cached;
            const parsed = original(fileName, languageVersionOrOptions, onError, shouldCreateNewSourceFile);
            if (parsed) libraries.set(cacheKey, parsed);
            return parsed;
        }
        return original(fileName, languageVersionOrOptions, onError, shouldCreateNewSourceFile);
    };
    return { host, reads };
}
