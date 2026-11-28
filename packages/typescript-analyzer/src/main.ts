// stdio entry point, started by crates/adapters/codeatlas-semantic-typescript (ADR-0015).
//
// It says hello, reads one request from stdin, and runs the analysis on a worker thread whose
// stack and heap it can size: the main thread's stack is fixed when Node starts. Messages go to
// stdout, one JSON object per line; anything for people goes to stderr. It never executes
// repository code or installs dependencies (ADR-0004).

import { createRequire } from "node:module";
import { Worker } from "node:worker_threads";
import { PROTOCOL_VERSION, type Message, type Request } from "./protocol/messages.ts";

/** Enough for the type checker on a 3,000-file import chain, with room to spare. */
const DEFAULT_STACK_MB = 256;

const require = createRequire(import.meta.url);
const typescriptVersion = (require("typescript/package.json") as { version: string }).version;
const analyzerVersion = (require("../package.json") as { version: string }).version;

function send(message: Message): void {
    process.stdout.write(JSON.stringify(message) + "\n");
}

function fatal(message: string): void {
    send({ type: "fatal", message });
    process.exitCode = 1;
}

function describe(error: unknown): string {
    return error instanceof Error ? `${error.name}: ${error.message}` : String(error);
}

send({ type: "hello", protocol: PROTOCOL_VERSION, analyzer: analyzerVersion, typescript: typescriptVersion });

const chunks: Buffer[] = [];
for await (const chunk of process.stdin) chunks.push(chunk as Buffer);

let request: Request | undefined;
try {
    request = JSON.parse(Buffer.concat(chunks).toString("utf8")) as Request;
} catch (error) {
    fatal(`the request is not valid JSON: ${describe(error)}`);
}

if (request && request.protocol !== PROTOCOL_VERSION) {
    fatal(`the request uses protocol ${request.protocol}; this analyzer speaks ${PROTOCOL_VERSION}`);
} else if (request) {
    let done = false;
    const worker = new Worker(new URL("./analysis/worker.ts", import.meta.url), {
        workerData: request,
        resourceLimits: {
            stackSizeMb: request.stackSizeMb ?? DEFAULT_STACK_MB,
            ...(request.maxMemoryMb ? { maxOldGenerationSizeMb: request.maxMemoryMb } : {}),
        },
    });
    worker.on("message", (message: Message) => {
        if (message.type === "done") done = true;
        send(message);
    });
    worker.on("error", (error) => fatal(`the analysis stopped: ${describe(error)}`));
    worker.on("exit", (code) => {
        if (!done && process.exitCode !== 1) fatal(`the analysis thread exited with code ${code} before it finished`);
    });
}
