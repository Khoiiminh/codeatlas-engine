import type { Readable } from "node:stream";
import { readFile } from "node:fs/promises";

export enum Mode {
    Fast = "fast",
    Safe = "safe",
}

export interface Options<T extends object = {}> {
    readonly mode?: Mode;
    extra?: Partial<T>;
}

function sealed(target: Function): void {
    Object.seal(target);
}

@sealed
export abstract class Base<T> {
    #secret = 1;
    static count = 0;
    protected constructor(private readonly value: T) {}
    abstract describe(): string;
    get current(): T {
        return this.value;
    }
}

export function pick(value: string): string;
export function pick(value: number): number;
export function pick(value: string | number): string | number {
    return value;
}

export const config = { mode: Mode.Fast } satisfies Options;

export async function load(path: string, stream?: Readable): Promise<string | undefined> {
    const text = await readFile(path, "utf8");
    const first = text.split("\n")[0]?.trim() ?? "";
    type Key = keyof typeof config;
    const key: Key = "mode";
    return stream ? `${first}:${config[key]}` : (first as string);
}
