//! Outcomes verified against pinned native tsgo 5b1047d10d32e7d5b446be4de56b126ff42f82bb.
use tsr_conformance::{TestCase, types_baseline::FileTypes, types_producer};

fn assertions(source: &str) -> Vec<String> {
    let case = TestCase::parse("probe/iteration", "iteration.ts", source);
    let expected: Vec<_> = case
        .files
        .iter()
        .map(|unit| FileTypes { file: unit.name.clone(), assertions: Vec::new() })
        .collect();
    types_producer::assertions_for_case(&case, &expected, false)
        .iter()
        .flatten()
        .map(types_producer::Assertion::line)
        .collect()
}

fn expect(lines: &[String], wanted: &[&str]) {
    for line in wanted {
        assert!(lines.iter().any(|actual| actual == line), "missing {line}: {lines:?}");
    }
}

#[test]
fn generator_unit_slots_widen_symbols_but_preserve_context_and_nonunit_unions() {
    // Native 5b1047d1 declaration output: unit yield/return slots widen unique
    // symbols, but a multi-member aggregate retains the distinct identities.
    // The asymmetric sync/async slots also catch swapped yield/return context.
    let lines = assertions(
        r#"// @strict: true
// @target: esnext
export declare const first: unique symbol;
export declare const second: unique symbol;
export function* syncYield() { yield first; return "finished"; }
export function* syncReturn() { yield 23; return second; }
export async function* asyncYield() { yield Promise.resolve(second); return true; }
export async function* asyncReturn() { yield "async"; return Promise.resolve(first); }
export function* unionYield() { yield first; yield second; return 37; }
export function* unionReturn(flag: boolean) { yield false; if (flag) return first; return second; }
export function* regularLiterals() { yield "steady" as const; return 7 as const; }
export function* literalUnion() { yield 1; yield 2; return "done"; }
export function* nextUnique() { const resumed: typeof first = yield "next"; return 99; }
export async function* nextAsyncUnique() { const resumed: typeof second = yield true; return "ready"; }
export const contextualSync: () => Generator<typeof first, typeof second, boolean> = function* () { const resumed: boolean = yield first; return second; };
export const contextualAsync: () => AsyncGenerator<typeof second, typeof first, string> = async function* () { const resumed: string = yield second; return first; };
export const contextualLiteral: () => Generator<"kept", 91, boolean> = function* () { const resumed: boolean = yield "kept"; return 91; };
export const contextualNext: () => Generator<number, string, typeof first> = function* () { const resumed: typeof first = yield 31; return "resumed"; };
export const contextualAsyncNext: () => AsyncGenerator<string, boolean, typeof second> = async function* () { const resumed: typeof second = yield "await"; return false; };
"#,
    );
    expect(
        &lines,
        &[
            "syncYield : () => Generator<symbol, string, unknown>",
            "syncReturn : () => Generator<number, symbol, unknown>",
            "asyncYield : () => AsyncGenerator<symbol, boolean, unknown>",
            "asyncReturn : () => AsyncGenerator<string, symbol, unknown>",
            // Type baselines allow unique-symbol syntax; declaration emit
            // spells the same distinct identities `typeof first | typeof second`.
            "unionYield : () => Generator<unique symbol | unique symbol, number, unknown>",
            "unionReturn : (flag: boolean) => Generator<boolean, unique symbol | unique symbol, unknown>",
            "regularLiterals : () => Generator<\"steady\", 7, unknown>",
            "literalUnion : () => Generator<1 | 2, string, unknown>",
            "nextUnique : () => Generator<string, number, symbol>",
            "nextAsyncUnique : () => AsyncGenerator<boolean, string, symbol>",
            "function* () { const resumed: boolean = yield first; return second; } : () => Generator<unique symbol, unique symbol, boolean>",
            "async function* () { const resumed: string = yield second; return first; } : () => AsyncGenerator<unique symbol, unique symbol, string>",
            "function* () { const resumed: boolean = yield \"kept\"; return 91; } : () => Generator<\"kept\", 91, boolean>",
            "function* () { const resumed: typeof first = yield 31; return \"resumed\"; } : () => Generator<number, string, unique symbol>",
            "async function* () { const resumed: typeof second = yield \"await\"; return false; } : () => AsyncGenerator<string, false, unique symbol>",
        ],
    );
}

