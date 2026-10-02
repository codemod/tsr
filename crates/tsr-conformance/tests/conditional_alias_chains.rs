//! Conditional alias chains and definite conditional outcomes, checked against
//! pinned tsgo 5b1047d1 declaration output
//! (docs/architecture/checker-99-conditional-chains.md).
use tsr_conformance::{TestCase, types_baseline::FileTypes, types_producer};

fn lines(source: &str) -> Vec<String> {
    let case = TestCase::parse("probe/conditional-chains", "chains.ts", source);
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
    for wanted in wanted {
        assert!(lines.iter().any(|line| line == wanted), "missing {wanted}: {lines:?}");
    }
}

const CHAINS: &str = r#"// @strict: true
type Extends<T, U> = T extends U ? true : false;
type If<C extends boolean, T, F> = C extends true ? T : F;
type Not<C extends boolean> = If<C, false, true>;
type IsString<T> = Extends<T, string>;
type Defaulted<T, U = string> = Extends<T, U>;
type ViaDefault<T> = Defaulted<T>;
declare const q1: IsString<number>;
declare const n3: Not<boolean>;
type N3 = Not<boolean>;
type N1 = Not<false>;
declare const d1: ViaDefault<"x">;
declare function h(y: IsString<"a">): IsString<"a">;
"#;

#[test]
fn an_alias_of_a_conditional_alias_evaluates_through_the_chain() {
    expect(
        CHAINS,
        &["q1 : false", "N1 : true", "d1 : true", "h : (y: IsString<\"a\">) => IsString<\"a\">"],
    );
}

#[test]
fn only_an_alias_declared_position_names_a_distributed_chain_result() {
    // getTypeFromTypeAliasReference passes the new alias symbol only for an
    // alias body; elsewhere the distributed union has no alias.
    expect(CHAINS, &["n3 : boolean", "N3 : N3"]);
}

#[test]
fn a_generic_extends_type_defers_unless_a_definite_outcome_exists() {
    expect(
        r"// @strict: true
type U = { name: 'a'; c: 1 } | { name: 'b'; c: 2 };
type DiscriminateUnion<T, K extends keyof T, V extends T[K]> = T extends Record<K, V> ? T : never;
type WithName<T extends U['name']> = DiscriminateUnion<U, 'name', T>;
type W5<T extends 'a' | 'b'> = { name: 'a' } extends { name: T } ? 1 : 0;
type W7<T extends 'a' | 'b'> = { name: 'a' } extends Record<'name', T> ? 1 : 0;
type Never<T> = string extends { name: T } ? 1 : 0;
declare function g<T extends 'a' | 'b'>(a: WithName<T>, b: W5<T>, c: W7<T>, d: Never<T>): void;
",
        &["a : WithName<T>", "b : W5<T>", "c : W7<T>", "d : 0"],
    );
}

#[test]
fn a_record_alias_reference_enumerates_its_literal_keys() {
    expect(
        r"// @strict: true
type Z2<T> = { name: 'a' } extends Record<'name', T> ? 1 : 0;
declare const z3: Z2<'b'>;
declare const z4: Z2<'a'>;
",
        &["z3 : 0", "z4 : 1"],
    );
}

#[test]
fn a_non_generic_conditional_node_evaluates_where_it_is_written() {
    // getTypeFromConditionalTypeNode resolves through getConditionalType with
    // no mapper; written annotations still reuse their conditional text.
    expect(
        r"// @strict: true
type Z = { name: 'a' } extends Record<'name', 'b'> ? 1 : 0;
type A = any extends number ? 1 : 0;
type B = never extends never ? true : false;
type N = number extends never ? true : false;
type C = [any] extends [number] ? 1 : 0;
declare let x: number extends string ? 1 : 0;
declare function f(p: number extends string ? 1 : 0): void;
type I = [1, 2, 3] extends [infer H, ...unknown[]] ? H : never;
",
        &[
            "Z : 0",
            "A : 0 | 1",
            "B : true",
            "N : false",
            "C : 1",
            "x : 0",
            "f : (p: number extends string ? 1 : 0) => void",
            "I : 1",
        ],
    );
}

#[test]
fn exact_optional_properties_relate_without_their_missing_type() {
    // strictOptionalProperties2: an explicit undefined does not relate to the
    // target's missing type (getNonMissingTypeOfSymbol on both sides).
    expect(
        r"// @strict: true
// @exactOptionalPropertyTypes: true
type T1 = { 0?: string | undefined } extends { 0?: string } ? true : false;
type T3 = { 0?: string } extends { 0?: string | undefined } ? true : false;
",
        &["T1 : false", "T3 : true"],
    );
}

#[test]
fn an_invalid_forward_default_fills_with_the_error_type() {
    // fillMissingTypeArguments maps unfilled positions to errorType before
    // instantiating each default (TS2744); upstream prints it as `any`.
    expect(
        r"// @strict: true
declare const x: any;
interface i05<T = T> { a: T; }
const i05c00 = (<i05>x).a;
interface i06<T = U, U = T> { a: [T, U]; }
const i06c00 = (<i06>x).a;
const i06c01 = (<i06<number>>x).a;
interface i08<T, U = V, V = number> { a: [T, U, V]; }
const i08c00 = (<i08<string>>x).a;
",
        &[
            "(<i05>x) : i05<any>",
            "i06c00 : [any, any]",
            "(<i06>x) : i06<any, any>",
            "i06c01 : [number, number]",
            "i08c00 : [string, any, number]",
        ],
    );
}

#[test]
fn a_recursive_defaulted_alias_still_terminates() {
    // infiniteConstraints: `Conv<T, U = T>` refers to itself through a
    // partial reference whose default names an earlier parameter.
    expect(
        r"// @strict: true
export type Prepend<Elm, T extends unknown[]> =
  T extends unknown ?
  ((arg: Elm, ...rest: T) => void) extends ((...args: infer T2) => void) ? T2 :
  never :
  never;
export type ExactExtract<T, U> = T extends U ? U extends T ? T : never : never;
type Conv<T, U = T> =
  { 0: [T]; 1: Prepend<T, Conv<ExactExtract<U, T>>>;}[U extends T ? 0 : 1];
",
        &["Conv : Conv<T, U>"],
    );
}
