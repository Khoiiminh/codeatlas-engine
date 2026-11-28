function parse(text) {
    return text.trim();
}

function format(value) {
    return String(value);
}

function unused() {}

exports.parse = parse;
module.exports.format = format;
