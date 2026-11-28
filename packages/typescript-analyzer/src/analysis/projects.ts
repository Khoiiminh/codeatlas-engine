// Which TypeScript project owns each scanned file (ADR-0015).
//
// A file belongs to the nearest tsconfig.json (or jsconfig.json) above it, inside the root,
// that includes it, as in an editor. A configuration that does not include the file is followed
// through its project references (a "solution" tsconfig.json with "files": [] and references to
// tsconfig.app.json and tsconfig.node.json, for example), then the search goes on upwards.
// Files that no configuration includes belong to one inferred project with the defaults below.
// References are used only to find owners: imports resolve to the referenced project's source,
// as an editor does, not to build output that may not exist.

import { basename, dirname } from "node:path";
import ts from "typescript";
import type { Message, WireRange } from "../protocol/messages.ts";
import { type Inventory, type InventoryFile, canonical, toSlashes } from "./inventory.ts";
import { LineMap } from "./positions.ts";

export interface Project {
    /** Absolute path of the configuration, or null for the inferred project. */
    configFile: string | null;
    options: ts.CompilerOptions;
    /** The program's root files: everything the configuration includes. */
    rootNames: string[];
    /** The scanned files this project analyses, in path order. */
    owned: InventoryFile[];
}

/** Options for files that no configuration includes: JavaScript allowed, as editors infer. */
export const INFERRED_OPTIONS: ts.CompilerOptions = {
    allowJs: true,
    checkJs: false,
    jsx: ts.JsxEmit.Preserve,
    module: ts.ModuleKind.ESNext,
    moduleResolution: ts.ModuleResolutionKind.Bundler,
    target: ts.ScriptTarget.ESNext,
    allowImportingTsExtensions: true,
    allowSyntheticDefaultImports: true,
    esModuleInterop: true,
    resolveJsonModule: false,
    skipLibCheck: true,
    noEmit: true,
};

/** What tsserver assumes for a jsconfig.json before reading it. */
const JSCONFIG_DEFAULTS: ts.CompilerOptions = {
    allowJs: true,
    maxNodeModuleJsDepth: 2,
    allowSyntheticDefaultImports: true,
    skipLibCheck: true,
    noEmit: true,
};

const CONFIG_NAMES = ["tsconfig.json", "jsconfig.json"];
/** "No inputs were found": expected of a solution configuration with "files": []. */
const NO_INPUTS = 18003;

interface Loaded {
    configFile: string;
    options: ts.CompilerOptions;
    fileNames: string[];
    fileSet: Set<string>;
    references: string[];
}

export function assignProjects(inventory: Inventory, send: (m: Message) => void): Project[] {
    const loaded = new Map<string, Loaded | null>();
    const configsIn = new Map<string, string[]>();
    const owners = new Map<string, { loaded: Loaded; owned: InventoryFile[] }>();
    const inferred: InventoryFile[] = [];
    const rootKey = canonical(inventory.root);

    const load = (configFile: string): Loaded | null => {
        const key = canonical(configFile);
        if (loaded.has(key)) return loaded.get(key)!;
        loaded.set(key, null);
        const result = parseConfig(configFile, inventory, send);
        loaded.set(key, result);
        return result;
    };

    const configsOf = (dir: string): string[] => {
        const key = canonical(dir);
        let found = configsIn.get(key);
        if (!found) {
            found = CONFIG_NAMES.map((name) => `${dir}/${name}`).filter((f) => ts.sys.fileExists(f));
            configsIn.set(key, found);
        }
        return found;
    };

    const containing = (config: Loaded, file: string, visited: Set<string>): Loaded | undefined => {
        if (visited.has(config.configFile)) return undefined;
        visited.add(config.configFile);
        if (config.fileSet.has(file)) return config;
        for (const reference of config.references) {
            const referenced = ts.sys.fileExists(reference) ? load(reference) : null;
            const owner = referenced && containing(referenced, file, visited);
            if (owner) return owner;
        }
        return undefined;
    };

    for (const file of inventory.files) {
        const key = canonical(file.fileName);
        let owner: Loaded | undefined;
        let dir = dirname(file.fileName);
        for (;;) {
            for (const configFile of configsOf(dir)) {
                const config = load(configFile);
                owner = config ? containing(config, key, new Set()) : undefined;
                if (owner) break;
            }
            if (owner || canonical(dir) === rootKey) break;
            const parent = dirname(dir);
            if (parent === dir) break;
            dir = parent;
        }
        if (!owner) {
            inferred.push(file);
            continue;
        }
        const entry = owners.get(owner.configFile) ?? { loaded: owner, owned: [] };
        entry.owned.push(file);
        owners.set(owner.configFile, entry);
    }

    const projects: Project[] = [...owners.values()]
        .sort((a, b) => (canonical(a.loaded.configFile) < canonical(b.loaded.configFile) ? -1 : 1))
        .map(({ loaded, owned }) => ({
            configFile: loaded.configFile,
            options: loaded.options,
            rootNames: loaded.fileNames,
            owned,
        }));
    if (inferred.length > 0) {
        projects.push({
            configFile: null,
            options: INFERRED_OPTIONS,
            rootNames: inferred.map((f) => f.fileName),
            owned: inferred,
        });
    }
    return projects;
}

function parseConfig(configFile: string, inventory: Inventory, send: (m: Message) => void): Loaded | null {
    const json = ts.readJsonConfigFile(configFile, (p) => ts.sys.readFile(p));
    const parsed = ts.parseJsonSourceFileConfigFileContent(
        json,
        ts.sys,
        dirname(configFile),
        basename(configFile) === "jsconfig.json" ? JSCONFIG_DEFAULTS : {},
        configFile,
    );
    const problems = [...((json as { parseDiagnostics?: ts.Diagnostic[] }).parseDiagnostics ?? []), ...parsed.errors];
    for (const d of problems.filter((d) => d.code !== NO_INPUTS)) send(problemOf(d, configFile, inventory));
    return {
        configFile,
        // The analysis never emits; output settings must not stop a program from being built.
        options: { ...parsed.options, noEmit: true },
        fileNames: parsed.fileNames,
        fileSet: new Set(parsed.fileNames.map((f) => canonical(toSlashes(f)))),
        references: (parsed.projectReferences ?? []).map((r) => toSlashes(ts.resolveProjectReferencePath(r))),
    };
}

function problemOf(d: ts.Diagnostic, configFile: string, inventory: Inventory): Message {
    const text = ts.flattenDiagnosticMessageText(d.messageText, " ");
    const file = d.file?.fileName ?? configFile;
    const path = inventory.relative(file) ?? null;
    let range: WireRange | null = null;
    if (path !== null && d.file && d.start !== undefined) {
        const lines = new LineMap(d.file.text);
        range = [...lines.position(d.start), ...lines.position(d.start + (d.length ?? 0))];
    }
    const where = path ?? toSlashes(file);
    return { type: "problem", path, range, message: `${where}: ${text} (TS${d.code})` };
}
