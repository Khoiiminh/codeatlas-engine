export namespace Geometry {
    export const unit = 1;
    export function area(side: number): number {
        return side * side;
    }
    class Helper {
        assist(): void {}
    }
    export namespace Inner {
        export type Id = string;
    }
}

namespace Outer.Middle.Last {
    export function deep(): void {}
}

declare namespace Ambient {
    function described(): void;
    let counter: number;
}

function merged(): void {}
namespace merged {
    export const extra = 1;
}

declare module "external-module" {
    export function fromElsewhere(): void;
}

declare global {
    interface Window {
        injected: string;
    }
}
