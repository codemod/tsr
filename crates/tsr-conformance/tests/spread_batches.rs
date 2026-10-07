//! Outcomes checked against pinned tsgo 5b1047d10d32e7d5b446be4de56b126ff42f82bb.
use tsr_conformance::{TestCase, types_baseline::FileTypes, types_producer};

fn expect(source: &str, wanted: &[&str]) {
    let case = TestCase::parse("probe/spread_batches", "probe.ts", source);
    let expected: Vec<_> = case
        .files
        .iter()
        .map(|unit| FileTypes { file: unit.name.clone(), assertions: Vec::new() })
        .collect();
    let lines: Vec<_> = types_producer::assertions_for_case(&case, &case.files.as_slice(), false)
        .iter()
        .flatten()
        .map(types_producer::Assertion::line)
        .collect();
    for line in wanted {
        assert!(lines.iter().any(|actual| actual == line), "missing {line}: {lines:?}");
    }
}

#[test]
fn mixed_batches_use_the_type_fold() {
    expect(
        r"// @strict: true
// @target: esnext
declare let source: { a: number };
export let before = { m(x: number) { return x; }, ...source };
export let after = { ...source, m(x: number) { return x; } };
export function generic<T>(value: T) { return { ...value, m(x: number) { return x; }, a: 1 }; }
export function genericTail<T>(value: T) { return { ...value, m(x: number) { return x; }, ...source, b: true }; }
export let getterBefore = { get x() { return 1; }, ...source };
export let getterAfter = { ...source, get x() { return 1; } };
export let setterAfter = { ...source, set x(value: number) {} };
export let pairAfter = { ...source, get x(): number { return 1; }, set x(value: string) {} };
export let constMethod = { ...source, m(x: number) { return x; } } as const;
export let frozenSource = { ...({ m() {} } as const) };
declare let union: { a: number } | { b: string };
export let unionAfter = { ...union, m() { return true; } };
export let replaced = { m() { return 1; }, ...{ m: true } };
export let replacing = { ...{ m: true }, m() { return 1; } };
declare const key: unique symbol;
export let uniqueAfter = { ...source, [key]() { return 1; } };
declare let dynamic: symbol;
export let dynamicAfter = { ...source, [dynamic]() { return 1; } };
export let dynamicSource = { ...{ [dynamic]() { return 1; } } };
",
        &[
            "before : { a: number; m(x: number): number; }",
            "after : { a: number; m(x: number): number; }",
            "generic : <T>(value: T) => T & { m(x: number): number; a: number; }",
            "genericTail : <T>(value: T) => T & { a: number; m(x: number): number; b: boolean; }",
            "getterBefore : { a: number; x: number; }",
            "getterAfter : { a: number; x: number; }",
            "setterAfter : { a: number; x: undefined; }",
            "pairAfter : { a: number; get x(): number; set x(value: string); }",
            "constMethod : { readonly a: number; readonly m: (x: number) => number; }",
            "frozenSource : { m: () => void; }",
            "unionAfter : { a: number; m(): boolean; } | { b: string; m(): boolean; }",
            "replaced : { m: boolean; }",
            "replacing : { m(): number; }",
            "uniqueAfter : { a: number; [key](): number; }",
            "dynamicAfter : { a: number; }",
            "dynamicSource : { [dynamic]: () => number; }",
        ],
    );
}

#[test]
fn symbol_components_and_accessor_copies() {
    expect(
        r"// @strict: true
// @target: esnext
declare let key: symbol;
declare let other: symbol;
export let components = { [key]: 1, [key]() {}, get [key]() { return true; } };
export let componentCopy = { ...components };
export let componentAgain = { ...componentCopy };
export let componentMerge = { ...componentCopy, ...{ [other]: 's' } };
export let componentRead = componentCopy[key];
export let frozenComponent = { ...componentCopy } as const;
export let componentAfter = { ...{ a: 1 }, [key]: 1 };
export let componentBefore = { [key]: 1, ...{ a: 1 } };
export let accessors = { get x(): number { return 1; }, set x(value: string) {} };
export let accessorCopy = { ...accessors };
export let accessorAgain = { ...accessorCopy };
export let accessorFrozen = { ...accessors } as const;
export let accessorReCopy = { ...accessorFrozen };
declare let optional: { x?: boolean };
export let accessorMerged = { ...accessors, ...optional };
export let constPair = { ...{}, get x(): number { return 1; }, set x(value: string) {} } as const;
",
        &[
            "components : { [key]: number; [key]: () => void; [key]: boolean; }",
            "componentCopy : { [key]: number; [key]: () => void; [key]: boolean; }",
            "componentAgain : { [key]: number; [key]: () => void; [key]: boolean; }",
            "componentMerge : { [x: symbol]: string | number | boolean | (() => void); }",
            "componentRead : number | boolean | (() => void)",
            "frozenComponent : { readonly [key]: number; readonly [key]: () => void; readonly [key]: boolean; }",
            "componentAfter : { a: number; }",
            "componentBefore : { a: number; }",
            "accessors : { get x(): number; set x(value: string); }",
            "accessorCopy : { get x(): number; set x(value: string); }",
            "accessorAgain : { get x(): number; set x(value: string); }",
            "accessorFrozen : { readonly x: number; }",
            "accessorReCopy : { x: number; }",
            "accessorMerged : { x: number | boolean; }",
            "constPair : { readonly x: number; }",
        ],
    );
}

#[test]
fn computed_keys_replace_members_and_generic_accessors_instantiate() {
    expect(
        r#"// @strict: true
// @target: esnext
export let quoted = { ...{}, ["a b"]() { return 1; } };
export let overwritten = { ...quoted, "a b": true };
export let overwrittenMethod = { ...{"a b": true}, ["a b"]() { return 1; } };
export let negative = { ...{}, [-1]() { return 1; } };
export let negativeOverwrite = { ...negative, [-1]: true };
export function pairGeneric<T, U>(read: T, write: U) { return { ...{}, get x(): T { return read; }, set x(value: U) {} }; }
export let pairInst = pairGeneric(1, 'a');
export let pairInstCopy = { ...pairInst };
"#,
        &[
            "quoted : { \"a b\"(): number; }",
            "overwritten : { \"a b\": boolean; }",
            "overwrittenMethod : { \"a b\"(): number; }",
            "negative : { [-1](): number; }",
            "negativeOverwrite : { [-1]: boolean; }",
            "pairGeneric : <T, U>(read: T, write: U) => { get x(): T; set x(value: U); }",
            "pairInst : { get x(): number; set x(value: string); }",
            "pairInstCopy : { get x(): number; set x(value: string); }",
        ],
    );
}

#[test]
fn instantiated_symbol_index_keeps_components_and_maps_its_value() {
    expect(
        r"// @strict: true
// @target: esnext
declare let key: symbol;
export function indexGeneric<T>(value: T) { return { [key]: value }; }
export let indexInstance = indexGeneric(1);
export let indexCopy = {...indexInstance};
export let indexRead = indexInstance[key];
",
        &[
            "indexGeneric : <T>(value: T) => { [key]: T; }",
            "indexInstance : { [key]: T; }",
            "indexCopy : { [key]: T; }",
            "indexRead : number",
        ],
    );
}
