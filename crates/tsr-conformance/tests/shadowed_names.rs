//! Outcomes checked against pinned tsgo 5b1047d10d32e7d5b446be4de56b126ff42f82bb
//! (`testdata/baselines/reference/submodule/compiler/{bluebirdStaticThis,
//! declarationEmitShadowing,contextualSignatureInstantiation2}.types` and
//! `conformance/conditionalTypes1.types`).
use tsr_conformance::{TestCase, types_baseline::FileTypes, types_producer};
use tsr_core::Idx;

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

/// Allocation skips both written names and inherited display allocations.
/// Sibling signatures restore their inherited naming state.
#[test]
fn suffixes_and_siblings_do_not_capture_parameter_names() {
    expect(
        r"// @strict: true
// @target: es2015
function suffixes<T, T_1>(value: T, reserved: T_1) {
    return function inner<T>(local: T) { return Math.random() ? value : local; };
}
function siblings<T>(value: T) {
    return {
        first<T>(local: T) { return Math.random() ? value : local; },
        second<T>(local: T) { return Math.random() ? value : local; }
    };
}
",
        &[
            "suffixes : <T, T_1>(value: T, reserved: T_1) => <T_2>(local: T_2) => T | T_2",
            "inner : <T>(local: T) => T_2 | T",
            "first : <T>(local: T) => T_1 | T",
            "second : <T>(local: T) => T_1 | T",
        ],
    );
}

