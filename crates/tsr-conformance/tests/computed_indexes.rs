//! Outcomes checked against pinned tsgo 5b1047d10d32e7d5b446be4de56b126ff42f82bb.
use tsr_conformance::{TestCase, types_baseline::FileTypes, types_producer};

fn expect(source: &str, wanted: &[&str]) {
    let case = TestCase::parse("probe/computed_indexes", "probe.ts", source);
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
fn computed_indexes() {
    expect(
        r"// @strict: true
// @target: esnext
declare let str: string;
declare let num: number;
declare let sym: symbol;
declare const unique: unique symbol;
export let stringNamed = { [str]: 1, a: true, m() { return 'x'; } };
export let numberNamed = { [num]: 1, 0: true, '3.5': 'yes', '-1': false, a: 'skip' };
export let mixed = { [str]: 1, [num]: true, [sym]: 'x', [unique]: false, a: 'a' };
export let mixedReadString = mixed[str];
export let mixedReadNumber = mixed[num];
export let mixedReadSymbol = mixed[sym];
export let stringCopy = { ...stringNamed };
export let stringMerge = { ...stringNamed, ...{ [str]: false } };
export let stringDrops = { ...stringNamed, extra: 1 };
export let methods = { [str]() { return 1; }, [num]() { return true; }, [sym]() { return 'x'; } };
export let inline = { ['x' + '']: 1, [Math.random()]: true, a: 'x' };
export let inlineRead = inline[str];
export let literals = { [str]: 1, ['lit']: true, [1]: 'x' };
export let repeated = { a: 1, a: 'x', [str]: true };
export let frozen = { [str]: 1, [num]: true, [sym]: 'x', a: 'a' } as const;
declare let unionKey: 'left' | 'right';
export let unionName = { [unionKey]: 1 };
",
        &[
            "stringNamed : { [str]: number; a: boolean; m(): string; }",
            "numberNamed : { [num]: number; 0: boolean; '3.5': string; '-1': boolean; a: string; }",
            "mixed : { [str]: number; [num]: boolean; [num]: boolean; [sym]: string; [unique]: boolean; a: string; }",
            "mixedReadString : string | number | boolean",
            "mixedReadNumber : boolean",
            "mixedReadSymbol : string | boolean",
            "stringCopy : { [str]: number; a: boolean; m(): string; }",
            "stringMerge : { [x: string]: number | boolean | (() => string); a: boolean; m(): string; }",
            "stringDrops : { a: boolean; m(): string; extra: number; }",
            "methods : { [str]: () => number; [num]: () => boolean; [num]: () => boolean; [sym]: () => string; }",
            "inline : { [x: string]: string | number | boolean; [x: number]: boolean; a: string; }",
            "inlineRead : string | number | boolean",
            "literals : { [x: string]: string | number | boolean; lit: boolean; 1: string; }",
            "repeated : { [str]: boolean; a: string; }",
            "frozen : { readonly [str]: 1; readonly [num]: true; readonly [num]: true; readonly [sym]: \"x\"; readonly a: 'a'; }",
            "unionName : { [unionKey]: number; }",
        ],
    );
}

#[test]
fn computed_more() {
    expect(
        r"// @strict: true
// @target: esnext
declare let str: string;
declare let num: number;
declare let sym: symbol;
export let repeated = { a: 1, a: 'x', [str]: true };
export let repeatedRead = repeated[str];
export let repeatedMethods = { m() { return 1; }, m() { return 'x'; }, [str]: true };
export let repeatedMethodsRead = repeatedMethods[str];
export let numeric = { [num]: 1, 0: true, '3.5': 'yes', '-1': false, '01': {}, a: 'skip' };
export let numericRead = numeric[num];
export let pair = { get [num]() { return true; }, set [num](v: string) {}, [str]: 1 };
export let pairRead = pair[num];
export let symbols = { [sym]: 'x', [str]: 1, plain: true };
export let symbolsRead = symbols[str];
export let tuple: [string, number] = ['x',1];
export let shorthand = { tuple, [str]: 0 };
export let shorthandRead = shorthand[str];
",
        &[
            "repeatedRead : string | number | boolean",
            "repeatedMethods : { [str]: boolean; m(): string; }",
            "repeatedMethodsRead : boolean | (() => number) | (() => string)",
            "numeric : { [num]: number; 0: boolean; '3.5': string; '-1': boolean; '01': {}; a: string; }",
            "numericRead : string | number | boolean",
            "pair : { [num]: boolean; [num]: string; [str]: number; [num]: boolean; [num]: string; }",
            "pairRead : string | boolean",
            "symbolsRead : number | boolean",
            "shorthandRead : number | [string, number]",
        ],
    );
}

#[test]
fn computed_generic() {
    expect(
        r"// @strict: true
// @target: esnext
export function unconstrained<T>(key:T) { return {[key]:0}; }
export function constrained<T extends string>(key:T) { return {[key]:0}; }
export function numeric<T extends number>(key:T) { return {[key]:0}; }
export function symbolic<T extends symbol>(key:T) { return {[key]:0}; }
declare let anyKey:any;
declare let neverKey:never;
export let anyName = {[anyKey]:1};
export let neverName = {[neverKey]:1};
",
        &[
            "unconstrained : <T>(key: T) => {}",
            "constrained : <T extends string>(key: T) => { [key]: number; }",
            "numeric : <T extends number>(key: T) => { [key]: number; }",
            "symbolic : <T extends symbol>(key: T) => { [key]: number; }",
            "anyName : { [anyKey]: number; }",
            "neverName : { [neverKey]: number; }",
        ],
    );
}

#[test]
fn computed_visibility() {
    expect(
        r"// @strict: true
// @target: esnext
export function local() { let key: string = 'x'; return { [key]: 1 }; }
export function parameter(key: string) { return { [key]: 1 }; }
export function destructured({key}: {key:string}) { return { [key]: 1 }; }
export function localDestructured(props: {key:string}) { const {key} = props; return { [key]: 1 }; }
export function nested() { function inner(key:string) { return { [key]: 1 }; } return inner('x'); }
declare let outside: { key: string };
export let chain = { [outside.key]: 1 };
export let missing = { [notDefined]: 1 };
export function keyofKey<T, K extends keyof T>(key: K) { return { [key]: 1 }; }
",
        &[
            "local : () => { [x: string]: number; }",
            "parameter : (key: string) => { [key]: number; }",
            "destructured : ({ key }: { key: string; }) => { [key]: number; }",
            "localDestructured : (props: { key: string; }) => { [x: string]: number; }",
            "nested : () => { [x: string]: number; }",
            "chain : { [outside.key]: number; }",
            "missing : { [x: number]: number; }",
            "keyofKey : <T, K extends keyof T>(key: K) => { [key]: number; }",
        ],
    );
}

#[test]
fn computed_keyof_alias_constraint() {
    expect(
        r"// @strict: true
// @target: esnext
type Key<T> = keyof T;
export function aliasKey<T, K extends Key<T>>(key: K) { return { [key]: 1 }; }
",
        &["aliasKey : <T, K extends Key<T>>(key: K) => { [key]: number; }"],
    );
}
