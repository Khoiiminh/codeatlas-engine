import { createServer } from "node:http";
import { createLogger, loadTransport } from "./logger.cjs";
import { createRouter } from "./router.js";
import defaults from "./defaults.json" with { type: "json" };

const logger = createLogger("server");

function handleJoin(message) {
    return { ok: true, room: message.room };
}

const route = createRouter({ join: handleJoin });

export function start(port = defaults.port) {
    const transport = loadTransport(defaults.transport);
    const server = createServer((request, response) => {
        response.end(JSON.stringify(route({ type: request.url?.slice(1) ?? "" })));
    });
    server.listen(port, () => logger.info(`listening on ${port} via ${transport}`));
    return server;
}

start();
