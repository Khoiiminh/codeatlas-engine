export type Level = "info" | "warn";

export class Logger {
    constructor(readonly scope: string) {}

    log(message: string, level: Level = "info"): string {
        return `[${level}] ${this.scope}: ${message}`;
    }
}
