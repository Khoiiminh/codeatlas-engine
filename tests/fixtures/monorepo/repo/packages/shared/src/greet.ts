import { pad } from "./internal/format";

export function greet(name: string): string {
    return pad(`hi ${name}`);
}