#[test]
fn completed_iteration_names_preserve_inherited_computed_members_and_absence() {
    let lines = assertions(
        r"// @strict: true
// @target: esnext
declare const marker: unique symbol;
interface Extra { readonly [marker]: number; }
interface Completed<T> extends Extra { next(): { done: false; value: T }; }
interface Cleanup<T> extends Extra { next(): { done: false; value: T }; return(): { done: false; value: 29 }; }
declare const completed: { [Symbol.iterator](): Completed<13> };
declare const cleanup: { [Symbol.iterator](): Cleanup<13> };
export function* completedDelegate() { yield* completed; }
export function* cleanupDelegate() { yield* cleanup; }
export function* openDelegate<T extends { done: false }>(source: { [Symbol.iterator](): { next(): T } }) { yield* source; }
",
    );
    expect(
        &lines,
        &[
            "completedDelegate : () => Generator<13, void, unknown>",
            "cleanupDelegate : () => Generator<13 | 29, void, unknown>",
            // An open generic protocol is still unsupported, not a completed
            // absence that could publish any as a successful slot answer.
            "openDelegate : error",
        ],
    );
    let aliases = TestCase::parse(
        "probe/iteration_alias_absence",
        "aliases.ts",
        r"// @strict: true
// @target: es2015
interface Props<T, K> { selector?: (obj: T) => K; }
type Result<T, K> = [K, T];
interface Person { name: string; }
declare function select<K = Person>(props?: Props<Person, K>): Result<Person, K>;
const [first] = select();
const tuple = select();
const [again] = tuple;
",
    );
    let diagnostics = tsr_conformance::diagnostics_suite::reported_for(&aliases);
    assert!(diagnostics.is_empty(), "tuple alias is not a certified non-iterable: {diagnostics:?}");
}

#[test]
fn union_iterator_methods_preserve_optional_mapped_and_inherited_metadata() {
    let lines = assertions(
        r"// @strict: true
// @target: esnext
declare const marker: unique symbol;
interface NextBase { next(): IteratorYieldResult<13>; }
interface NextChild extends NextBase { readonly [marker]: number; }
type OptionalNext<T> = { [K in keyof T]?: T[K] };
type RequiredNext<T> = { [K in keyof T]-?: T[K] };
declare const inherited: { [Symbol.iterator](): NextChild | { next(): IteratorYieldResult<29> } };
declare const optional: { [Symbol.iterator](): NextChild | { next?: () => IteratorYieldResult<29> } };
declare const missing: { [Symbol.iterator](): NextChild | { other: boolean } };
declare const mappedOptional: { [Symbol.iterator](): NextChild | OptionalNext<{ next(): IteratorYieldResult<29> }> };
declare const mappedRequired: { [Symbol.iterator](): NextChild | RequiredNext<{ next?: () => IteratorYieldResult<29> }> };
export function* inheritedDelegate() { yield* inherited; }
export function* optionalDelegate() { yield* optional; }
export function* missingDelegate() { yield* missing; }
export function* mappedOptionalDelegate() { yield* mappedOptional; }
export function* mappedRequiredDelegate() { yield* mappedRequired; }
",
    );
    expect(
        &lines,
        &[
            "inheritedDelegate : () => Generator<13 | 29, void, unknown>",
            "optionalDelegate : () => Generator<any, void, unknown>",
            "missingDelegate : () => Generator<any, void, unknown>",
            "mappedOptionalDelegate : () => Generator<any, void, unknown>",
            "mappedRequiredDelegate : () => Generator<13 | 29, void, unknown>",
        ],
    );
}

