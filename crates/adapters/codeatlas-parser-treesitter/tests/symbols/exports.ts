import { external } from "./elsewhere";

const a = 1;
function b(): void {}
class C {}
let d = 2, e = 3;
interface F {}
type G = string;
const hidden = 0;

export { a, b as renamed, C, type F };
export type { G };
export { external };
export { reexported } from "./elsewhere";
export * from "./elsewhere";
export * as Everything from "./elsewhere";

export default d;

export function overloaded(value: string): string;
export function overloaded(value: number): string;
export function overloaded(value: unknown): string {
    return String(value);
}

export const { first, second: renamedSecond, ...others } = { first: 1, second: 2, third: 3 };
export const [x0, , x2 = 2, ...xs] = [0, 1, 2, 3];
const { nested: { deep }, list: [item] } = { nested: { deep: 1 }, list: [1] };

export const Status = { On: "on", Off: "off" } as const;
export type Status = (typeof Status)[keyof typeof Status];
