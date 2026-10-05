export {};
interface Required { tag: string; count: number }
class Empty {}
const unrelatedError = ;
function local() {
    var object: Required = {};
    var callable: Required = function () {};
    var constructed: Required = new Empty();
    var bare: Required = new Empty;
}
function objectDefault(parameter: Required = {}) {}
function functionDefault(parameter: Required = function () {}) {}
function newDefault(parameter: Required = new Empty()) {}
function bareDefault(parameter: Required = new Empty) {}
