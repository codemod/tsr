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

#[test]
fn infer_constraints_filter_candidates_and_nested_conditionals_continue_with_the_mapper() {
    let source = r#"// @strict: true
// @target: es2015
type Classify<T> = T extends [infer U extends string] ? ["string", U] : T extends [infer U extends number] ? ["number", U] : never;
type Merge<T> = T extends { a: infer U, b: infer U extends string } ? U : never;
type Outer<T, C> = T extends [infer U extends C] ? U : never;
type S = Classify<["a"]>;
type N = Classify<[1]>;
type F = Classify<[object]>;
type M = Merge<{ a: "a", b: "b" }>;
type MF = Merge<{ a: "a", b: 1 }>;
type O = Outer<["a"], string>;
type OF = Outer<[1], string>;
declare const stringResult: S;
declare const numberResult: N;
declare const failed: F;
declare const merged: M;
declare const mergeFailed: MF;
declare const outer: O;
declare const outerFailed: OF;
export function result() { return { stringResult, numberResult, failed, merged, mergeFailed, outer, outerFailed }; }
"#;
    let case = TestCase::parse("probe/infer-constraints", "infer-constraints.ts", source);
    let expected: Vec<_> = case
        .files
        .iter()
        .map(|unit| FileTypes { file: unit.name.clone(), assertions: Vec::new() })
        .collect();
    let assertions = types_producer::assertions_for_case(&case, &expected, false);
    let lines: Vec<_> = assertions.iter().flatten().map(types_producer::Assertion::line).collect();
    for wanted in [
        "S : [\"string\", \"a\"]",
        "N : [\"number\", 1]",
        "F : never",
        "M : \"a\" | \"b\"",
        "MF : never",
        "O : \"a\"",
        "OF : never",
    ] {
        assert!(lines.iter().any(|line| line == wanted), "missing {wanted}: {lines:?}");
    }
}

#[test]
fn non_strict_nullable_extends_continues_into_nested_inference() {
    let source = r#"// @strict: false
// @target: es2015
type Box<T> = { value: T };
type Unwrap<T> = T extends null | undefined ? T : T extends Box<infer U> ? U : T;
type Parenthesized<T> = T extends (null | undefined) ? T : T extends Box<infer U> ? U : T;
type TwiceParenthesized<T> = T extends ((null | undefined)) ? T : T extends Box<infer U> ? U : T;
type Primitive = Unwrap<number>;
type Nested = Unwrap<Box<"value">>;
type Nullable = Unwrap<undefined>;
type ParenthesizedPrimitive = Parenthesized<number>;
type ParenthesizedNested = Parenthesized<Box<"value">>;
type TwiceParenthesizedPrimitive = TwiceParenthesized<number>;
declare const primitive: Primitive;
declare const nested: Nested;
declare const nullable: Nullable;
declare const parenthesizedPrimitive: ParenthesizedPrimitive;
declare const parenthesizedNested: ParenthesizedNested;
declare const twiceParenthesizedPrimitive: TwiceParenthesizedPrimitive;
export function result() { return { primitive, nested, nullable, parenthesizedPrimitive, parenthesizedNested, twiceParenthesizedPrimitive }; }
"#;
    let case = TestCase::parse(
        "probe/non-strict-nullable-conditional",
        "non-strict-nullable-conditional.ts",
        source,
    );
    let expected: Vec<_> = case
        .files
        .iter()
        .map(|unit| FileTypes { file: unit.name.clone(), assertions: Vec::new() })
        .collect();
    let assertions = types_producer::assertions_for_case(&case, &expected, false);
    let lines: Vec<_> = assertions.iter().flatten().map(types_producer::Assertion::line).collect();
    for wanted in [
        "primitive : number",
        "nested : \"value\"",
        "nullable : undefined",
        "parenthesizedPrimitive : number",
        "parenthesizedNested : \"value\"",
        "twiceParenthesizedPrimitive : number",
    ] {
        assert!(lines.iter().any(|line| line == wanted), "missing {wanted}: {lines:?}");
    }
}

