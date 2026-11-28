// The files the engine scanned, and how compiler file names map back to repository paths.

import { createHash } from "node:crypto";
import { readFileSync, realpathSync } from "node:fs";
import ts from "typescript";

export interface InventoryFile {
    /** Repository-relative, "/" separators. */
    path: string;
    /** Absolute, "/" separators, as the compiler names it. */
    fileName: string;
}

export interface FileContent {
    text: string;
    sha256: string;
}

const canonical = (fileName: string): string => (ts.sys.useCaseSensitiveFileNames ? fileName : fileName.toLowerCase());

export const toSlashes = (p: string): string => p.replace(/\\/g, "/");

export class Inventory {
    /** Absolute real path of the root, "/" separators, no trailing "/". */
    readonly root: string;
    readonly files: InventoryFile[];
    private readonly byKey = new Map<string, InventoryFile>();
    private readonly rootPrefix: string;

    constructor(root: string, paths: string[]) {
        // The real path: TypeScript resolves symbolic links in what it reports (macOS temporary
        // folders are under a link, for example), so every name is compared in real form.
        const real = toSlashes(realpathSync.native(root));
        const prefix = real.endsWith("/") ? real : real + "/";
        this.root = prefix.length > 1 && !/^[A-Za-z]:\/$/.test(prefix) ? prefix.slice(0, -1) : prefix;
        this.rootPrefix = canonical(prefix);
        this.files = [...paths].sort().map((path) => ({ path, fileName: prefix + path }));
        for (const file of this.files) this.byKey.set(canonical(file.fileName), file);
    }

    /** The inventory file a compiler file name refers to. */
    get(fileName: string): InventoryFile | undefined {
        return this.byKey.get(canonical(toSlashes(fileName)));
    }

    /** The repository-relative path of any file under the root, inventory or not. */
    relative(fileName: string): string | undefined {
        const name = toSlashes(fileName);
        return canonical(name).startsWith(this.rootPrefix) ? name.slice(this.rootPrefix.length) : undefined;
    }

    /** Reads a file as the scanner did: its SHA-256 is of the bytes on disk; the text has no BOM. */
    static read(fileName: string): FileContent {
        const bytes = readFileSync(fileName);
        const sha256 = createHash("sha256").update(bytes).digest("hex");
        let text = bytes.toString("utf8");
        if (text.charCodeAt(0) === 0xfeff) text = text.slice(1);
        return { text, sha256 };
    }
}

export { canonical };