#[test]
fn iterator_result_common_value_absence_differs_from_unsupported_metadata() {
    let lines = assertions(
        r#"// @strict: true
// @target: esnext
interface Source<R> { [Symbol.iterator](): { next(): R }; }
declare const forward: Source<IteratorYieldResult<number> | Promise<IteratorResult<string, string>>>;
declare const reverse: Source<Promise<IteratorResult<string, string>> | IteratorYieldResult<number>>;
declare const missing: Source<{ done: false; tag: 13 } | { done: false; tag: 29 }>;
declare const present: Source<{ done: false; value: string } | { done: false; value: number }>;
declare const optional: Source<{ done: false; value?: number }>;
export function* forwardDelegate() { yield* forward; }
export function* reverseDelegate() { yield* reverse; }
export function* missingDelegate() { yield* missing; }
export function* presentDelegate() { yield* present; }
export function* optionalDelegate() { yield* optional; }
export function* openDelegate<T extends { done: false }>(source: Source<{ done: false; value: number } | T>) { yield* source; }
declare const first: unique symbol;
declare const second: unique symbol;
type Mixed = (() => Generator<typeof first, string, unknown>) | (() => AsyncGenerator<typeof second, string, unknown>);
export const mixedSync = (function* () { yield first; return "done"; }) satisfies Mixed;
export const mixedAsync = (async function* () { yield second; return "done"; }) satisfies Mixed;
type LiteralMixed = (() => Generator<"kept", string, unknown>) | (() => AsyncGenerator<"kept", string, unknown>);
export const mixedLiteral = (function* () { yield "kept"; return "done"; }) satisfies LiteralMixed;
type InvalidMixed = () => number | Generator<typeof first, string, unknown>;
export const mixedNumber = (function* () { yield first; return "done"; }) satisfies InvalidMixed;
"#,
    );
    expect(
        &lines,
        &[
            "forwardDelegate : () => Generator<any, void, unknown>",
            "reverseDelegate : () => Generator<any, void, unknown>",
            "missingDelegate : () => Generator<any, void, unknown>",
            "presentDelegate : () => Generator<string | number, void, unknown>",
            "optionalDelegate : () => Generator<number | undefined, void, unknown>",
            "openDelegate : error",
            // Native's raw mixed context widens sync slots, unlike a filtered
            // return-expression context. Async still retains its unique slot.
            "mixedSync : () => Generator<symbol, string, unknown>",
            "mixedAsync : () => AsyncGenerator<unique symbol, string, unknown>",
            "mixedLiteral : () => Generator<string, string, unknown>",
            "mixedNumber : () => Generator<symbol, string, unknown>",
        ],
    );
}

#[test]
fn semantic_iteration_uses_inherited_methods_and_yield_arms() {
    let lines = assertions(
        r"// @strict: true
// @target: es2015
export declare function gather<T extends unknown[]>(...args:T):T;
interface Numbers extends Array<number> {}
interface ReadonlyNumbers extends ReadonlyArray<number> {}
declare const numbers: Numbers;
declare const readonlyNumbers: ReadonlyNumbers;
export const a = [...numbers];
export const b = gather(...numbers);
export const c = [...readonlyNumbers];
export const d = gather(...readonlyNumbers);
declare const iterable: { [Symbol.iterator](): { next(): {done:false,value:number} | {done:true,value:string} } };
export const e = [...iterable];
export const f = gather(...iterable);
declare const other: { [Symbol.iterator](): { next(): {value:string} } };
export const g = [...other];
export const h = [...new Map<string,number>()];
export const i = [...(null as unknown as typeof iterable | typeof other)];
declare const overloaded: { [Symbol.iterator](): { next(): {value:number} }; [Symbol.iterator](required:number): { next(): {value:string} } };
export const j = [...overloaded];
declare const returned: { [Symbol.iterator](): { next(): {done:false,value:number}; return(): {done:false,value:boolean} } };
export const k = [...returned];
",
    );
    expect(
        &lines,
        &[
            "a : number[]",
            "b : number[]",
            "c : number[]",
            "d : number[]",
            "e : number[]",
            "f : number[]",
            "g : string[]",
            "h : [string, number][]",
            "i : (string | number)[]",
            "j : number[]",
            "k : (number | boolean)[]",
        ],
    );
}

#[test]
fn builtin_iterator_return_and_indirect_heritage_substitution() {
    let source = r"// @strict: true
// @target: es2015
export declare const intrinsic: BuiltinIteratorReturn;
export const iterator = [1][Symbol.iterator]();
export const result = iterator.next();
export const value = result.value;
interface Base<A> { get(): A; own<A>(x: A): A }
interface Middle<B> extends Base<B[]> {}
interface Outer<C> extends Middle<C> {}
declare const instance: Outer<string>;
export const inherited = instance.get();
export const shadowed = instance.own(1);
";
    for (directives, intrinsic, result, value) in [
        (
            "// @strict: true\n",
            "intrinsic : undefined",
            "result : IteratorResult<number, undefined>",
            "value : number | undefined",
        ),
        (
            "// @strict: true\n// @strictBuiltinIteratorReturn: false\n",
            "intrinsic : any",
            "result : IteratorResult<number, any>",
            "value : any",
        ),
        (
            "// @strict: false\n",
            "intrinsic : any",
            "result : IteratorResult<number, any>",
            "value : any",
        ),
        (
            "// @strict: false\n// @strictBuiltinIteratorReturn: true\n",
            "intrinsic : undefined",
            "result : IteratorResult<number, undefined>",
            "value : number",
        ),
    ] {
        let lines =
            assertions(&format!("{directives}{}", source.replace("// @strict: true\n", "")));
        expect(&lines, &[intrinsic, result, value, "inherited : string[]", "shadowed : 1"]);
    }
}

