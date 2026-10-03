//! Native inferToConditionalType controls, independent of Awaited's spelling.
use tsr_conformance::{TestCase, types_baseline::FileTypes, types_producer};

fn expect(source: &str, wanted: &[&str]) {
    let case = TestCase::parse("probe/conditional-target", "conditional-target.ts", source);
    let expected: Vec<_> = case
        .files
        .iter()
        .map(|unit| FileTypes { file: unit.name.clone(), assertions: Vec::new() })
        .collect();
    let lines: Vec<_> = types_producer::assertions_for_case(&case, &expected, false)
        .iter()
        .flatten()
        .map(types_producer::Assertion::line)
        .collect();
    for wanted in wanted {
        assert!(lines.iter().any(|line| line == wanted), "missing {wanted}: {lines:?}");
    }
}

#[test]
fn awaited_context_retains_only_the_matching_argument_literal() {
    expect(
        r"// @strict: true
// @target: es2015
function matched(): Promise<true> { return Promise.resolve(true); }
function opposite(): Promise<true> { return Promise.resolve(false); }
const free = Promise.resolve(false);
",
        &[
            "Promise.resolve(true) : Promise<true>",
            "Promise.resolve(false) : Promise<boolean>",
            "free : Promise<boolean>",
        ],
    );
}

#[test]
fn conditional_branches_prefer_structured_inference_to_a_naked_parameter() {
    expect(
        r#"// @strict: true
// @target: es2015
interface Wrap<T> { value: T }
type Select<T> = T extends string ? { tag: T } : T;
declare function select<T>(input: T): Wrap<Select<T>>;
function structured(): Wrap<{ tag: "held" }> { return select("held"); }
function naked(): Wrap<true> { return select(true); }
const free = select("free");
"#,
        &[
            "select(\"held\") : Wrap<{ tag: \"held\"; }>",
            "select(true) : Wrap<true>",
            "free : Wrap<{ tag: string; }>",
        ],
    );
}

#[test]
fn nested_infer_branches_keep_the_outer_mapper_without_special_alias_names() {
    expect(
        r#"// @strict: true
// @target: es2015
interface Wrap<T> { value: T }
type Unwrap<T> = T extends null | undefined ? T : T extends { item: infer U } ? U : T;
type Indirect<T> = Unwrap<T>;
declare function unwrap<T>(input: T): Wrap<Unwrap<T>>;
declare function indirect<T>(input: T): Wrap<Indirect<T>>;
function contextual(): Wrap<"held"> { return unwrap("held"); }
function chained(): Wrap<7> { return indirect(7); }
const free = unwrap("free");
"#,
        &["unwrap(\"held\") : Wrap<\"held\">", "indirect(7) : Wrap<7>", "free : Wrap<string>"],
    );
}

#[test]
fn a_constant_false_result_does_not_overwrite_the_nested_argument_candidate() {
    // Both calls return Wrap<false>, which cannot distinguish their argument
    // candidates. Contextual inference must retain false, while without a return
    // context, nested candidate inference keeps true (getCovariantInference
    // widens only top-level candidates), rather than substituting false.
    expect(
        r"// @strict: true
// @target: es2015
interface Wrap<T> { value: T }
type Select<T> = T extends number ? T : false;
declare function select<T>(input: { kept: T }): Wrap<Select<T>>;
function contextual(): Wrap<false> { return select({ kept: false }); }
const free = select({ kept: true });
",
        &["{ kept: false } : { kept: false; }", "{ kept: true } : { kept: true; }"],
    );
}

#[test]
fn matching_deferred_references_keep_their_generic_argument_identity() {
    expect(
        r"// @strict: true
// @target: es2015
type Select<T> = T extends string ? { yes: T } : { no: T };
declare function extract<T>(input: Select<T>): T;
function generic<U>(input: Select<U>) { return extract(input); }
",
        &["extract(input) : U", "generic : <U>(input: Select<U>) => U"],
    );
}