/// Constraints/defaults reference the outer identity even though their written
/// alias resolves to a same-spelled type parameter.
#[test]
fn constraints_and_defaults_keep_outer_parameter_names() {
    expect(
        r"// @strict: true
// @target: es2015
function constraints<T>(value: T) {
    type Outer = T;
    return function inner<T extends Outer = Outer>(local: T): Outer | T { return Math.random() ? value : local; };
}
",
        &[
            "constraints : <T>(value: T) => <T_1 extends T = T>(local: T_1) => T | T_1",
            "inner : <T extends T_1 = T_1>(local: T) => T_1 | T",
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

/// `compiler/declarationEmitHigherOrderRetainedGenerics.types`: an instantiated
/// overload set keeps semantic signatures for later use, but the node builder
/// must still allocate names again at each assertion site. Both sibling
/// overloads independently allocate `_1`; a nested retained signature sees the
/// same site shadow without leaking either sibling's allocation.
#[test]
fn retained_overloads_allocate_shadowed_names_at_the_print_site() {
    let source = include_str!(
        "../../../vendor/typescript-go/_submodules/TypeScript/tests/cases/compiler/declarationEmitHigherOrderRetainedGenerics.ts"
    );
    let lines = lines(source);

    let retained: Vec<_> =
        lines.iter().filter(|line| line.contains("<R2_1, O2_1, E2_1, B_1, A, C>")).collect();
    assert!(retained.len() >= 2, "retained overloads were not site-renamed: {lines:?}");
    assert!(
        retained.iter().any(|line| line.contains("): <R1_1, O1_1, E1_1>(self:")),
        "the nested retained signature was not renamed: {retained:?}"
    );
    assert!(
        retained.iter().all(|line| !line.contains("R1_2") && !line.contains("R2_2")),
        "an overload scope leaked into its sibling or nested signature: {retained:?}"
    );
}

/// A union must render its distinct semantic parameters at the print site,
/// not repeat their identical mint-time spelling. Pinned native `TestLocal`
/// control `tsrShadowedNameProbe.ts` under strict/es2015.
#[test]
fn uninstantiated_shadowed_unions_allocate_names() {
    expect(
        r"// @strict: true
// @target: es2015
class Holder<T> {
    value!: T;
    choose<T>(other: T) { return Math.random() ? this.value : other; }
}
function outer<T>(value: T) {
    return function inner<T>(local: T) { return Math.random() ? value : local; };
}
",
        &[
            "choose : <T>(other: T) => T_1 | T",
            "inner : <T>(local: T) => T_1 | T",
            "Math.random() ? this.value : other : T_1 | T",
            "Math.random() ? value : local : T_1 | T",
        ],
    );
}

/// Three distinct same-spelled identities need three names in one union, but
/// the single-parameter reads each start a fresh allocation. Reversed and warm
/// queries distinguish print-scoped state from a Checker-lifetime name cache.
#[test]
fn repeated_serialization_resets_allocations_without_changing_types() {
    let source = r"// @strict: true
// @target: es2015
function triple<T>(outer: T) {
    return function middle<T>(mid: T) {
        return function inner<T>(local: T) {
            const mix = Math.random() ? outer : Math.random() ? mid : local;
            mix; outer; mid; local; mix;
        };
    };
}
";
    let case = TestCase::parse("probe/shadowed_names", "probe.ts", source);
    let arena = tsr_core::Arena::new();
    let program = types_producer::program_for_case(&arena, &case);
    let mut checker = types_producer::configured_checker(&program);
    let mut sites = (0..program.nodes().len())
        .filter_map(|index| {
            let id = tsr_ast::NodeId::from_usize(index);
            let tsr_ast::Node::Identifier(identifier) = program.node_map().get(id)? else {
                return None;
            };
            let parent = program.nodes().parent(id)?;
            (program.nodes().kind(parent) == tsr_ast::SyntaxKind::ExpressionStatement)
                .then_some((id, identifier))
        })
        .collect::<Vec<_>>();
    sites.sort_by_key(|(id, _)| program.nodes().span(*id).start);
    let types: Vec<_> = sites
        .iter()
        .map(|(_, identifier)| {
            checker.check_expression(tsr_ast::Expression::Identifier(identifier))
        })
        .collect();
    let semantic: Vec<_> = types.iter().map(|id| checker.type_of(*id).clone()).collect();
    for reversed in [false, true, false] {
        let indices: Vec<_> =
            if reversed { (0..sites.len()).rev().collect() } else { (0..sites.len()).collect() };
        for index in indices {
            let (id, identifier) = sites[index];
            let expected = match identifier.text {
                "mix" => "T_1 | T_2 | T",
                "outer" | "mid" => "T_1",
                "local" => "T",
                other => panic!("unexpected reference {other}"),
            };
            assert_eq!(checker.type_to_string_at(types[index], id).as_deref(), Some(expected));
            assert_eq!(checker.type_of(types[index]), &semantic[index]);
            assert_eq!(
                checker.check_expression(tsr_ast::Expression::Identifier(identifier)),
                types[index],
                "serialization changed the semantic type"
            );
        }
    }
    let tsr_checker::types::TypeData::Union { types: constituents, .. } = &semantic[0].data else {
        panic!("mix must retain a semantic union");
    };
    assert_eq!(constituents.len(), 3);
    assert!(constituents.iter().all(|id| checker.type_to_string(*id) == "T"));
    assert_eq!(types[0], types[4]);
}

/// Reopening a semantic union must retain the existing display plan's boolean
/// collapse and nullable ordering. Pinned native strict/es2015 control.
#[test]
fn shadowed_unions_keep_nullable_order_and_boolean_collapse() {
    expect(
        r"// @strict: true
// @target: es2015
function formats<T>(value: T, nullable: T | null | undefined, bool: boolean | T) {
    nullable; bool;
    return function inner<T>(local: T) {
        const choice = Math.random() ? value : local;
        const optional = Math.random() ? choice : undefined;
        const booleanChoice = Math.random() ? choice : true as boolean;
        choice; optional; booleanChoice;
    };
}
",
        &[
            "nullable : T | null | undefined",
            "bool : boolean | T",
            "choice : T_1 | T",
            "optional : T_1 | T | undefined",
            "booleanChoice : boolean | T_1 | T",
        ],
    );
}