#[test]
fn final_results_are_not_yields_and_optional_methods_contribute_independently() {
    let lines = assertions(
        r"// @strict: true
// @target: es2015
declare const finished: { [Symbol.iterator](): { next(): { done: true, value: string } } };
export const a = [...finished];
declare const missing: { [Symbol.iterator](): { next(): { done: true } } };
export const b = [...missing];
declare const optional: { [Symbol.iterator](): { next?: () => { value: string } } };
export const c = [...optional];
declare const badReturn: { [Symbol.iterator](): { next(): { value: string }; return: number } };
export const d = [...badReturn];
declare const badNext: { [Symbol.iterator](): { next: number } };
export const e = [...badNext];
declare const malformed: { [Symbol.iterator](): { next(): {} } };
export const f = [...malformed];
declare const doneUnion: { [Symbol.iterator](): { next(): { done: false, value: number } | {done:true} } };
export const g = [...doneUnion];
declare const onlyReturns: { [Symbol.iterator](): { next(): {done: true, value: string}; return(): {done: true, value: boolean} } };
export const h = [...onlyReturns];
declare const throwing: { [Symbol.iterator](): { next(): {value:number}; throw(): {value:boolean} } };
export const i = [...throwing];
",
    );
    // Invalid next signatures and missing value diagnostics remain separate
    // work; these checks cover the native outcomes this port can establish.
    expect(
        &lines,
        &[
            "a : any[]",
            "b : any[]",
            "d : string[]",
            "g : number[]",
            "h : any[]",
            "i : (number | boolean)[]",
        ],
    );
}

#[test]
fn recursive_iterator_return_does_not_invalidate_its_callable() {
    let lines = assertions(
        r"// @target: ES6
// @noImplicitAny: true
class MyStringIterator {
    next() { return v; }
    [Symbol.iterator]() { return this; }
}
for (var v of new MyStringIterator) { }
",
    );
    expect(&lines, &["next : () => any"]);
    let lines = assertions(
        r"// @target: ES6
// @noImplicitAny: true
class MyStringIterator {
    [Symbol.iterator]() { return v; }
}
for (var v of new MyStringIterator) { }
",
    );
    expect(&lines, &["[Symbol.iterator] : () => any"]);
}

#[test]
fn written_nullish_types_stay_distinct_from_widening_global_type_queries() {
    let lines = assertions(
        r"// @strict: false
// @target: es2015
export declare const writtenUndefined: undefined;
export declare const writtenNull: null;
export declare const queried: typeof undefined;
function shadow(undefined: undefined) {
    const local: typeof undefined = undefined;
    return local;
}
",
    );
    expect(
        &lines,
        &[
            "writtenUndefined : undefined",
            "writtenNull : null",
            "queried : any",
            "local : undefined",
        ],
    );
}

#[test]
fn iterable_unions_distinguish_absent_yields_from_never() {
    let lines = assertions(
        r"// @strict: true
// @target: es2015
declare const returns: { [Symbol.iterator](): { next(): {done:true, value:string} } };
declare const yields: { [Symbol.iterator](): { next(): {done:false, value:number} } };
export const union = [...(null as unknown as typeof returns | typeof yields)];
declare const never: Iterable<never>;
export const empty = [...never];
",
    );
    expect(&lines, &["union : number[]", "empty : never[]"]);
}

#[test]
fn arbitrary_iterable_spreads_preserve_const_and_contextual_rest_positions() {
    let lines = assertions(
        r#"// @strict: true
// @target: es2015
declare const words: Iterable<string>;
declare const nums: Iterable<number>;
declare const tokens: Iterable<"token">;
export const ordinary = [17, ...tokens, false];
export const contextual: [number, ...string[]] = [9, ...words];
export const satisfied = [9, ...words] satisfies [number, ...string[]];
export const constant = [1, ...nums, false] as const;
export const entries = [true, ...new Map<string, number>()] as const;
export function generic<T extends Iterable<string>>(t: T) { return [1, ...t, false] as const; }
"#,
    );
    expect(
        &lines,
        &[
            "ordinary : (number | \"token\" | boolean)[]",
            "[9, ...words] : [number, ...string[]]",
            "satisfied : [number, ...string[]]",
            "constant : readonly [1, ...number[], false]",
            "entries : readonly [true, ...[string, number][]]",
            "generic : <T extends Iterable<string>>(t: T) => readonly [1, ...string[], false]",
        ],
    );
}

