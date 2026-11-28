import { Inject, Injectable } from "./decorators";

@Injectable()
export class Service {
    static instances = 0;
    #secret = "s";
    readonly name: string = "service";
    declare tag: string;
    accessor count = 0;
    optional?: number;
    [key: string]: unknown;

    static {
        Service.instances = 0;
    }

    constructor(
        @Inject("token") private readonly token: string,
        public retries: number,
        plain: number,
        protected readonly clock?: Date,
    ) {}

    @Log()
    run(): void {}

    get size(): number {
        return 1;
    }

    set size(value: number) {}

    format(value: string): string;
    format(value: number): string;
    format(value: unknown): string {
        return String(value);
    }

    #hidden(): void {}

    static create(): Service {
        return new Service("t", 1, 0);
    }

    ["computed"](): void {}
    "quoted"(): void {}
    42(): void {}
}

export abstract class Shape {
    abstract area(): number;
    protected abstract readonly sides: number;
}

class Base {
    constructor(value: number) {}
}

class Derived extends Base {
    constructor(override readonly value: number) {
        super(value);
    }
}

function Log() {
    return (..._args: unknown[]) => undefined;
}
