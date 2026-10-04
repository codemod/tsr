// @strict: false
// @allowJs: true
// @checkJs: true
// @noEmit: true
// @target: es2015
// @module: commonjs
// @filename: /payload.js
exports.available = 1;
exports.alternate = 2;
// @filename: /usage.js
const x = require("./payload");
x.missing;
x["missing"];
x["availabl"];
x["available"];
