// Messages between this analyzer and crates/adapters/codeatlas-semantic-typescript (ADR-0015).
//
// The adapter writes one Request as JSON to stdin and closes it. The analyzer writes one
// Message per line to stdout: first "hello", then "project", "problem", "file" and "failed"
// messages in any order, and last "done" (or "fatal" if it cannot go on). stderr is free-form
// text for people, never parsed. Change PROTOCOL_VERSION whenever a message changes shape.

export const PROTOCOL_VERSION = 1;

export interface Request {
    protocol: number;
    /** Absolute path of the repository root. */
    root: string;
    /** The files to analyse: repository-relative, with "/" separators. */
    files: string[];
    /** Stack of the analysis thread, in MiB (deep import chains recurse deeply). */
    stackSizeMb?: number;
    /** Heap limit of the analysis thread, in MiB; the thread stops cleanly beyond it. */
    maxMemoryMb?: number;
}

/** Zero-based [line, character]; characters are UTF-16 code units; line breaks are \n, \r\n and \r. */
export type WirePosition = [number, number];
/** [startLine, startCharacter, endLine, endCharacter], end exclusive. */
export type WireRange = [number, number, number, number];

export type UnresolvedReason = "not_found" | "ambiguous" | "dynamic" | "unsupported";

export type ModuleTarget =
    { kind: "file"; path: string } | { kind: "external" } | { kind: "unresolved"; reason: UnresolvedReason };

export interface ModuleFact {
    kind: "import" | "re_export";
    /** As written, without quotes; for a computed specifier, the expression's text. */
    specifier: string;
    range: WireRange;
    typeOnly: boolean;
    target: ModuleTarget;
}

/** A declaration, by its file and the start of its name. */
export interface DeclarationSite {
    path: string;
    at: WirePosition;
}

export type ReferenceTarget =
    | { kind: "declarations"; sites: DeclarationSite[] }
    | { kind: "unresolved"; specifier: string; reason: UnresolvedReason };

export interface ReferenceFact {
    kind: "call" | "reference" | "extends" | "implements";
    /** The name that was resolved. */
    range: WireRange;
    target: ReferenceTarget;
}

export type Message =
    | { type: "hello"; protocol: number; analyzer: string; typescript: string }
    /** A TypeScript program is being built; `config` is null for the inferred project. */
    | { type: "project"; config: string | null; files: number }
    /** A configuration problem. `path` and `range` are set when the file is inside the root. */
    | { type: "problem"; path: string | null; range: WireRange | null; message: string }
    | { type: "file"; path: string; sha256: string; modules: ModuleFact[]; references: ReferenceFact[] }
    | { type: "failed"; path: string; reason: "unreadable" | "analyzer_failed"; message: string }
    | { type: "done"; peakRssBytes: number }
    | { type: "fatal"; message: string };
