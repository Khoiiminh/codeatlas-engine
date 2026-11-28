import { greet } from "@mono/shared";
import { pad } from "@/lib/format";
import { AuditLogger } from "./audit-logger";

export function run(): string {
    const logger = new AuditLogger("app");
    return logger.log(pad(greet("atlas")));
}
