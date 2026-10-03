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
