// Native 5b1047d10d32e7d5b446be4de56b126ff42f82bb runtime/declaration control.
// @strict: true
// @declaration: true
// @target: esnext
export type ReturnOf<T> = T extends (...args: any[]) => infer R ? R : never;
export type Wrapped<T> = ReturnOf<T>;
export declare const booleanReturn: ReturnOf<BooleanConstructor>;
export declare const booleanWrapped: Wrapped<BooleanConstructor>;
export type TestBit<A, B> = A extends B ? 1 : 0;
export declare const genericBit: TestBit<string, string>;
export type Tail<T extends any[]> = T extends [any, ...infer U] ? U : never;
export declare function tail<T extends any[]>(value: T): Tail<T>;
export const concreteTail = tail([1, 'x'] as [number, string]);
export type F<T> = () => void;
export declare function phantom<T>(value: F<T>): T;
export declare const stringFn: F<string>;
export const phantomString = phantom(stringFn);
export const isString = (value: unknown) => typeof value === 'string';
export function recursivePredicate(value: unknown) { return typeof value === 'string' && recursivePredicate(value); }