#[test]
fn conditional_top_level_candidates_widen_through_exactly_three_branches() {
    // Pinned wideningWithTopLevelTypeParameter: depth three still counts as
    // top level; depth four deliberately retains the literal candidate.
    expect(
        r#"// @strict: true
// @target: es2015
type C1<T> = T extends unknown ? T | undefined : never;
type C2<T> = T extends unknown ? T | undefined : never;
type C3<T> = T extends unknown ? T | undefined : never;
type C4<T> = T extends unknown ? T | undefined : never;
declare function one<T>(input: C1<T>): [T];
declare function three<T>(input: C1<C2<C3<T>>>): [T];
declare function four<T>(input: C1<C2<C3<C4<T>>>>): [T];
const a = one(7);
const b = three("branch");
const c = four(23);
"#,
        &["one(7) : [number]", "three(\"branch\") : [string]", "four(23) : [23]"],
    );
}

#[test]
fn conditional_sources_match_all_four_operands_across_aliases_and_inline_nodes() {
    // Fresh pinned tsgo 5b1047d1 conditionalOperandsWave4.types: none of
    // A/B can be inferred from either branch; C/D must retain their positions.
    expect(
        r"// @strict: true
// @target: es2015
type Source<A, B, C, D> = A extends B ? C : D;
type Target<E, F, G, H> = E extends F ? G : H;
declare function match<E, F, G, H>(input: Target<E, F, G, H>): [E, F, G, H];
declare function inlineMatch<E, F, G, H>(input: E extends F ? G : H): [E, F, G, H];
export function fromAlias<A, B, C, D>(alias: Source<A, B, C, D>) { return match(alias); }
export function fromNode<A, B, C, D>(node: A extends B ? C : D) { return match(node); }
export function toNode<A, B, C, D>(alias: Source<A, B, C, D>) { return inlineMatch(alias); }
",
        &[
            "match(alias) : [A, B, C, D]",
            "match(node) : [A, B, C, D]",
            "inlineMatch(alias) : [A, B, C, D]",
        ],
    );
}

#[test]
fn conditional_operand_matching_preserves_asymmetric_structured_branch_positions() {
    // Pinned native returns [A, B, C, D] and [A, B, D, C], respectively.
    // A branch-union walk or a swapped true/false pairing cannot infer this.
    expect(
        r"// @strict: true
// @target: es2015
type Source<A, B, C, D> = A extends { tag: B } ? [C, D] : { x: D; y: C };
type Target<E, F, G, H> = E extends { tag: F } ? [G, H] : { x: H; y: G };
declare function match<E, F, G, H>(input: Target<E, F, G, H>): [E, F, G, H];
export function structured<A, B, C, D>(value: Source<A, B, C, D>) { return match(value); }
type Swapped<A, B, C, D> = A extends { tag: B } ? [D, C] : { x: C; y: D };
export function swapped<A, B, C, D>(other: Swapped<A, B, C, D>) { return match(other); }
",
        &["match(value) : [A, B, C, D]", "match(other) : [A, B, D, C]"],
    );
}

#[test]
fn constant_conditional_branches_do_not_hide_check_and_extends_inference() {
    // A/B occur only in the relation operands, never in the constant branches.
    expect(
        r"// @strict: true
// @target: es2015
type Source<A, B> = A extends B ? 11 : false;
type Target<E, F> = E extends F ? 11 : false;
declare function match<E, F>(input: Target<E, F>): [E, F];
export function constants<A, B>(value: Source<A, B>) { return match(value); }
",
        &["match(value) : [A, B]"],
    );
}

#[test]
fn conditional_default_constraint_retains_alias_literals_and_free_widening() {
    // Pinned native primitiveConstraintWave5.types retains the alias-constraint
    // literal but still widens an unconstrained enum member and string literal.
    expect(
        r#"// @strict: true
// @target: es2015
type Both<U> = U extends string ? string : number;
type Alias<U> = Both<U>;
declare function alias<U, T extends Alias<U>>(value: T, other: U): [T];
declare function free<T>(value: T): [T];
enum First { A = "a", B = "b" }
export const held = alias("alias", "key");
export const ordinaryEnum = free(First.A);
export const freeText = free("free");
"#,
        &["held : [\"alias\"]", "ordinaryEnum : [First]", "freeText : [string]"],
    );
}

