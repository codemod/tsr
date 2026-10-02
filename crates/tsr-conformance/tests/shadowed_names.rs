//! Outcomes checked against pinned tsgo 5b1047d10d32e7d5b446be4de56b126ff42f82bb
//! (`testdata/baselines/reference/submodule/compiler/{bluebirdStaticThis,
//! declarationEmitShadowing,contextualSignatureInstantiation2}.types` and
//! `conformance/conditionalTypes1.types`).
use tsr_conformance::{TestCase, types_baseline::FileTypes, types_producer};

fn lines(source: &str) -> Vec<String> {
    let case = TestCase::parse("probe/shadowed_names", "probe.ts", source);
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

fn expect(source: &str, wanted: &[&str]) {
    let lines = lines(source);
    for line in wanted {
        assert!(lines.iter().any(|actual| actual == line), "missing {line}: {lines:?}");
    }
}

/// A qualified generic reference is one type per type-argument identity: the
/// class's `R` and an overload's shadowing `R` give two `Promise.Thenable<R>`.
#[test]
fn qualified_references_keep_type_argument_identity() {
    expect(
        r"// @target: es2015
export declare class Promise<R> implements Promise.Thenable<R> {
    constructor(callback: (thenableOrResult: R | Promise.Thenable<R>) => void);
    static try<R>(fn: () => Promise.Thenable<R>): Promise<R>;
    static try<R>(fn: () => R): Promise<R>;
}
export declare namespace Promise {
    export interface Thenable<R> { then<U>(f: (value: R) => Thenable<U>): Thenable<U>; }
}
",
        &[
            "try : { <R>(fn: () => Promise.Thenable<R>): Promise<R>; <R_1>(fn: () => R_1): Promise<R_1>; }",
            "try : { <R_1>(fn: () => Promise.Thenable<R_1>): Promise<R_1>; <R>(fn: () => R): Promise<R>; }",
        ],
    );
}

/// A bare type parameter takes the next free name where its written name
/// resolves to a different type parameter at the assertion's parent.
#[test]
fn shadowed_parameters_are_renamed_at_the_site() {
    expect(
        r"export function needsRenameForShadowing<T>() {
  type A = T
  return function O<T>(t: A, t2: T) {
  }
}
",
        &["O : <T>(t: T_1, t2: T) => void", "t : T_1", "t2 : T"],
    );
}

/// An arrow's concise body is contextually typed by the written return
/// annotation before any contextual signature, and an alias of a conditional
/// alias stays generic, so the body keeps its declared type.
#[test]
fn concise_bodies_read_the_return_annotation() {
    expect(
        r"var dot: <T, S>(f: (_: T) => S) => <U>(g: (_: U) => T) => (_: U) => S;
dot = <T, S>(f: (_: T) => S) => <U>(g: (_: U) => T): (r:U) => S => (x) => f(g(x));
type Foo<T> = T extends string ? boolean : number;
type Baz<T> = Foo<T>;
const convert2 = <T>(value: Foo<T>): Baz<T> => value;
",
        &["x : U", "value : Foo<T>"],
    );
    let lines = lines(
        r"type Foo<T> = T extends string ? boolean : number;
type Baz<T> = Foo<T>;
const convert2 = <T>(value: Foo<T>): Baz<T> => value;
",
    );
    assert!(!lines.iter().any(|line| line == "value : number | boolean"), "{lines:?}");
}