#[test]
fn strict_nullable_extends_keeps_ordinary_union_semantics_through_parentheses() {
    let source = r#"// @strict: true
// @target: es2015
type Box<T> = { value: T };
type Parenthesized<T> = T extends (null | undefined) ? T : T extends Box<infer U> ? U : T;
type TwiceParenthesized<T> = T extends ((null | undefined)) ? T : T extends Box<infer U> ? U : T;
type Primitive = Parenthesized<number>;
type Nested = TwiceParenthesized<Box<"value">>;
type Nullable = Parenthesized<undefined>;
declare const primitive: Primitive;
declare const nested: Nested;
declare const nullable: Nullable;
export function result() { return { primitive, nested, nullable }; }
"#;
    let case = TestCase::parse(
        "probe/strict-parenthesized-nullable-conditional",
        "strict-parenthesized-nullable-conditional.ts",
        source,
    );
    let expected: Vec<_> = case
        .files
        .iter()
        .map(|unit| FileTypes { file: unit.name.clone(), assertions: Vec::new() })
        .collect();
    let assertions = types_producer::assertions_for_case(&case, &expected, false);
    let lines: Vec<_> = assertions.iter().flatten().map(types_producer::Assertion::line).collect();
    for wanted in ["primitive : number", "nested : \"value\"", "nullable : undefined"] {
        assert!(lines.iter().any(|line| line == wanted), "missing {wanted}: {lines:?}");
    }
}

#[test]
fn conditional_distribution_maps_each_union_constituent_and_preserves_tuple_wrapping() {
    let source = r#"// @strict: true
// @target: es2015
type Filter<T> = T extends string ? T : never;
type NonDistributive<T> = [T] extends [string] ? T : never;
type Element<T> = T extends readonly (infer U)[] ? U : never;
type F = Filter<"a" | 1 | "b">;
type N = NonDistributive<"a" | 1>;
type E = Element<readonly "a"[] | readonly 1[]>;
type Z = Filter<never>;
type S = Filter<"a" | 1>;
type SE = Filter<E>;
declare const filtered: F;
declare const boxed: N;
declare const element: E;
declare const absent: Z;
declare const single: S;
declare const stringElement: SE;
export function result() { return { filtered, boxed, element, absent, single, stringElement }; }
"#;
    let case =
        TestCase::parse("probe/conditional-distribution", "conditional-distribution.ts", source);
    let expected: Vec<_> = case
        .files
        .iter()
        .map(|unit| FileTypes { file: unit.name.clone(), assertions: Vec::new() })
        .collect();
    let assertions = types_producer::assertions_for_case(&case, &expected, false);
    let lines: Vec<_> = assertions.iter().flatten().map(types_producer::Assertion::line).collect();
    for wanted in ["F : F", "N : never", "E : E", "Z : never", "S : \"a\"", "SE : \"a\""] {
        assert!(lines.iter().any(|line| line == wanted), "missing {wanted}: {lines:?}");
    }
}

#[test]
fn implicit_reference_constraints_filter_candidates_and_discard_self_constraints() {
    let source = r#"// @strict: true
// @target: es2015
type Box<T extends string> = { value: T };
type Pair<T extends string, U extends T> = { a: T, b: U };
type FromBox<T> = T extends Box<infer U> ? U : never;
type Same<T> = T extends Pair<infer U, infer U> ? U : never;
type Good = FromBox<{ value: "a" }>;
type Bad = FromBox<{ value: 1 }>;
type Merged = Same<{ a: "a", b: "b" }>;
declare const good: Good;
declare const bad: Bad;
declare const merged: Merged;
export function result() { return { good, bad, merged }; }
"#;
    let case = TestCase::parse(
        "probe/implicit-infer-constraints",
        "implicit-infer-constraints.ts",
        source,
    );
    let expected: Vec<_> = case
        .files
        .iter()
        .map(|unit| FileTypes { file: unit.name.clone(), assertions: Vec::new() })
        .collect();
    let assertions = types_producer::assertions_for_case(&case, &expected, false);
    let lines: Vec<_> = assertions.iter().flatten().map(types_producer::Assertion::line).collect();
    for wanted in ["Good : \"a\"", "Bad : never", "Merged : \"a\" | \"b\""] {
        assert!(lines.iter().any(|line| line == wanted), "missing {wanted}: {lines:?}");
    }
}

