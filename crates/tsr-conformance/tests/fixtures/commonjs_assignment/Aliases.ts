// @strict: true
// @noImplicitAny: false
// @allowJs: true
// @checkJs: true
// @module: commonjs
// @noEmit: true
// @filename: /aliases.js
let value = 1;
exports.scalar = value;
value = 2;
{
    const value = "shadow";
    exports.shadow = value;
}
const owner = { flag: true };
exports.member = owner.flag;
exports.scalar;
exports.shadow;
exports.member;
// @filename: /klass.js
module.exports = class Bound { field = 9; };
module.exports;
// @filename: /writes.js
exports.first = exports.second = void 0;
exports.first = 1;
exports.second = "ready";
/** @type {number} */
exports.typed = 1;
exports.typed = undefined;
exports.typed = "bad";
const ordinary = { typed: 1 };
ordinary.typed = undefined;
/** @param {{typed: number}} exports */
function shadow(exports) {
    exports.typed = undefined;
}
shadow(ordinary);
// @filename: /use.ts
import "./writes";
import aliases = require("./aliases");
import Bound = require("./klass");
aliases.scalar;
aliases.shadow;
aliases.member;
const instance: Bound = new Bound();
instance.field;
