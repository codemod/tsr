// @target: es2015
// @strict: true
// @module: commonjs
// @filename: source.ts
type Inputs<F extends (...args: any[]) => any> = F extends (...args: infer P) => any ? P : never;
function helper(value: string, count: number) { return value; }
function exposed(value: Inputs<typeof helper>[0]) { return value; }
const rootView = exposed;
function typeShadow() {
    type Inputs<F> = boolean;
    const typeView = exposed;
    return typeView;
}
function valueShadow() {
    const helper = (value: boolean) => value;
    const valueView = exposed;
    return valueView;
}
namespace Scoped {
    export type LocalInputs<F extends (...args: any[]) => any> = F extends (...args: infer P) => any ? P : never;
    export function peer(value: number) { return value; }
    export function nested(value: LocalInputs<typeof peer>[0]) { return value; }
    const insideView = nested;
}
const outsideView = Scoped.nested;
namespace Sibling {
    const siblingView = exposed;
}
// @filename: cross.ts
const crossView = exposed;
// @filename: entry.ts
export type ModuleInputs<F extends (...args: any[]) => any> = F extends (...args: infer P) => any ? P : never;
export function peer(value: number) { return value; }
export function moduleFoo(value: ModuleInputs<typeof peer>[0]) { return value; }
const moduleInsideView = moduleFoo;
// @filename: consumer.ts
import { moduleFoo } from "./entry";
const importedView = moduleFoo;