#[test]
fn conditional_default_constraints_elide_any_without_following_branch_constraints() {
    // The any branches must not swallow the primitive opposite branch or
    // make an object-only default primitive. A constrained variable branch is
    // not replaced by its string base constraint at this native boundary.
    expect(
        r#"// @strict: true
// @target: es2015
type Both<U> = U extends string ? string : number;
type TrueAny<U> = U extends string ? any : number;
type FalseAny<U> = U extends string ? string : any;
type NoPrimitive<U> = U extends string ? any : {};
type FalseObject<U> = U extends string ? {} : any;
type VariableOnly<U, V extends string> = U extends string ? any : V;
declare function both<U, T extends Both<U>>(value: T, other: U): [T];
declare function trueAny<U, T extends TrueAny<U>>(value: T, other: U): [T];
declare function falseAny<U, T extends FalseAny<U>>(value: T, other: U): [T];
declare function noPrimitive<U, T extends NoPrimitive<U>>(value: T, other: U): [T];
declare function falseObject<U, T extends FalseObject<U>>(value: T, other: U): [T];
declare function variableOnly<U, V extends string, T extends VariableOnly<U, V>>(value: T, other: U, variable: V): [T];
export const text = both("held", "key");
export const numeric = both(37, false);
export const trueNumber = trueAny(41, "key");
export const falseText = falseAny("other", false);
export const objectControl = noPrimitive("wide", "key");
export const falseObjectControl = falseObject(53, false);
export const variableControl = variableOnly("variable", "key", "marker");
"#,
        &[
            "text : [\"held\"]",
            "numeric : [37]",
            "trueNumber : [41]",
            "falseText : [\"other\"]",
            "objectControl : [string]",
            "falseObjectControl : [number]",
            "variableControl : [\"variable\"]",
        ],
    );
}

#[test]
fn dependent_conditional_calls_map_branches_before_applicability() {
    // Pinned conditionalApplicabilityWave6.types: argument order and a forward
    // constraint reference must not change inference. Equal string payloads
    // from a foreign enum owner still recover to First, never Second.A.
    expect(
        r#"// @strict: true
// @target: es2015
enum First { A = "a", B = "b" }
enum Second { A = "a", C = "c" }
declare function enumChoice<U, T extends U extends string ? First : number>(value: T, other: U): [T];
declare function keyed<U, T extends U extends string ? First : number>(other: U, value: T): [T];
declare function forward<T extends U extends string ? First : number, U>(value: T, other: U): [T];
export const held = enumChoice(First.A, "key");
export const numeric = enumChoice(37, false);
export const keyedValue = keyed("key", First.B);
export const forwardValue = forward(First.B, "key");
export const foreignOwner = enumChoice(Second.A, "key");
export const falseBranchEnum = enumChoice(First.A, false);
"#,
        &[
            "held : [First.A]",
            "numeric : [37]",
            "keyedValue : [First.B]",
            "forwardValue : [First.B]",
            "foreignOwner : [First]",
            "falseBranchEnum : [number]",
        ],
    );
}

#[test]
fn dependent_conditionals_preserve_captured_alias_bindings_and_distribution() {
    // The alias captures X := First while its call supplies U. A substituted
    // string | boolean check must distribute, rather than choose only number.
    expect(
        r#"// @strict: true
// @target: es2015
enum First { A = "a", B = "b" }
enum Second { A = "a", C = "c" }
type Factory<X> = <U, T extends U extends string ? X : number>(value: T, other: U) => [T];
declare const captured: Factory<First>;
declare function enumChoice<U, T extends U extends string ? First : number>(value: T, other: U): [T];
declare const unionKey: string | boolean;
export const capturedValue = captured(First.B, "key");
export const capturedForeign = captured(Second.A, "key");
export const distributedEnum = enumChoice(First.B, unionKey);
export const distributedNumber = enumChoice(53, unionKey);
"#,
        &[
            "capturedValue : [First.B]",
            "capturedForeign : [First]",
            "distributedEnum : [First.B]",
            "distributedNumber : [53]",
        ],
    );
}

#[test]
fn nested_parenthesized_conditional_constraints_keep_both_branch_boundaries() {
    // Pinned conditionalWave6_constraintWrappers.types. Scope admission must
    // follow transparent wrappers without swapping or skipping the inner test.
    expect(
        r#"// @strict: true
// @target: es2015
enum First { A = "a", B = "b" }
declare function nested<const U, T extends ((U extends string ? U extends "only" ? First : number : boolean))>(value: T, other: U): [T];
export const outerAndInner = nested(First.A, "only");
export const innerFalse = nested(37, "other");
export const outerFalse = nested(true, false);
"#,
        &["outerAndInner : [First.A]", "innerFalse : [37]", "outerFalse : [true]"],
    );
}
