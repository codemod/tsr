// @strict: true
// @target: es2020
// @module: commonjs
// @noEmit: true
// @filename: /literal.ts
export = (13);
// @filename: /object.ts
export = ({ value: "fresh", run: function() { return "done"; } });
// @filename: /function.ts
export = function() { return 1; };
// @filename: /parenthesized.ts
export = (function() { return true; });
// @filename: /symbol.ts
const original = Symbol();
export = (original);
// @filename: /symbolAlias.ts
const original = Symbol();
export = original;
// @filename: /use.ts
import literal = require("./literal");
import object = require("./object");
import fn = require("./function");
import parenthesized = require("./parenthesized");
import symbol = require("./symbol");
import alias = require("./symbolAlias");
literal;
object.value;
object.run();
fn();
parenthesized();
symbol;
alias;
