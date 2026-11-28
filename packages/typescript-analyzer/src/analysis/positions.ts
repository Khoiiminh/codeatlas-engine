// Positions in the engine's units (codeatlas-core::text): zero-based lines, characters in UTF-16
// code units (a JavaScript string index already is one), and only \n, \r\n and \r as line
// breaks. TypeScript also breaks lines at U+2028 and U+2029, so its own line map is not used.

import type { WirePosition } from "../protocol/messages.ts";

export class LineMap {
    private readonly starts: number[] = [0];

    constructor(text: string) {
        for (let i = 0; i < text.length; i++) {
            const c = text.charCodeAt(i);
            if (c === 10) {
                this.starts.push(i + 1);
            } else if (c === 13) {
                if (text.charCodeAt(i + 1) === 10) i++;
                this.starts.push(i + 1);
            }
        }
    }

    position(offset: number): WirePosition {
        let low = 0;
        let high = this.starts.length - 1;
        while (low < high) {
            const mid = (low + high + 1) >> 1;
            if (this.starts[mid]! <= offset) low = mid;
            else high = mid - 1;
        }
        return [low, offset - this.starts[low]!];
    }
}