#[test]
fn any_checks_include_both_branches_except_for_any_or_unknown_extends() {
    let source = r"// @strict: true
// @target: es2015
type Select<T> = T extends string ? 1 : 2;
type Unknown<T> = T extends unknown ? 1 : 2;
type Any<T> = T extends any ? 1 : 2;
type Never<T> = T extends never ? 1 : 2;
type Nested<T> = T extends string ? 3 : T extends number ? 4 : 5;
type S = Select<any>;
type U = Unknown<any>;
type A = Any<any>;
type N = Never<any>;
type D = Nested<any>;
declare const selected: S;
declare const unknown: U;
declare const any: A;
declare const never: N;
declare const nested: D;
export function result() { return { selected, unknown, any, never, nested }; }
";
    let case = TestCase::parse("probe/any-conditional", "any-conditional.ts", source);
    let expected: Vec<_> = case
        .files
        .iter()
        .map(|unit| FileTypes { file: unit.name.clone(), assertions: Vec::new() })
        .collect();
    let assertions = types_producer::assertions_for_case(&case, &expected, false);
    let lines: Vec<_> = assertions.iter().flatten().map(types_producer::Assertion::line).collect();
    for wanted in ["S : 1 | 2", "U : 1", "A : 1", "N : 1 | 2", "D : 3 | 4 | 5"] {
        assert!(lines.iter().any(|line| line == wanted), "missing {wanted}: {lines:?}");
    }
}

#[test]
fn dependent_constraints_use_the_other_parameters_inference_before_branch_selection() {
    let source = r#"// @strict: true
// @target: es2015
type Pair<T, U extends T> = { a: T, b: U };
type StringPair<T extends string, U extends T> = { a: T, b: U };
type ExtractPair<T> = T extends Pair<infer A, infer B> ? [A, B] : never;
type ExtractStringPair<T> = T extends StringPair<infer A, infer B> ? [A, B] : never;
type Good = ExtractPair<{ a: string, b: "b" }>;
type Bad = ExtractPair<{ a: "a", b: "b" }>;
type Refined = ExtractStringPair<{ a: 1, b: 1 }>;
declare const good: Good;
declare const bad: Bad;
declare const refined: Refined;
export function result() { return { good, bad, refined }; }
"#;
    let case = TestCase::parse(
        "probe/dependent-infer-constraints",
        "dependent-infer-constraints.ts",
        source,
    );
    let expected: Vec<_> = case
        .files
        .iter()
        .map(|unit| FileTypes { file: unit.name.clone(), assertions: Vec::new() })
        .collect();
    let assertions = types_producer::assertions_for_case(&case, &expected, false);
    let lines: Vec<_> = assertions.iter().flatten().map(types_producer::Assertion::line).collect();
    for wanted in ["Good : [string, \"b\"]", "Bad : never", "Refined : never"] {
        assert!(lines.iter().any(|line| line == wanted), "missing {wanted}: {lines:?}");
    }
}

#[test]
fn any_infer_checks_union_branches_after_applying_the_inference_mapper() {
    let source = r"// @strict: true
// @target: es2015
type Whole<T> = T extends infer U ? U : 0;
type Element<T> = T extends (infer U)[] ? U : 0;
type Input<T> = T extends (...args: infer U) => void ? U : 0;
type Property<T> = T extends { value: infer U } ? U : 0;
type W = Whole<any>;
type E = Element<any>;
type I = Input<any>;
type P = Property<any>;
declare const whole: W;
declare const element: E;
declare const input: I;
declare const property: P;
export function result() { return { whole, element, input, property }; }
";
    let case = TestCase::parse("probe/any-infer", "any-infer.ts", source);
    let expected: Vec<_> = case
        .files
        .iter()
        .map(|unit| FileTypes { file: unit.name.clone(), assertions: Vec::new() })
        .collect();
    let assertions = types_producer::assertions_for_case(&case, &expected, false);
    let lines: Vec<_> = assertions.iter().flatten().map(types_producer::Assertion::line).collect();
    for wanted in ["W : any", "E : unknown", "I : 0 | unknown[]", "P : unknown"] {
        assert!(lines.iter().any(|line| line == wanted), "missing {wanted}: {lines:?}");
    }
}
