const path = require("node:path");

function createThing(name) {
    return { name, base: path.basename(name) };
}

class Registry {
    items = [];

    add(item) {
        this.items.push(item);
    }
}

const notExported = 1;

module.exports = { createThing, Registry: Registry, version: "1.0" };
