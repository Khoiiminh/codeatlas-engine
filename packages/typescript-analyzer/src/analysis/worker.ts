// The analysis thread. main.ts starts it with a large stack, because the type checker recurses
// once per file along an import chain (a 3,000-file chain overflows the default stack).

import { parentPort, workerData } from "node:worker_threads";
import type { Message, Request } from "../protocol/messages.ts";
import { analyze } from "./analyze.ts";

const port = parentPort!;
analyze(workerData as Request, (message: Message) => port.postMessage(message));