#[test]
fn ordinary_spread_uses_the_iterator_not_a_numeric_index() {
    let lines = assertions(
        r#"// @strict: true
// @target: es2015
declare const indexOnly: { [n: number]: "indexed" };
export const invalid = [3, ...indexOnly];
export const invalidConst = [3, ...indexOnly, true] as const;
declare const hybrid: { [n: number]: number; [Symbol.iterator](): { next(): { done: false, value: "yielded" } | { done: true, value: Date } } };
export const iterated = [false, ...hybrid];
let head: boolean;
let tail: typeof hybrid;
[head, ...tail] = null as any;
declare const genuine: { [Symbol.iterator](): { next(): { done: false, value: string } } };
let iteratorTail: typeof genuine;
[head, ...iteratorTail] = null as any;
"#,
    );
    expect(
        &lines,
        &[
            "invalid : any[]",
            "invalidConst : readonly [3, ...any[], true]",
            "iterated : (\"yielded\" | boolean)[]",
            "[head, ...tail] : [boolean, ...number[]]",
            "[head, ...iteratorTail] : [boolean, ...string[]]",
        ],
    );
}

#[test]
fn missing_iteration_protocols_recover_without_borrowing_partial_union_yields() {
    let lines = assertions(
        r#"// @strict: true
// @target: es2015
declare const badUnion: Iterable<string> | { count: number };
export const badUnionSpread = [...badUnion];
declare const validUnion: Iterable<"text"> | readonly [42, false];
export const validUnionSpread = [...validUnion];
declare const nonCallable: { [Symbol.iterator]: number };
export const nonCallableSpread = [...nonCallable];
declare const optional: { [Symbol.iterator]?: () => { next(): {value: string} } };
export const optionalSpread = [...optional];
export const primitiveSpread = [...123];
declare const unknown: unknown;
export const unknownSpread = [...unknown];
"#,
    );
    expect(
        &lines,
        &[
            "badUnionSpread : any[]",
            "validUnionSpread : (\"text\" | 42 | false)[]",
            "nonCallableSpread : any[]",
            "optionalSpread : any[]",
            "primitiveSpread : any[]",
            "unknownSpread : any[]",
        ],
    );
}

#[test]
fn invalid_next_methods_are_absent_types_not_completed_iterator_yields() {
    let lines = assertions(
        r#"// @strict: true
// @target: es2015
declare const optionalNext: { [Symbol.iterator](): { next?: () => {value: string} } };
export const optional = [...optionalNext];
declare const missingNext: { [Symbol.iterator](): {} };
export const missing = [...missingNext];
declare const malformedResult: { [Symbol.iterator](): { next(): {} } };
export const malformed = [...malformedResult];
declare const recoveredReturn: { [Symbol.iterator](): { next: number; return(): {value: "returned"} } };
export const returned = [...recoveredReturn];
declare const recoveredThrow: { [Symbol.iterator](): { next?: () => {value: string}; throw(): {value: 73} } };
export const thrown = [...recoveredThrow];
declare const empty: { [Symbol.iterator](): {next: number} };
declare const good: Iterable<"actual">;
export const combined = [...(null as unknown as typeof empty | typeof good)];
"#,
    );
    expect(
        &lines,
        &[
            "optional : any[]",
            "missing : any[]",
            "malformed : any[]",
            "returned : \"returned\"[]",
            "thrown : 73[]",
            "combined : any[]",
        ],
    );
}

#[test]
fn assignment_rests_do_not_apply_spread_any_recovery() {
    let lines = assertions(
        r"// @strict: true
// @target: es2015
let missing: {};
[...missing] = null as any;
let finished: { [Symbol.iterator](): { next(): {done:true, value:string} } };
[...finished] = null as any;
let never: Iterable<never>;
[...never] = null as any;
",
    );
    expect(
        &lines,
        &["[...missing] : unknown[]", "[...finished] : unknown[]", "[...never] : never[]"],
    );
}

