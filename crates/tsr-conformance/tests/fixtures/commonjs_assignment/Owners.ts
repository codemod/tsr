// @strict: true
// @noImplicitAny: false
// @allowJs: true
// @checkJs: true
// @module: commonjs
// @noEmit: true
// @filename: /named.js
exports.item = undefined;
exports.item = 1;
exports.item = "ready";
module["exports"].extra = 7;
// @filename: /whole.js
module.exports = 3;
module.exports;
exports;
// @filename: /element.js
module["exports"] = 4;
module["exports"];
// @filename: /use.js
const named = require("./named");
named.item;
named.extra;
const whole = require("./whole");
const element = require(`./element`);
/** @type {any} */
const annotated = require("./named");
annotated.item;
whole;
element;
function shadow() {
    function require(path) { return { item: "local" }; }
    const named = require("./named");
    return named.item;
}
shadow();
// @filename: /validate.ts
import "./use";
import named = require("./named");
import whole = require("./whole");
import element = require("./element");
named.item;
named.extra;
whole;
element;
declare function require(path: string): { item: boolean };
const ordinary = require("./named");
ordinary.item;
