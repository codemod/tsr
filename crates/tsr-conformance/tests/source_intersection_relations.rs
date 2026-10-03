//! Source-intersection controls checked against pinned tsgo 5b1047d1.
use tsr_conformance::{TestCase, diagnostics_suite, types_baseline::FileTypes, types_producer};

fn expect(source: &str, wanted: &[&str]) {
    let case = TestCase::parse("probe/source-intersection", "fixture.ts", source);
    let files: Vec<_> = case
        .files
        .iter()
        .map(|unit| FileTypes { file: unit.name.clone(), assertions: Vec::new() })
        .collect();
    let lines: Vec<_> = types_producer::assertions_for_case(&case, &files, false)
        .iter()
        .flatten()
        .map(types_producer::Assertion::line)
        .collect();
    for wanted in wanted {
        assert!(lines.iter().any(|line| line == wanted), "missing {wanted}: {lines:?}");
    }
}

const STRUCTURE: &str = r#"
interface State { a: { b: string; c: number }; d: boolean }
declare const concrete: { d: boolean } & { a: { b: string; c: number } };
declare const named: Pick<State, "d"> & { a: { b: string; c: number } };
declare const nested: { d: boolean; a: { c: number } & { b: string } };
export const concreteCheck: State = concrete;
export const namedCheck: State = named;
export const nestedCheck: State = nested;
type Rel<S, T> = S extends T ? true : false;
declare const forward: Rel<typeof concrete, State>;
declare const ordered: Rel<{ a: { c: number } & { b: string } } & { d: boolean }, State>;
declare const missing: Rel<{ a: { b: string } } & { d: boolean }, State>;
declare const wrong: Rel<{ d: boolean } & { a: { b: number; c: string } }, State>;
declare const extra: Rel<typeof concrete & { tag: "extra" }, State>;
declare const reverse: Rel<State, typeof concrete & { tag: "extra" }>;
export function structureChecks() { return [forward, ordered, missing, wrong, extra, reverse] as const; }
"#;

#[test]
fn disjoint_and_nested_members_combine_without_losing_direction_or_value_types() {
    for exact in [false, true] {
        let source = format!(
            "// @strict: true\n// @target: es2015\n// @exactOptionalPropertyTypes: {exact}\n{STRUCTURE}"
        );
        expect(
            &source,
            &["structureChecks : () => readonly [true, true, false, false, true, false]"],
        );
        let case = TestCase::parse("probe/source-intersection", "fixture.ts", &source);
        assert!(diagnostics_suite::reported_for(&case).is_empty());
    }
}

const OPTIONALS: &str = r#"
type Rel<S, T> = S extends T ? true : false;
interface RequiredTarget { a: any; d: boolean }
interface Optional { a?: string; d: boolean }
declare const optionalToRequired: Rel<{ a?: string } & { d: boolean }, RequiredTarget>;
declare const requiredWins: Rel<{ a?: string } & { a: "ok" } & { d: boolean }, RequiredTarget>;
declare const requiredWinsReversed: Rel<{ a: "ok" } & { a?: string } & { d: boolean }, RequiredTarget>;
declare const compatible: Rel<{ d: boolean } & { a?: string }, Optional>;
declare const incompatible: Rel<{ d: boolean } & { a?: number }, Optional>;
declare const incompatibleReversed: Rel<{ a?: number } & { d: boolean }, Optional>;
declare const explicitUndefined: Rel<{ d: boolean } & { a?: string | undefined }, Optional>;
export function optionalChecks() { return [optionalToRequired, requiredWins, requiredWinsReversed, compatible, incompatible, incompatibleReversed, explicitUndefined] as const; }
interface Box<T> { a?: T; d: boolean }
declare const referenceGood: Rel<{ d: boolean } & { a: 37 }, Box<number>>;
declare const referenceBad: Rel<{ d: boolean } & { a: number }, Box<string>>;
export function referenceChecks() { return [referenceGood, referenceBad] as const; }
"#;

#[test]
fn optional_contributions_are_anded_and_optional_targets_recheck_the_whole_source() {
    for (exact, last) in [(false, "true"), (true, "false")] {
        let source =
            format!("// @strict: true\n// @exactOptionalPropertyTypes: {exact}\n{OPTIONALS}");
        expect(
            &source,
            &[
                &format!(
                    "optionalChecks : () => readonly [false, true, true, true, false, false, {last}]"
                ),
                "referenceChecks : () => readonly [true, false]",
            ],
        );
    }
}

