import { Logger } from "@mono/shared";

export class AuditLogger extends Logger {
    override log(message: string): string {
        return super.log(`audit ${message}`, "warn");
    }
}
