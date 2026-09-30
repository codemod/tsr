//! Conditional inference controls compared with pinned tsgo declarations.
use tsr_conformance::{TestCase, types_baseline::FileTypes, types_producer};
#[test]
fn conditional_infer_keeps_parameter_identity_and_literal_candidates() {
    let source = r#"// @strict: true
// @target: es2015
type Element<T> = T extends readonly (infer U)[] ? U : never;
type Tail<T> = T extends readonly [unknown, ...infer U] ? U : never;
type Head<T> = T extends readonly [infer U, ...unknown[]] ? U : never;
type Both<T> = T extends { a: infer U, b: infer U } ? U : never;
type Input<T> = T extends (value: infer U) => void ? U : never;
type E = Element<readonly ("a" | "b")[]>;
declare const element: E;
type R = Tail<readonly [1, "b", true]>;
declare const tail: R;
type H = Head<readonly [1, "b", true]>;
declare const head: H;
type B = Both<{ a: "a", b: "b" }>;
declare const both: B;
type I = Input<(value: "a") => void>;
declare const input: I;
export function result() { return { element, tail, head, both, input }; }
"#;
    let case = TestCase::parse("probe/conditional-infer", "conditional-infer.ts", source);
    let expected: Vec<_> = case
        .files
        .iter()
        .map(|unit| FileTypes { file: unit.name.clone(), assertions: Vec::new() })
        .collect();
    let assertions = types_producer::assertions_for_case(&case, &expected, false);
    let lines: Vec<_> = assertions.iter().flatten().map(types_producer::Assertion::line).collect();
    for wanted in [
        "element : \"a\" | \"b\"",
        "R : [\"b\", true]",
        "head : 1",
        "both : \"a\" | \"b\"",
        "input : \"a\"",
    ] {
        assert!(lines.iter().any(|line| line == wanted), "missing {wanted}: {lines:?}");
    }
}

#[test]
fn conditional_infer_uses_strict_contravariance_even_when_function_types_are_nonstrict() {
    let source = r#"// @strict: false
// @target: es2015
type Common<T> = T extends { a: (x: infer U) => void; b: (x: infer U) => void } ? U : never;
type C = Common<{ a: (x: "a" | "b") => void; b: (x: "b" | "c") => void }>;
declare const common: C;
export function result() { return common; }
"#;
    let case = TestCase::parse(
        "probe/conditional-infer-contravariance",
        "conditional-infer-contra.ts",
        source,
    );
    let expected: Vec<_> = case
        .files
        .iter()
        .map(|unit| FileTypes { file: unit.name.clone(), assertions: Vec::new() })
        .collect();
    let assertions = types_producer::assertions_for_case(&case, &expected, false);
    let lines: Vec<_> = assertions.iter().flatten().map(types_producer::Assertion::line).collect();
    assert!(lines.iter().any(|line| line == "C : \"b\""), "{lines:?}");
}

#[test]
fn an_empty_concrete_rest_slice_does_not_contribute_never_to_inference() {
    let source = r#"// @strict: true
// @target: es2015
type RestElements<T> = T extends readonly [unknown, ...(infer U)[]] ? U : never;
type Empty = RestElements<[1]>;
type Populated = RestElements<[1, "a", "b"]>;
declare const empty: Empty;
declare const populated: Populated;
export function result() { return { empty, populated }; }
"#;
    let case =
        TestCase::parse("probe/conditional-infer-empty-rest", "conditional-infer-empty.ts", source);
    let expected: Vec<_> = case
        .files
        .iter()
        .map(|unit| FileTypes { file: unit.name.clone(), assertions: Vec::new() })
        .collect();
    let assertions = types_producer::assertions_for_case(&case, &expected, false);
    let lines: Vec<_> = assertions.iter().flatten().map(types_producer::Assertion::line).collect();
    for wanted in ["Empty : unknown", "Populated : \"a\" | \"b\""] {
        assert!(lines.iter().any(|line| line == wanted), "missing {wanted}: {lines:?}");
    }
}