#[test]
fn iterator_result_values_are_read_after_filtering_whole_yield_and_return_unions() {
    let lines = assertions(
        r"// @strict: true
// @target: es2015
declare const completedMissing: { [Symbol.iterator](): {next(): {done:true}} };
declare const completed: { [Symbol.iterator](): {next(): {done:true, value:string}} };
declare const good: Iterable<number>;
export const missingUnion = [...(null as unknown as typeof completedMissing | typeof good)];
export const completedUnion = [...(null as unknown as typeof completed | typeof good)];
declare const partial: { [Symbol.iterator](): { next(): {done:false} | {done:true,value:string} } };
export const partialUnion = [...(null as unknown as typeof partial | typeof good)];
declare const missingYield: { [Symbol.iterator](): { next(): {done:false} | {done:false,value:number} } };
export const missingYieldUnion = [...(null as unknown as typeof missingYield | typeof good)];
declare const completeYield: { [Symbol.iterator](): { next(): {done:false,value:42} | {done:true} } };
export const completeYieldUnion = [...(null as unknown as typeof completeYield | typeof good)];
",
    );
    expect(
        &lines,
        &[
            "missingUnion : any[]",
            "completedUnion : number[]",
            "partialUnion : number[]",
            "missingYieldUnion : any[]",
            "completeYieldUnion : number[]",
        ],
    );
}

const ELISIONS: &str = r#"
declare const words: Iterable<string>;
declare const fixed: readonly ["tail", 37];
declare const numbers: number[];
export const prefix = [1, , ...words] as const;
export const between = [1, , ...fixed, false] as const;
export const trailing = [1, ...words, , false] as const;
export const after = [...numbers, , "end"] as const;
export const contextual = [9, , ...words] satisfies [number, undefined?, ...string[]];
export const plainConst = [1, , "last", false] as const;
export const onlyHoles = [, ,] as const;
export const array = [1, , ...words, false];
export const explicitUndefined = [1, undefined, false] as const;
export const explicitAfterHole = [1, , undefined, false] as const;
export const plainLength = plainConst.length;
export const holesLength = onlyHoles.length;
export const explicitLength = explicitUndefined.length;
export const betweenLength = between.length;
export const holeValue = onlyHoles[0];
"#;

#[test]
fn elisions_preserve_optional_hole_semantics_independently_of_display() {
    for (exact, wanted) in [
        (
            false,
            ["plainLength : 4", "holesLength : 2", "betweenLength : 5", "holeValue : undefined"],
        ),
        (
            true,
            [
                "plainLength : 1 | 2 | 3 | 4",
                "holesLength : 0 | 1 | 2",
                "betweenLength : 4 | 5",
                "holeValue : undefined",
            ],
        ),
    ] {
        let lines = assertions(&format!(
            "// @strict: true\n// @target: es2015\n// @exactOptionalPropertyTypes: {exact}\n{ELISIONS}"
        ));
        expect(&lines, &wanted);
        expect(
            &lines,
            &["explicitLength : 3", "array : (string | number | boolean | undefined)[]"],
        );
    }
}

#[test]
fn optional_holes_hide_missing_but_not_explicit_undefined_in_tuple_display() {
    for (exact, wanted) in [
        (
            false,
            [
                "prefix : readonly [1, undefined, ...string[]]",
                "between : readonly [1, undefined, \"tail\", 37, false]",
                "trailing : readonly [1, ...string[], undefined, false]",
                "after : readonly [...number[], undefined, \"end\"]",
                "contextual : [number, undefined, ...string[]]",
                "plainConst : readonly [1, undefined, \"last\", false]",
                "onlyHoles : readonly [undefined, undefined]",
                "explicitAfterHole : readonly [1, undefined, undefined, false]",
            ],
        ),
        (
            true,
            [
                "prefix : readonly [1, never?, ...string[]]",
                "between : readonly [1, undefined, \"tail\", 37, false?]",
                "trailing : readonly [1, ...(string | false | undefined)[]]",
                "after : readonly (number | \"end\" | undefined)[]",
                "contextual : [number, never?, ...string[]]",
                "plainConst : readonly [1, never?, \"last\"?, false?]",
                "onlyHoles : readonly [never?, never?]",
                "explicitAfterHole : readonly [1, never?, undefined?, false?]",
            ],
        ),
    ] {
        let lines = assertions(&format!(
            "// @strict: true\n// @target: es2015\n// @exactOptionalPropertyTypes: {exact}\n{ELISIONS}"
        ));
        expect(&lines, &wanted);
        expect(&lines, &["explicitUndefined : readonly [1, undefined, false]"]);
    }
}

