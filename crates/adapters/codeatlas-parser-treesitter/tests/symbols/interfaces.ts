export interface Shape {
    readonly id: string;
    area(): number;
    scale?(factor: number): Shape,
    get label(): string;
    set label(value: string);
    (input: string): Shape;
    new (input: string): Shape;
    [key: string]: unknown;
    "quoted-key": number;
}

export interface Shape {
    color: string;
}

interface Circle extends Shape {
    radius: number
}

export class Square {
    side = 1;
}

export interface Square {
    corners: number;
}

type Point = { x: number; y: number };
export type Pair<T> = [T, T];

interface Widget {
    size: number;
}

class Widget {
    name = "widget";
}
