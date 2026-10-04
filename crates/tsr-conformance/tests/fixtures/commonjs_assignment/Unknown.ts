// @target: es2015
// @module: commonjs
// @allowJs: true
// @outDir: ./out/
// @filename: node.d.ts
declare function require(moduleName: string): any;
declare module "assert" {
    export function equal(actual: any, expected: any): void;
}
// @filename: ns.ts
/// <reference path="node.d.ts"/>
namespace receiver {
    export type label = "label";
}
var receiver = require("assert");
// @filename: app.js
exports.equal = receiver.equal;
exports.equal();
