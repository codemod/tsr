// @strict: true
// @noEmit: true
// @target: es2015
// @module: commonjs
// @filename: /payload.ts
export const available = 17;
export const alternate = "second";
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
// @filename: /usage.ts
import * as x from "./payload";
x.missing;
x["missing"];
x["availabl"];
x["available"];
import indexed = require("./indexed");
indexed["newKey"];
const object = { available: 1 };
object["available"];
