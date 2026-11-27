// @codeatlas/contracts: canonical CodeAtlas contract types (FND-03, ADR-0009).
//
// Everything here comes from contracts/schemas/codeatlas.schema.json in codeatlas-engine:
// types and constants are generated (src/generated), and the schema itself ships at
// `@codeatlas/contracts/schemas/codeatlas.schema.json`. Consumers use these types only in
// their adapter layers and map them to their own core types (ADR-0010/0011/0012).
//
// Reading rule: ignore unknown properties. A newer engine may add fields (contracts/README.md).

export type * from "./generated/contracts.js";
export { CONTRACT_VERSION, PROTOCOL_VERSION, Method, ErrorCode } from "./generated/constants.js";

/**
 * Compatibility key of a `MAJOR.MINOR` protocol version: `MAJOR` when MAJOR >= 1, `MAJOR.MINOR`
 * while MAJOR is 0. Returns `undefined` for a malformed version. Same rule as the engine's
 * `codeatlas_protocol::version::is_protocol_compatible`.
 */
export function protocolCompatibilityKey(version: string): string | undefined {
    const match = /^(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)$/.exec(version);
    if (!match) return undefined;
    const [, major, minor] = match;
    return major === "0" ? `0.${minor}` : major;
}

/** True when two protocol versions can talk to each other. */
export function isProtocolCompatible(a: string, b: string): boolean {
    const key = protocolCompatibilityKey(a);
    return key !== undefined && key === protocolCompatibilityKey(b);
}
