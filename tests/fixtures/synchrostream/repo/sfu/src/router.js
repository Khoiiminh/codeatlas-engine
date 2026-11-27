import { createLogger } from "./logger.cjs";

const logger = createLogger("router");

export function createRouter(handlers) {
    return function route(message) {
        const handler = handlers[message.type];
        if (!handler) {
            logger.info(`no handler for ${message.type}`);
            return undefined;
        }
        return handler(message);
    };
}
