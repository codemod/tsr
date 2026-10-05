// @target: es2015
// @strict: true
// @filename: aliases.ts
type Inputs<First, Second = First> = boolean;
type StringBody<F> = ((string));
type NumberBody<F> = number;
type BigIntBody<F> = bigint;
type SymbolBody<F> = symbol;
type VoidBody<F> = void;
type UndefinedBody<F> = undefined;
type AnyBody<F> = any;
type UnknownBody<F> = unknown;
type NeverBody<F> = never;
type ObjectKeyword<F> = object;
type BooleanUnion<F> = true | false;
declare let left: Inputs<string, number>;
declare let right: Inputs<number, string>;
declare let defaulted: Inputs<symbol>;
declare let namedUnion: BooleanUnion<string>;
function consumeBool(value: Inputs<string>): Inputs<string> { return value; }
function owner<Outer>() {
    type Uncaptured<Inner> = ((number));
    return 17;
}