#[test]
fn tuple_relations_remove_missing_only_at_optional_source_and_target_positions() {
    let source = r#"
declare function probe<T>(): T;
export const holes = [,] as const;
export const explicit = [undefined] as const;
export const prefix = [17, , false] as const;
declare const words: Iterable<string>;
export const spreadHole = [17, , ...words] as const;
export const holePresent = probe<[undefined] extends typeof holes ? true : false>();
export const holeEmpty = probe<[] extends typeof holes ? true : false>();
export const explicitPresent = probe<[undefined] extends typeof explicit ? true : false>();
export const explicitEmpty = probe<[] extends typeof explicit ? true : false>();
export const holeToOptionalNumber = probe<typeof holes extends readonly [37?] ? true : false>();
export const holeToNumericRest = probe<typeof holes extends readonly [...37[]] ? true : false>();
export const wrongPrefix = probe<[18, undefined, false] extends typeof prefix ? true : false>();
export const explicitPrefixHole = probe<[17, undefined, false] extends typeof prefix ? true : false>();
export const absentPrefixTail = probe<[17] extends typeof prefix ? true : false>();
export const spreadPresent = probe<[17, undefined, "word"] extends typeof spreadHole ? true : false>();
export const spreadEmpty = probe<[17] extends typeof spreadHole ? true : false>();
export const spreadWrongRest = probe<[17, undefined, 73] extends typeof spreadHole ? true : false>();
declare const optional: readonly [17, number?, ...string[]];
declare const explicitOptional: readonly [17, (number | undefined)?, ...string[]];
export const optionalSame = probe<typeof optional extends readonly [17, number?, ...string[]] ? true : false>();
export const optionalExplicitTarget = probe<typeof optional extends typeof explicitOptional ? true : false>();
export const optionalExplicitSource = probe<typeof explicitOptional extends typeof optional ? true : false>();
export const optionalWrongHead = probe<typeof optional extends readonly [false, number?, ...string[]] ? true : false>();
export const optionalWrongRest = probe<typeof optional extends readonly [17, number?, ...boolean[]] ? true : false>();
export const optionalIntoNumberRest = probe<typeof optional extends readonly [17, ...(number | string)[]] ? true : false>();
"#;
    for exact in [false, true] {
        let lines = assertions(&format!(
            "// @strict: true\n// @target: es2015\n// @exactOptionalPropertyTypes: {exact}\n{source}"
        ));
        let present = if exact { "false" } else { "true" };
        let empty = if exact { "true" } else { "false" };
        expect(
            &lines,
            &[
                &format!("holePresent : {present}"),
                &format!("holeEmpty : {empty}"),
                "explicitPresent : true",
                "explicitEmpty : false",
                "holeToOptionalNumber : true",
                "holeToNumericRest : false",
                "wrongPrefix : false",
                &format!("explicitPrefixHole : {present}"),
                &format!("absentPrefixTail : {empty}"),
                &format!("spreadPresent : {present}"),
                &format!("spreadEmpty : {empty}"),
                "spreadWrongRest : false",
                "optionalSame : true",
                "optionalExplicitTarget : true",
                &format!("optionalExplicitSource : {present}"),
                "optionalWrongHead : false",
                "optionalWrongRest : false",
                "optionalIntoNumberRest : false",
            ],
        );
    }
}

#[test]
fn tuple_assignment_diagnostics_preserve_the_native_multiset_in_both_exact_modes() {
    let source = r"export const holes = [,] as const;
export const explicit = [undefined] as const;
export const present: typeof holes = [undefined];
export const absent: typeof holes = [];
export const explicitOk: typeof explicit = [undefined];
export const explicitBad: typeof explicit = [];
declare const optional: readonly [17, number?, ...string[]];
declare const explicitOptional: readonly [17, (number | undefined)?, ...string[]];
export const implicitToExplicit: typeof explicitOptional = optional;
export const explicitToImplicit: typeof optional = explicitOptional;
function omittedAssignments(t: [number, string?, boolean?]) {
    t = [42, ,];
    t = [42, , ,];
}
";
    for (exact, wanted) in [
        (false, vec![(4, 14, 2322), (6, 14, 2322)]),
        (true, vec![(3, 14, 2322), (6, 14, 2322), (10, 14, 2322)]),
    ] {
        let case = TestCase::parse(
            "probe/tuple_assignments",
            "iteration.ts",
            &format!(
                "// @strict: true\n// @target: es2015\n// @exactOptionalPropertyTypes: {exact}\n{source}"
            ),
        );
        let diagnostics = tsr_conformance::diagnostics_suite::reported_for(&case);
        assert!(diagnostics.iter().all(|diagnostic| diagnostic.file == "iteration.ts"));
        let mut actual: Vec<_> = diagnostics
            .iter()
            .map(|diagnostic| (diagnostic.line, diagnostic.column, diagnostic.code))
            .collect();
        actual.sort_unstable();
        assert_eq!(actual, wanted, "exactOptionalPropertyTypes: {exact}");
    }
}

