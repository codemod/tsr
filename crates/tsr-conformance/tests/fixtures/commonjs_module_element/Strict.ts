// @strict: true
// @allowJs: true
// @checkJs: true
// @noEmit: true
// @target: es2015
// @module: commonjs
// @filename: /payload.js
exports.available = 1;
exports.alternate = 2;
// @filename: /indexed.ts
const dictionary: { [key: string]: number } = { present: 5 };
export = dictionary;
// @filename: /classValue.ts
class Value { static count = 3; }
export = Value;
// @filename: /clone.ts
import * as clone from "./classValue";
clone.count;
clone["count"];
// @filename: /usage.js
const x = require("./payload");
x.missing;
x["missing"];
x["availabl"];
x["available"];
const indexed = require("./indexed");
indexed["newKey"];
const object = { available: 1 };
object["available"];
