// One analysis run: assign files to projects, then build one program per project, analyse the
// files it owns, and let it go before the next one, so memory is bounded by the largest
// project rather than by the repository (ADR-0014, ADR-0015).

import ts from "typescript";
import type { Message, Request } from "../protocol/messages.ts";
import { createAnalysisHost, type LibraryCache } from "./host.ts";
import { Inventory } from "./inventory.ts";
import { createModuleResolver } from "./modules.ts";
import { type Project, assignProjects } from "./projects.ts";
import { ReferenceAnalyzer } from "./references.ts";

export function analyze(request: Request, send: (message: Message) => void): void {
    const inventory = new Inventory(request.root, request.files);
    const libraries: LibraryCache = new Map();
    for (const project of assignProjects(inventory, send)) {
        send({
            type: "project",
            config: project.configFile === null ? null : (inventory.relative(project.configFile) ?? project.configFile),
            files: project.owned.length,
        });
        analyzeProject(project, inventory, libraries, send);
    }
    // maxRSS is in kibibytes and covers the whole process.
    send({ type: "done", peakRssBytes: process.resourceUsage().maxRSS * 1024 });
}

function analyzeProject(
    project: Project,
    inventory: Inventory,
    libraries: LibraryCache,
    send: (m: Message) => void,
): void {
    let next = 0;
    try {
        const { host, reads } = createAnalysisHost(project.options, inventory, libraries);
        const program = ts.createProgram({ rootNames: project.rootNames, options: project.options, host });
        const analyzer = new ReferenceAnalyzer(
            program.getTypeChecker(),
            inventory,
            createModuleResolver(program, project.options, host, inventory),
        );
        for (; next < project.owned.length; next++) {
            const file = project.owned[next]!;
            const sf = program.getSourceFile(file.fileName);
            const read = reads.get(file.path);
            if (!sf || !read || "error" in read) {
                send({
                    type: "failed",
                    path: file.path,
                    reason: "unreadable",
                    message: read && "error" in read ? read.error : "the compiler did not load this file",
                });
            } else {
                const facts = analyzer.analyze(sf);
                send({ type: "file", path: file.path, sha256: read.sha256, ...facts });
            }
        }
    } catch (error) {
        // After an exception the checker's state is unknown, so nothing else in this project is
        // trusted; the next project gets a new program.
        const pending = project.owned.slice(next);
        const message = `TypeScript failed in ${pending[0]?.path ?? "the project"}: ${describe(error)}`;
        process.stderr.write(`${message}\n${error instanceof Error ? error.stack : ""}\n`);
        for (const file of pending) send({ type: "failed", path: file.path, reason: "analyzer_failed", message });
    }
}

export function describe(error: unknown): string {
    if (error instanceof Error) return `${error.name}: ${error.message}`;
    return String(error);
}
