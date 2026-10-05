export {};
interface Required { tag: string; count: number }
const unrelatedError = ;
function noColon(parameter Required = {}) { var local: Required = {}; }
function noEquals(parameter: Required {}) { var local: Required = {}; }
function noAnnotation(parameter: = {}) { var local: Required = {}; }
function noName(: Required = {}) { var local: Required = {}; }
function optional(parameter?: Required = {}) { var local: Required = {}; }
function rest(...parameter: Required = {}) { var local: Required = {}; }
function pattern({ tag }: Required = {}) { var local: Required = {}; }
function generic<T>(parameter: Required = {}) { var local: Required = {}; }
function named(parameter: Required = function named() {}) { var local: Required = {}; }
function member(parameter: Required = new globalThis.Object()) { var local: Required = {}; }
function escaped(param\u0065ter: Required = {}) { var local: Required = {}; }
