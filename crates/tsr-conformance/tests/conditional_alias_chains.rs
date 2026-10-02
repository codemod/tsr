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
