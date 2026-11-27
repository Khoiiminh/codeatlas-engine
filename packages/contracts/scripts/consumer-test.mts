// FND-03 consumer package-install test.
//
// Packs @codeatlas/contracts exactly as it is shipped, installs the tarball into a throw-away
// consumer project, and proves that a consumer can:
//   1. type-check imports from both an ES module and a CommonJS module (TypeScript node16 rules,
//      the same rules codeatlas-vscode compiles with);
//   2. load the runtime constants with `import` and with `require`;
//   3. read the bundled JSON Schema, which matches the constants.
//
//   node scripts/consumer-test.mts [--keep]   (--keep leaves the temp folder for inspection)

import { execFileSync } from "node:child_process";
import { mkdtempSync, writeFileSync, rmSync, readdirSync, mkdirSync } from "node:fs";
import { tmpdir } from "node:os";
import { join, resolve } from "node:path";

const pkg = resolve(import.meta.dirname, "..");
const keep = process.argv.includes("--keep");
const work = mkdtempSync(join(tmpdir(), "codeatlas-contracts-consumer-"));
// Run npm through its own CLI script (set by `npm run`), so no shell is needed on any platform.
const npmCli = process.env.npm_execpath;

function run(cmd: string, args: string[], cwd: string): string {
    if (cmd === "npm") {
        if (!npmCli?.endsWith(".js")) throw new Error("run this script with `npm run test:consumer`");
        return execFileSync(process.execPath, [npmCli, ...args], {
            cwd,
            encoding: "utf8",
            stdio: ["ignore", "pipe", "pipe"],
        });
    }
    return execFileSync(cmd, args, { cwd, encoding: "utf8", stdio: ["ignore", "pipe", "pipe"] });
}

try {
    // 1. Pack (runs `prepack` -> build) into the temp folder.
    const packDir = join(work, "pack");
    mkdirSync(packDir);
    run("npm", ["pack", "--pack-destination", packDir, "--silent"], pkg);
    const tarball = readdirSync(packDir).find((f) => f.endsWith(".tgz"));
    if (!tarball) throw new Error("npm pack produced no tarball");
    console.log(`packed ${tarball}`);

    // 2. A consumer project that installs only the tarball (no registry access needed).
    const consumer = join(work, "consumer");
    mkdirSync(consumer);
    writeFileSync(
        join(consumer, "package.json"),
        JSON.stringify({ name: "contracts-consumer", private: true, type: "module" }, null, 2),
    );
    run("npm", ["install", "--no-audit", "--no-fund", "--ignore-scripts", join(packDir, tarball)], consumer);

    // 3. Type-check as ESM and as CommonJS.
    writeFileSync(
        join(consumer, "tsconfig.json"),
        JSON.stringify(
            {
                compilerOptions: {
                    module: "node16",
                    moduleResolution: "node16",
                    target: "es2022",
                    strict: true,
                    noEmit: true,
                    types: [],
                    skipLibCheck: false,
                },
                include: ["*.mts", "*.cts"],
            },
            null,
            2,
        ),
    );
    const usage = `
const node: Node = { id: "n1", kind: "function", name: "start" };
const edge: Edge = {
  id: "e1", kind: "calls", source: "n1", target: "n2", resolution: "resolved",
  evidence: [{ kind: "semantic", analyzer: "typescript", location: { path: "a.ts", range: { start: { line: 0, character: 0 }, end: { line: 0, character: 5 } } } }],
};
const init: InitializeParams = { client: { name: "consumer", version: "0.0.0" }, protocolVersions: [PROTOCOL_VERSION] };
const method: RequestMethod = Method.INITIALIZE;
const code: number = ErrorCode.UNSUPPORTED_PROTOCOL_VERSION;
const ok: boolean = isProtocolCompatible(PROTOCOL_VERSION, "0.1");
// @ts-expect-error unknown node kinds must not type-check
const bad: Node = { id: "n2", kind: "lambda", name: "x" };
export { node, edge, init, method, code, ok, bad, CONTRACT_VERSION };
`;
    writeFileSync(
        join(consumer, "esm-check.mts"),
        `import { CONTRACT_VERSION, PROTOCOL_VERSION, Method, ErrorCode, isProtocolCompatible } from "@codeatlas/contracts";\nimport type { Node, Edge, InitializeParams, RequestMethod } from "@codeatlas/contracts";\n${usage}`,
    );
    writeFileSync(
        join(consumer, "cjs-check.cts"),
        `import { CONTRACT_VERSION, PROTOCOL_VERSION, Method, ErrorCode, isProtocolCompatible } from "@codeatlas/contracts";\nimport type { Node, Edge, InitializeParams, RequestMethod } from "@codeatlas/contracts";\n${usage}`,
    );
    run(process.execPath, [join(pkg, "node_modules/typescript/bin/tsc"), "-p", "tsconfig.json"], consumer);
    console.log("type-check passed (ESM and CommonJS, node16 resolution)");

    // 4. Runtime: import, require, and the bundled schema.
    writeFileSync(
        join(consumer, "runtime.mjs"),
        `
import assert from "node:assert/strict";
import { createRequire } from "node:module";
import { readFileSync } from "node:fs";
import * as esm from "@codeatlas/contracts";
const require = createRequire(import.meta.url);
const cjs = require("@codeatlas/contracts");
const schema = JSON.parse(readFileSync(require.resolve("@codeatlas/contracts/schemas/codeatlas.schema.json"), "utf8"));
for (const mod of [esm, cjs]) {
  assert.equal(mod.CONTRACT_VERSION, schema["x-contract-version"]);
  assert.equal(mod.PROTOCOL_VERSION, schema["x-protocol-version"]);
  assert.equal(mod.Method.GRAPH_QUERY, "graph/query");
  assert.equal(mod.ErrorCode.REQUEST_CANCELLED, -32800);
  assert.equal(mod.isProtocolCompatible("0.1", "0.2"), false);
  assert.equal(mod.isProtocolCompatible("1.0", "1.3"), true);
  assert.equal(mod.protocolCompatibilityKey("01.0"), undefined);
}
assert.ok(schema.$defs.Edge && schema.$defs.InitializeParams);
console.log("runtime check passed (import, require, bundled schema " + schema["x-contract-version"] + ")");
`,
    );
    process.stdout.write(run(process.execPath, ["runtime.mjs"], consumer));
    console.log("consumer package-install test passed");
} catch (error) {
    const e = error as { stdout?: string; stderr?: string; message: string };
    console.error(e.stdout ?? "", e.stderr ?? "", e.message);
    process.exitCode = 1;
} finally {
    if (keep) console.log(`kept ${work}`);
    else rmSync(work, { recursive: true, force: true });
}