#[test]
fn combined_members_keep_missing_required_and_bad_values_as_diagnostics() {
    for exact in [false, true] {
        let source = format!(
            r"// @strict: true
// @exactOptionalPropertyTypes: {exact}
interface State {{ a: {{ b: string; c: number }}; d: boolean }}
interface Optional {{ a?: string; d: boolean }}
declare const missing: {{ a: {{ b: string }} }} & {{ d: boolean }};
declare const wrong: {{ a: {{ b: number; c: string }} }} & {{ d: boolean }};
declare const badOptional: {{ d: boolean }} & {{ a: number }};
const missingCheck: State = missing;
const wrongCheck: State = wrong;
const optionalCheck: Optional = badOptional;
"
        );
        let case = TestCase::parse("probe/source-intersection-negative", "fixture.ts", &source);
        let mut actual: Vec<_> = diagnostics_suite::reported_for(&case)
            .into_iter()
            .map(|diagnostic| (diagnostic.line, diagnostic.column, diagnostic.code))
            .collect();
        actual.sort_unstable();
        assert_eq!(actual, [(6, 7, 2322), (7, 7, 2322), (8, 7, 2322)], "exact={exact}");
    }
}

#[test]
fn generic_source_intersections_keep_the_existing_constraint_inference_boundary() {
    expect(
        r"// @strict: true
// @target: es2015
// @lib: esnext
interface First { commonProperty: number }
interface Second { commonProperty: number }
const first = <T extends First | Second>(value: T) => {
    const withOther: T & { otherProperty: number } = Object.assign(value, { otherProperty: 3 });
    second(withOther);
};
const second = <T extends { commonProperty: number; otherProperty: number }>(value: T) => value;
",
        &["second(withOther) : { commonProperty: number; otherProperty: number; }"],
    );
}

#[test]
fn object_bounded_intersections_prove_combined_members_and_keep_inferred_candidates() {
    for exact in [false, true] {
        let source = format!(
            r#"// @strict: true
// @target: es2015
// @exactOptionalPropertyTypes: {exact}
interface State {{ a: string; d: boolean }}
const identity = <T extends {{ a: string; d: boolean }}>(value: T) => value;
export function bounded<T extends {{ a: string; tag: number }}>(value: T & {{ d: boolean }}) {{
    const check: State = value;
    return identity(value);
}}
export function reversed<T extends {{ a: string; tag: number }}>(value: {{ d: boolean }} & T) {{
    const check: State = value;
    return identity(value);
}}
export function requiredWins<T extends {{ a?: string }}>(value: T & {{ a: "ok" }} & {{ d: boolean }}) {{
    const check: State = value;
    return check;
}}
"#
        );
        expect(
            &source,
            &["identity(value) : T & { d: boolean; }", "identity(value) : { d: boolean; } & T"],
        );
        let case = TestCase::parse("probe/bounded-intersection", "fixture.ts", &source);
        assert!(diagnostics_suite::reported_for(&case).is_empty(), "exact={exact}");
    }
}

#[test]
fn object_bounds_do_not_erase_missing_values_or_optional_source_requirements() {
    for exact in [false, true] {
        let source = format!(
            r"// @strict: true
// @exactOptionalPropertyTypes: {exact}
interface State {{ a: string; d: boolean }}
interface RequiredAny {{ a: any; d: boolean }}
function missing<T extends {{ tag: number }}>(value: T & {{ d: boolean }}) {{ const check: State = value; }}
function wrong<T extends {{ a: number }}>(value: T & {{ d: boolean }}) {{ const check: State = value; }}
function reversed<T extends {{ a: number }}>(value: {{ d: boolean }} & T) {{ const check: State = value; }}
function optional<T extends {{ a?: string }}>(value: T & {{ d: boolean }}) {{ const check: RequiredAny = value; }}
"
        );
        let case = TestCase::parse("probe/bounded-intersection-negative", "fixture.ts", &source);
        let mut actual: Vec<_> = diagnostics_suite::reported_for(&case)
            .into_iter()
            .map(|diagnostic| (diagnostic.line, diagnostic.column, diagnostic.code))
            .collect();
        actual.sort_unstable();
        assert_eq!(
            actual,
            [(3, 80, 2322), (4, 76, 2322), (5, 79, 2322), (6, 80, 2322)],
            "exact={exact}"
        );
    }
}

#[test]
fn unsupported_combined_members_do_not_accept_an_inference_candidate() {
    expect(
        r"// @strict: true
// @target: es2015
class NominalState { protected a!: string; d!: boolean }
const identity = <T extends NominalState>(value: T) => value;
export function unknownProof<T extends { a: string }>(value: T & { d: boolean }) {
    return identity(value);
}
",
        &["identity(value) : NominalState"],
    );
}
