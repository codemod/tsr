interface Array<T> { [index: number]: T; length: number }
interface ReadonlyArray<T> { readonly [index: number]: T; readonly length: number }
interface Iterable<T, R = void, N = undefined> { yielded?: T }
type Hue = "indigo" | "ochre";
declare const hue: Hue;
declare function measured(): number;
function objectDefaults({ word = "hello", amount = measured(), shade = hue }) {}
function arrayDefaults([first = "one", required, trailing = 9]) {}
function omitted([, middle = true, last]) {}
function nested({ box: { child = "inner" }, lane: [left = 3, right] }) {}
function objectRest({ head = 2, ...objectTail }) {}
function arrayRest([start = 5, ...arrayTail]) {}
function literalKeys({ ["named"]: named = false, 17: numeric = "n" }) {}
function nestedArrayDefault({ row: [position, padded = 7] = ["yes"] }) {}
function nestedObjectDefault({ inside: { added = "x", count = 11 } = {} }) {}
function rootObjectDefault({ kept, extra = 3 } = { kept: "yes" }) {}
function rootTupleDefault([keptTuple, extraTuple = "tail", , absent] = [2]) {}
function longerDefault([single] = [1, "two"]) {}
function annotated({ annotatedValue = 4 }: { annotatedValue?: 1 | 2 }) {}
const contextual: (p: { fixed: "locked" }) => void = ({ fixed = "loose" }) => {};
function unsupported({ sensitive = (value) => value }) {}
function unsupportedObject({ inner: { present = hue } = { present: "indigo" } }) {}
function unsupportedArray({ lane: [colored = hue] = ["indigo"] }) {}
function unsupportedComputed({ inner: { ["present"]: calculated = hue } = { present: "indigo" } }) {}
function unsupportedParenthesized({ inner: { present = hue } = ({ present: "indigo" }) }) {}
function unsupportedParenthesizedArray({ lane: [parenthesized, missing = 7] = (["yes"]) }) {}
function recursiveSibling({ source, copied = source.value }) {}
type GuardChoice = { tag: "num"; current: number } | { tag: "txt"; current: string };
function guarded({ tag, current }: GuardChoice) { if (tag === "num") current; }
function independent({ tag: otherTag, current: other }: GuardChoice) { if (otherTag === "txt") other; }
function* generatorDefault({ yieldedDefault = yield "x" }) {}
function nestedGuard({ one: { tag: nestedTag, current: nestedCurrent }, two: { tag: secondTag, current: secondCurrent } }: { one: GuardChoice; two: GuardChoice }) {
    if (nestedTag === "num") nestedCurrent;
    if (secondTag === "txt") secondCurrent;
}
function presentObject({ inner: { presentPrimitive = 1 } = { presentPrimitive: 2 } }) {}
function primitiveSource({ copiedPrimitive = 1 } = { copiedPrimitive: 2 }) {}
function annotatedSource({ stamped = 1 }: { stamped?: number } = { stamped: 2 }) {}
type ParentSource = { inside: { projected?: number; stable: string } };
declare const parentSource: ParentSource;
function annotatedNested({ inside: { projected = 1, stable } = { projected: 2, stable: "s" } }: ParentSource) {}
function initializedNested({ inside: { projected = 1, stable } = { projected: 2, stable: "s" } } = parentSource) {}
const { inside: { projected: variableProjected = 1, stable: variableStable } = { projected: 2, stable: "s" } } = parentSource;
(({ argumentDefault = 1 } = { argumentDefault: 2 }) => argumentDefault)({ argumentDefault: 3 });
