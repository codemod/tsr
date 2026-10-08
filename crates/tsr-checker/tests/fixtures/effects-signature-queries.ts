export function ordinaryRepeat(a: unknown, b: unknown) { ordinary(); const left = a; const right = b; return {left, right}; }
declare function ordinary(): void;
declare function assertString(value: unknown): asserts value is string;
export function assertion(value: unknown) { assertString(value); const left = value; const right = value; return {left, right}; }
declare function predicate(value: unknown): value is string;
export function predicateBranch(value: unknown) { if (predicate(value)) { const left = value; const right = value; return {left, right}; } return {left: '', right: ''}; }
declare function stop(): never;
export function annotatedNever(value: string | number) { stop(); return value; }
function throwOnly() { throw 'error'; }
export function inferredNever(value: string | number) { throwOnly(); return value; }
declare const dotted: { assert(value: unknown): asserts value is string; ordinary(): void };
export function dottedAssertion(value: unknown) { dotted.assert(value); return value; }
export function dottedOrdinary(value: unknown) { dotted.ordinary(); return value; }
declare const optional: { ordinary(): void } | undefined;
export function optionalOrdinary(value: unknown) { optional?.ordinary(); return value; }
declare function generic<T>(value: T): void;
export function genericOrdinary(value: unknown) { generic(value); return value; }
declare function genericAssert<T>(value: unknown): asserts value is T;
export function genericAssertion(value: unknown) { genericAssert<string>(value); return value; }
export function loop(value: unknown, repeat: boolean) { while (repeat) { assertString(value); const left = value; const right = value; if (left === right) return left; } return value; }
export function contextual(value: unknown) { const callback: (x: unknown) => boolean = x => predicate(x); callback(value); return value; }
export function latePredicate(value: unknown) { if (inferredPredicate(value)) return value; return ''; }
function inferredPredicate(value: unknown) { return typeof value === 'string'; }
export function recursive(value: unknown): boolean { ordinary(); if (predicate(value)) return true; return recursive(value); }
