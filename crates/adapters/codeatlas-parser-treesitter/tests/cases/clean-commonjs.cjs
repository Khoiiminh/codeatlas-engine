"use strict";
const path = require("node:path");

module.exports = function resolve(...parts) {
    return path.join(__dirname, ...parts);
};
