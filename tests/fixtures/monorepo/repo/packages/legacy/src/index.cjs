// A CommonJS package, analysed through its jsconfig.json.
const dates = require("./dates.cjs");

function report(date) {
    return `report for ${dates.formatDate(date)}`;
}

module.exports = { report };
