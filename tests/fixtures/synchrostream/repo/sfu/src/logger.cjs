// CommonJS module; the ESM server imports it through Node's CommonJS interop.
const os = require("node:os");

function createLogger(scope) {
    return {
        info(message) {
            console.log(`[${os.hostname()}] ${scope}: ${message}`);
        },
    };
}

function loadTransport(name) {
    return require(name);
}

module.exports = { createLogger, loadTransport };
