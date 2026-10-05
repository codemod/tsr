export {};
interface Required { tag: string; count: number }
class Empty {}
new <Required> Empty;
function nested() { var local: Required = {}; }
for (var iteration: Required = {} in {}) {}
var { tag }: Required = {};
function defaults(parameter: Required = {}) {}
class Holder { property: Required = {}; }
declare var ambient: Required = {};
var primitive: number = {};
