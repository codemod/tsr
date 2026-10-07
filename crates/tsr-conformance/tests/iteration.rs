//! Outcomes verified against pinned native tsgo 5b1047d10d32e7d5b446be4de56b126ff42f82bb.
use tsr_conformance::{TestCase, types_baseline::FileTypes, types_producer};

fn assertions(source: &str) -> Vec<String> {
    let case = TestCase::parse("probe/iteration", "iteration.ts", source);
    let expected: Vec<_> = case
        .files
        .iter()
        .map(|unit| FileTypes { file: unit.name.clone(), assertions: Vec::new() })
        .collect();
    types_producer::assertions_for_case(&case, &case.files.as_slice(), false)
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
