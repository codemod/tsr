// @strict: false
// @noEmit: true
// @target: es2015
// @module: commonjs
// @filename: /payload.ts
export const available = 17;
export const alternate = "second";
// @filename: /usage.ts
import * as x from "./payload";
x.missing;
x["missing"];
x["availabl"];
x["available"];
