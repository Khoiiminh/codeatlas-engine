export function outer(): () => void {
    function inner(): void {}
    const local = 1;
    class LocalClass {
        method(): void {}
    }
    return () => inner();
}

if (Math.random() > 0.5) {
    var hoisted = 1;
}

for (let i = 0; i < 1; i++) {}

export const Expression = class {
    method(): void {}
};

export const object = {
    method() {},
    property: 1,
    get getter() {
        return 1;
    },
};

export const arrow = (value: number) => value;

(function iife() {})();

enum Color {
    Red,
    Green,
}

const enum Direction {
    Up = 1,
}
