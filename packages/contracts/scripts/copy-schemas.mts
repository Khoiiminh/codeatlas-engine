// Copies the canonical schema bundle into the package so consumers get exactly the schema
// the types were generated from (`@codeatlas/contracts/schemas/codeatlas.schema.json`).

import { copyFileSync, mkdirSync } from "node:fs";
import { join, resolve } from "node:path";

const pkg = resolve(import.meta.dirname, "..");
const source = resolve(pkg, "../../contracts/schemas/codeatlas.schema.json");
mkdirSync(join(pkg, "schemas"), { recursive: true });
copyFileSync(source, join(pkg, "schemas/codeatlas.schema.json"));
console.log("copied contracts/schemas/codeatlas.schema.json -> packages/contracts/schemas/");