#[test]
fn optional_tuple_check_types_preserve_named_enum_identity_and_reads() {
    let source = r"declare function probe<T>(): T;
enum First { Shared = 17, Tail = 37 }
enum Other { Shared = 17, Tail = 37 }
declare const optional: readonly [17, First?];
declare const explicit: readonly [17, (First | undefined)?];
export const same = probe<typeof optional extends readonly [17, First?] ? true : false>();
export const different = probe<typeof optional extends readonly [17, Other?] ? true : false>();
export const narrowRest = probe<typeof optional extends readonly [17, ...First[]] ? true : false>();
export const wideRest = probe<typeof optional extends readonly [17, ...(First | undefined)[]] ? true : false>();
export const differentRest = probe<typeof optional extends readonly [17, ...(Other | undefined)[]] ? true : false>();
export const explicitSource = probe<typeof explicit extends typeof optional ? true : false>();
export const read = optional[1];
";
    for exact in [false, true] {
        let lines = assertions(&format!(
            "// @strict: true\n// @target: es2015\n// @exactOptionalPropertyTypes: {exact}\n{source}"
        ));
        let explicit = if exact { "false" } else { "true" };
        expect(
            &lines,
            &[
                "same : true",
                "different : false",
                "narrowRest : false",
                "wideRest : true",
                "differentRest : false",
                &format!("explicitSource : {explicit}"),
                "read : First | undefined",
            ],
        );
    }
}

/// OSS iterator errors caused by inference or narrowing must be fixed at the
/// operand's owner. In particular, `any[]` is not a tuple, and `never` is not
/// `Iterable<never>`; silencing their errors would admit invalid programs.
#[test]
fn tuple_spreads_and_iteration_keep_valid_and_invalid_contracts_distinct() {
    let source = r"declare function pair(n: number, label?: string): void;
interface Methods { pair(n: number, label?: string): void; }
declare const methods: Methods;
methods.pair = (...args) => pair(...args);
declare const tuple: readonly [number, string?];
pair(...tuple);
declare const wrong: [string, number];
pair(...wrong);
declare const array: number[];
pair(...array);
declare function rest(...values: number[]): void;
rest(...array);
declare const invalid: { count: number };
for (const x of invalid) {}
declare const nothing: never;
for (const x of nothing) {}
declare const empty: Iterable<never>;
for (const x of empty) {}
declare const sync: Iterable<string>;
declare const async: AsyncIterable<number>;
async function consume() {
    for (const x of sync) {}
    for await (const x of async) {}
    for await (const x of sync) {}
    for (const x of async) {}
    for await (const x of invalid) {}
    for await (const x of nothing) {}
    for await (const x of empty) {}
}
declare const anyArray: any[];
pair(...anyArray);
";
    let case = TestCase::parse(
        "probe/oss_spread_iteration_controls",
        "iteration.ts",
        &format!("// @strict: true\n// @target: esnext\n{source}"),
    );
    let diagnostics = tsr_conformance::diagnostics_suite::reported_for(&case);
    assert!(diagnostics.iter().all(|diagnostic| diagnostic.file == "iteration.ts"));
    let mut actual: Vec<_> = diagnostics
        .iter()
        .map(|diagnostic| (diagnostic.line, diagnostic.column, diagnostic.code))
        .collect();
    actual.sort_unstable();
    // Pinned native 5b1047d: exact multiset, including the invalid tuple's
    // element relation, not merely an absence of false iterator diagnostics.
    assert_eq!(
        actual,
        [
            (8, 6, 2345),
            (10, 6, 2556),
            (14, 17, 2488),
            (16, 17, 2488),
            (25, 21, 2488),
            (26, 27, 2504),
            (27, 27, 2504),
            (31, 6, 2556),
        ]
    );
}
