//! What the assignable relation must get right.
//!
//! Every fixture is written as **two type annotations** and asks whether the
//! first is assignable to the second, so that the question the test asks is the
//! question `getAssignmentReducedType` and `resolveCall` will ask.
//!
//! Two properties are load-bearing and each has a test whose *only* job is to
//! die under a specific mutation — noted on the test:
//!
//! - assignability is **directional**, so every positive case has a negative
//!   twin in the other direction wherever the relation is not symmetric;
//! - the relation **terminates** on a recursive type — and since structural
//!   comparison landed, both guards are what make that true. See
//!   `mutually_recursive_interfaces_terminate` and
//!   `a_chain_deeper_than_the_cap_gives_up`, whose mutations kill them
//!   separately.
//!
//! See [`crates/tsr-checker/src/relater.rs`](../src/relater.rs) for what is
//! still gapped. Object types are now compared **structurally**, so the gaps
//! that remain are narrower: optional properties, `readonly`, index and call
//! signatures, and any base type this port cannot follow. Each is asserted here
//! in the direction it fails, so a gap is a fact of the test suite rather than
//! an unstated limit.

use std::fmt::Write as _;

use tsr_ast::Statement;
use tsr_checker::{Checker, TypeId};
use tsr_core::Arena;

#[test]
fn subtype_follows_parameter_constraints_and_rejects_the_reverse() {
    use tsr_checker::relater::{Relation, Ternary};
    with_checker("function f<T, U extends T>(x:T, y:U) {}", |checker, statements| {
        let Statement::FunctionDeclaration(function) = statements[0] else { panic!("function") };
        let source = checker.get_type_from_type_node(function.parameters[0].r#type.unwrap());
        let derived = checker.get_type_from_type_node(function.parameters[1].r#type.unwrap());
        assert_eq!(checker.relate_ternary(derived, source, Relation::Subtype), Ternary::Related);
        assert_eq!(checker.relate_ternary(source, derived, Relation::Subtype), Ternary::NotRelated);
    });
}

#[test]
fn circular_parameter_constraints_do_not_prove_an_object_subtype() {
    use tsr_checker::relater::{Relation, Ternary};
    with_checker(
        "function f<T extends U,U extends T>(x:T,y:{value:number}) {}",
        |checker, statements| {
            let Statement::FunctionDeclaration(function) = statements[0] else {
                panic!("function")
            };
            let source = checker.get_type_from_type_node(function.parameters[0].r#type.unwrap());
            let target = checker.get_type_from_type_node(function.parameters[1].r#type.unwrap());
            assert_eq!(checker.relate_ternary(source, target, Relation::Subtype), Ternary::Unknown);
        },
    );
}

/// Parse, bind and check one source, then answer a question about it.
///
/// Same shape as the harness in `tests/unions.rs`, and for the same reason: the
/// checker borrows the arena, the parse result and the bind result, so all three
/// have to outlive it.
fn with_checker<R>(
    source: &str,
    ask: impl for<'a> FnOnce(&mut Checker<'a, '_>, &[Statement<'a>]) -> R,
) -> R {
    let arena = Arena::new();
    let parsed = tsr_parser::parse(&arena, source);
    assert!(
        parsed.diagnostics.is_empty(),
        "fixture must parse: {:?}",
        parsed.diagnostics.iter().map(tsr_diagnostics::Diagnostic::text).collect::<Vec<_>>()
    );
    let bound = tsr_binder::bind(
        &arena,
        parsed.source_file,
        &parsed.nodes,
        tsr_binder::FileInfo { name: "test.ts", text: source },
    );
    let mut checker = Checker::new(&bound, &parsed.nodes, &parsed.node_map);
    ask(&mut checker, parsed.source_file.statements)
}

/// The type of the annotation on the `index`th statement's first declaration.
fn annotation_type<'a>(
    checker: &mut Checker<'a, '_>,
    statements: &[Statement<'a>],
    index: usize,
) -> TypeId {
    let Statement::VariableStatement(statement) = statements[index] else {
        panic!("statement {index} must be a variable statement");
    };
    let annotation = statement
        .declaration_list
        .and_then(|list| list.declarations.first().copied())
        .and_then(|declaration| declaration.r#type)
        .expect("an annotation");
    checker.get_type_from_type_node(annotation)
}

/// Is the first declaration's annotated type assignable to the second's?
///
/// The fixture is always two `declare`-free `let` statements, so that the two
/// types are written in source and nothing has to be constructed by hand.
fn assignable(source: &str) -> bool {
    with_checker(source, |checker, statements| {
        let source_type = annotation_type(checker, statements, 0);
        let target_type = annotation_type(checker, statements, 1);
        checker.is_type_assignable_to(source_type, target_type)
    })
}

fn array_tuple_assignable(from: &str, to: &str) -> bool {
    let source = format!(
        "interface Array<T> {{ length: number; [index: number]: T }}
        interface ReadonlyArray<T> {{ readonly length: number; readonly [index: number]: T }}
        let source: {from}; let target: {to};"
    );
    with_checker(&source, |checker, statements| {
        let source = annotation_type(checker, statements, 2);
        let target = annotation_type(checker, statements, 3);
        checker.is_type_assignable_to(source, target)
    })
}

#[test]
fn fixed_tuple_relations_check_readonly_arity_and_optional_elements() {
    assert!(assignable("let source: [number]; let target: readonly [number];"));
    assert!(!assignable("let source: readonly [number]; let target: [number];"));
    assert!(assignable("let source: [number]; let target: [number, string?];"));
    assert!(!assignable("let source: [number?]; let target: [number];"));
    assert!(!assignable("let source: [number, string]; let target: [number];"));
    assert!(!assignable("let source: [number]; let target: [string];"));
    assert!(assignable("let source: [number | undefined]; let target: [number?];"));
}

#[test]
fn tuple_array_relations_compare_index_types_and_readonly_state() {
    let related = array_tuple_assignable;
    assert!(related("[number, number]", "number[]"));
    assert!(!related("[number, string]", "number[]"));
    assert!(related("readonly [number]", "ReadonlyArray<number>"));
    assert!(!related("readonly [number]", "number[]"));
    assert!(related("[]", "string[]"));
    assert!(!related("number[]", "[number?]"));
    assert!(!related("number[]", "[]"));
    assert!(related("[number, ...string[]]", "(number | string)[]"));
    assert!(!related("[number, ...string[]]", "number[]"));
    assert!(!related("[number?]", "number[]"));
    assert!(related("[number?]", "(number | undefined)[]"));
}

#[test]
fn rest_tuple_relations_align_required_prefixes_and_suffixes() {
    let related = array_tuple_assignable;
    assert!(related("[number, string, string]", "[number, ...string[]]"));
    assert!(!related("[number, number]", "[number, ...string[]]"));
    assert!(related("[number, number, string]", "[...number[], string]"));
    assert!(!related("[number, number]", "[...number[], string]"));
    assert!(related("[number, ...string[]]", "[number, ...(string | number)[]]"));
    assert!(!related("[number, ...string[]]", "[number, string, ...string[]]"));
    assert!(!related("[number, ...string[]]", "[number, string?]"));
    assert!(related("[number]", "[number, ...string[]]"));
    assert!(!related("[]", "[number, ...string[]]"));
    assert!(!related("[...number[], string]", "[...string[], string]"));
}

#[test]
fn array_to_rest_tuple_relations_keep_required_and_readonly_bounds() {
    let related = array_tuple_assignable;
    assert!(related("number[]", "[number?, ...number[]]"));
    assert!(!related("number[]", "[number, ...number[]]"));
    assert!(related("ReadonlyArray<number>", "readonly [number?, ...number[]]"));
    assert!(!related("ReadonlyArray<number>", "[number?, ...number[]]"));
    assert!(!related("string[]", "[number?, ...number[]]"));
}

#[test]
fn exact_optional_tuple_targets_require_explicit_undefined() {
    with_checker(
        "let source: [number | undefined]; let target: [number?];",
        |checker, statements| {
            let source = annotation_type(checker, statements, 0);
            let target = annotation_type(checker, statements, 1);
            assert!(checker.is_type_assignable_to(source, target));
            checker.apply_compiler_options(&tsr_core::CompilerOptions {
                strict: tsr_core::Tristate::True,
                exact_optional_property_types: tsr_core::Tristate::True,
                ..Default::default()
            });
            assert!(!checker.is_type_assignable_to(source, target));
        },
    );
}

#[test]
fn generic_variadic_sources_use_only_concrete_array_or_tuple_constraints() {
    use tsr_checker::relater::Ternary;
    let relation = |parameters: &str, from: &str, to: &str| {
        let source = format!(
            "interface Array<T> {{ length: number; [index: number]: T }}
            interface ReadonlyArray<T> {{ readonly length: number; readonly [index: number]: T }}
            declare function compare<{parameters}>(source: {from}, target: {to}): void;"
        );
        with_checker(&source, |checker, statements| {
            let Statement::FunctionDeclaration(function) = statements[2] else {
                panic!("function")
            };
            let source = checker.get_type_from_type_node(function.parameters[0].r#type.unwrap());
            let target = checker.get_type_from_type_node(function.parameters[1].r#type.unwrap());
            checker.relate_ternary(source, target, tsr_checker::relater::Relation::Assignable)
        })
    };
    assert_eq!(
        relation("T extends string[]", "[number, ...T]", "[number, ...string[]]"),
        Ternary::Related
    );
    assert_eq!(
        relation("T extends string[]", "[number, ...T]", "[number, ...boolean[]]"),
        Ternary::NotRelated
    );
    assert_eq!(
        relation(
            "T extends readonly [string, boolean]",
            "[number, ...T]",
            "[number, string, boolean]"
        ),
        Ternary::Related
    );
    assert_eq!(
        relation("U extends string[], T extends U", "[number, ...T]", "[number, ...string[]]"),
        Ternary::Related
    );
    assert_eq!(
        relation(
            "T extends [string] | [boolean]",
            "[number, ...T]",
            "[number, ...(string | boolean)[]]"
        ),
        Ternary::Related
    );
    assert_eq!(
        relation("T extends string[]", "[number, ...T]", "(number | string)[]"),
        Ternary::Related
    );
    assert_eq!(
        relation("T extends string[]", "readonly [number, ...T]", "(number | string)[]"),
        Ternary::NotRelated
    );
}

#[test]
fn nonstrict_nullable_sources_relate_to_objects_but_not_never() {
    for nullable in ["null", "undefined"] {
        let source = format!("let a: {nullable}; let b: {{ value: number }}; let c: never;");
        with_checker(&source, |checker, statements| {
            let a = annotation_type(checker, statements, 0);
            let b = annotation_type(checker, statements, 1);
            let c = annotation_type(checker, statements, 2);
            assert!(!checker.is_type_assignable_to(a, b));
            checker.apply_compiler_options(&tsr_core::CompilerOptions {
                strict_null_checks: tsr_core::Tristate::False,
                ..Default::default()
            });
            assert!(checker.is_type_assignable_to(a, b));
            assert!(!checker.is_type_assignable_to(b, a));
            assert!(!checker.is_type_assignable_to(a, c));
        });
    }
}

/// Both directions at once, so a symmetric bug cannot pass as a correct answer.
fn both_ways(source: &str) -> (bool, bool) {
    with_checker(source, |checker, statements| {
        let a = annotation_type(checker, statements, 0);
        let b = annotation_type(checker, statements, 1);
        (checker.is_type_assignable_to(a, b), checker.is_type_assignable_to(b, a))
    })
}

#[test]
fn strict_function_parameters_are_contravariant() {
    assert_eq!(
        both_ways("let a: (value: string) => void; let b: (value: 'a') => void;"),
        (true, false)
    );
}

#[test]
fn generic_reference_inputs_follow_measured_contravariance() {
    let source = "interface Sink<T> { consume: (value: T) => void }
        let broad: Sink<string>; let narrow: Sink<'a'>;";
    with_checker(source, |checker, statements| {
        let broad = annotation_type(checker, statements, 1);
        let narrow = annotation_type(checker, statements, 2);
        assert!(checker.is_type_assignable_to(broad, narrow));
        assert!(!checker.is_type_assignable_to(narrow, broad));
    });
}

#[test]
fn generic_reference_methods_follow_measured_bivariance() {
    let source = "interface Sink<T> { consume(value: T): void }
        let broad: Sink<string>; let narrow: Sink<'a'>;";
    with_checker(source, |checker, statements| {
        let broad = annotation_type(checker, statements, 1);
        let narrow = annotation_type(checker, statements, 2);
        assert!(checker.is_type_assignable_to(broad, narrow));
        assert!(checker.is_type_assignable_to(narrow, broad));
    });
}

#[test]
fn generic_reference_inputs_and_outputs_require_invariance() {
    let source = "interface Cell<T> { read: () => T; write: (value: T) => void }
        let broad: Cell<string>; let narrow: Cell<'a'>;";
    with_checker(source, |checker, statements| {
        let broad = annotation_type(checker, statements, 1);
        let narrow = annotation_type(checker, statements, 2);
        assert!(!checker.is_type_assignable_to(broad, narrow));
        assert!(!checker.is_type_assignable_to(narrow, broad));
    });
}

#[test]
fn unused_reference_parameters_are_independent() {
    let source = "interface Phantom<T> { value: number }
        let text: Phantom<string>; let numeric: Phantom<number>;";
    with_checker(source, |checker, statements| {
        let text = annotation_type(checker, statements, 1);
        let numeric = annotation_type(checker, statements, 2);
        assert!(checker.is_type_assignable_to(text, numeric));
        assert!(checker.is_type_assignable_to(numeric, text));
    });
}

#[test]
fn covariance_to_void_can_fall_back_to_structural_return_comparison() {
    let source = "interface Producer<T> { produce: () => T }
        let numeric: Producer<number>; let ignored: Producer<void>;";
    with_checker(source, |checker, statements| {
        let numeric = annotation_type(checker, statements, 1);
        let ignored = annotation_type(checker, statements, 2);
        assert!(checker.is_type_assignable_to(numeric, ignored));
        assert!(!checker.is_type_assignable_to(ignored, numeric));
    });
}

#[test]
fn changing_strict_function_types_invalidates_measured_variance() {
    let source = "interface Cell<T> { read: () => T; write: (value: T) => void }
        let broad: Cell<string>; let narrow: Cell<'a'>;";
    with_checker(source, |checker, statements| {
        let broad = annotation_type(checker, statements, 1);
        let narrow = annotation_type(checker, statements, 2);
        assert!(!checker.is_type_assignable_to(narrow, broad));
        checker.apply_compiler_options(&tsr_core::CompilerOptions {
            strict_function_types: tsr_core::Tristate::False,
            ..Default::default()
        });
        assert!(checker.is_type_assignable_to(narrow, broad));
    });
}

#[test]
fn generic_signatures_instantiate_from_contextual_inputs() {
    assert!(assignable("let a: <T>(value: T) => T; let b: (value: number) => number;"));
    assert!(!assignable(
        "let a: <T extends number>(value: T) => T; let b: (value: string) => string;"
    ));
    assert!(assignable("let a: <T>(value: T) => T; let b: <U>(value: U) => U;"));
}

#[test]
fn contextual_return_inference_fills_parameters_missing_from_inputs() {
    assert!(assignable("let a: <T>() => T; let b: () => string;"));
    assert!(!assignable("let a: <T>(value: T) => T; let b: (value: number) => string;"));
}

#[test]
fn generic_callback_comparison_terminates_with_nested_signatures() {
    assert!(assignable(
        "let a: <T>(callback: (value: T) => T) => T;
        let b: <U>(callback: (value: U) => U) => U;"
    ));
}

#[test]
fn nonstrict_function_parameters_are_bivariant() {
    let source = "let a: (value: string) => void; let b: (value: 'a') => void;";
    with_checker(source, |checker, statements| {
        checker.apply_compiler_options(&tsr_core::CompilerOptions {
            strict_function_types: tsr_core::Tristate::False,
            ..Default::default()
        });
        let a = annotation_type(checker, statements, 0);
        let b = annotation_type(checker, statements, 1);
        assert!(checker.is_type_assignable_to(a, b));
        assert!(checker.is_type_assignable_to(b, a));
    });
}

#[test]
fn callback_return_variance_follows_strict_function_types() {
    let source = "let a: (callback: () => number) => void;
        let b: (callback: () => number | string) => void;";
    assert_eq!(both_ways(source), (false, true));
    with_checker(source, |checker, statements| {
        checker.apply_compiler_options(&tsr_core::CompilerOptions {
            strict_function_types: tsr_core::Tristate::False,
            ..Default::default()
        });
        let a = annotation_type(checker, statements, 0);
        let b = annotation_type(checker, statements, 1);
        assert!(checker.is_type_assignable_to(a, b));
        assert!(checker.is_type_assignable_to(b, a));
    });
}

#[test]
fn predicate_relations_compare_asserted_types_and_parameter_positions() {
    assert!(assignable(
        "let a: (input: unknown) => input is number;
        let b: (value: unknown) => value is number;"
    ));
    assert!(!assignable(
        "let a: (input: unknown) => input is number;
        let b: (value: unknown) => value is string;"
    ));
    assert!(!assignable(
        "let a: (input: unknown, ignored: unknown) => input is number;
        let b: (unused: unknown, value: unknown) => value is number;"
    ));
    assert!(!assignable(
        "let a: (input: unknown) => boolean;
        let b: (value: unknown) => value is number;"
    ));
}

#[test]
fn function_this_parameters_are_compared_and_void_sources_are_permitted() {
    assert!(!assignable(
        "let a: (this: string) => void;
        let b: (this: number) => void;"
    ));
    assert!(assignable(
        "let a: (this: void) => void;
        let b: (this: number) => void;"
    ));
}

#[test]
fn strict_subtype_signature_arity_counts_optional_parameters() {
    let source = "let a: (value: number, index?: number) => void;
        let b: (value: number) => void;";
    with_checker(source, |checker, statements| {
        let a = annotation_type(checker, statements, 0);
        let b = annotation_type(checker, statements, 1);
        assert!(checker.is_type_assignable_to(a, b));
        assert!(!checker.is_type_strict_subtype_of(a, b));
        assert!(checker.is_type_strict_subtype_of(b, a));
    });
}

#[test]
fn fixed_tuple_rests_expand_parameter_types_and_required_arity() {
    assert_eq!(
        both_ways(
            "let a: (...args: [number, string]) => void;
            let b: (first: number, second: string) => void;"
        ),
        (true, true)
    );
    assert!(!assignable(
        "let a: (...args: [number, string]) => void;
        let b: (first: number) => void;"
    ));
    assert!(assignable(
        "let a: (...args: [number, string?]) => void;
        let b: (first: number) => void;"
    ));
}

#[test]
fn array_rests_compare_each_effective_parameter_contravariantly() {
    assert_eq!(
        both_ways(
            "let a: (...args: string[]) => void;
            let b: (first: 'a', second: 'a') => void;
            interface Array<T> { [index: number]: T }"
        ),
        (true, false)
    );
    assert!(!assignable(
        "let a: (...args: string[]) => void;
        let b: (first: string, second: number) => void;
        interface Array<T> { [index: number]: T }"
    ));
}

#[test]
fn a_variable_tuple_rest_keeps_its_fixed_prefix() {
    assert!(assignable(
        "let a: (...args: [number, ...string[]]) => void;
        let b: (first: number, second: 'a', third: 'b') => void;
        interface Array<T> { [index: number]: T }"
    ));
    assert!(!assignable(
        "let a: (...args: [number, ...string[]]) => void;
        let b: (first: string, second: 'a') => void;
        interface Array<T> { [index: number]: T }"
    ));
}

#[test]
fn trailing_void_parameters_can_be_omitted_in_signature_comparison() {
    assert!(assignable(
        "let a: (value: number, unused: void) => void;
        let b: (value: number) => void;"
    ));
    assert!(!assignable(
        "let a: (value: number, extra: string) => void;
        let b: (value: number) => void;"
    ));
}

#[test]
fn top_signatures_accept_any_inputs_and_results() {
    for rest in ["any", "any[]", "never", "never[]"] {
        let source = format!(
            "let a: (input: number) => string;
            let b: (...args: {rest}) => unknown;
            interface Array<T> {{ [index: number]: T }}"
        );
        assert!(assignable(&source), "{rest}");
    }
}

#[test]
fn top_signatures_are_not_strict_subtypes_of_concrete_signatures() {
    with_checker(
        "let a: (...args: any) => any; let b: (input: number) => string;",
        |checker, statements| {
            let a = annotation_type(checker, statements, 0);
            let b = annotation_type(checker, statements, 1);
            assert!(checker.is_type_assignable_to(a, b));
            assert!(!checker.is_type_strict_subtype_of(a, b));
            assert!(checker.is_type_strict_subtype_of(b, a));
        },
    );
}

#[test]
fn method_parameters_remain_bivariant_under_strict_function_types() {
    assert_eq!(
        both_ways(
            "let a: { apply(value: string): void };
            let b: { apply(value: 'a'): void };"
        ),
        (true, true)
    );
}

#[test]
fn instantiated_generic_method_inputs_use_ordinary_parameter_comparison() {
    for (source, target) in [("string", "'a'"), ("'a'", "string")] {
        let fixture = format!(
            "let a: Generic<(input: {source}) => void>;
            let b: {{ accept(callback: (input: {target}) => void): void }};
            interface Generic<T> {{ accept(callback: T): void }}"
        );
        assert_eq!(both_ways(&fixture), (true, true));
    }
    assert!(!assignable(
        "let a: Generic<(input: string) => void>;
        let b: { accept(callback: (input: number) => void): void };
        interface Generic<T> { accept(callback: T): void }"
    ));
}

/// A literal is assignable to its base primitive, and not the other way round.
///
/// **Directional**, which is the whole point: a relation implemented as "do the
/// flags overlap" passes the first half and fails the second.
///
/// Reddened by: replacing the `STRING_LIKE`/`STRING` arm's operands with
/// `s.intersects(STRING) && t.intersects(STRING_LIKE)` — the reversed arm makes
/// the second assertion true.
#[test]
fn a_string_literal_is_assignable_to_string_but_not_conversely() {
    let (forward, backward) = both_ways("let a: \"x\"; let b: string;");
    assert!(forward, "\"x\" -> string");
    assert!(!backward, "string -> \"x\"");
}

/// The same shape for numbers and bigints, which have their own arms.
///
/// Reddened by: deleting the `NUMBER_LIKE`/`NUMBER` arm.
#[test]
fn a_number_literal_is_assignable_to_number_but_not_conversely() {
    let (forward, backward) = both_ways("let a: 1; let b: number;");
    assert!(forward, "1 -> number");
    assert!(!backward, "number -> 1");
}

/// `never` is assignable to everything; everything is not assignable to `never`.
///
/// **The arm order is NOT tested here, and cannot be in this port.** The claim
/// this test originally carried — that hoisting `t.intersects(NEVER) => false`
/// above `s.intersects(NEVER) => true` flips `never -> never` — was run and came
/// back **green**: `never` is one interned type, so `never -> never` is settled
/// by the `source == target` identity check before `is_simple_type_related_to`
/// is ever called. Distinguishing the two orders needs two distinct `never`
/// types, which this port does not have. Recorded rather than deleted because
/// the order still matters upstream and will matter here the day it does.
///
/// What *is* verified: the three answers below.
///
/// Reddened by: deleting the `s.intersects(NEVER)` half of the first arm, which
/// flips `never -> string`.
#[test]
fn never_is_assignable_to_everything_including_never() {
    assert!(assignable("let a: never; let b: string;"), "never -> string");
    assert!(assignable("let a: never; let b: never;"), "never -> never");
    assert!(!assignable("let a: string; let b: never;"), "string -> never");
}

/// `any` relates in both directions; `unknown` only as a target.
///
/// Reddened by: deleting the `t.intersects(UNKNOWN)` arm, which flips the
/// second assertion.
#[test]
fn any_relates_both_ways_and_unknown_only_as_a_target() {
    let (forward, backward) = both_ways("let a: string; let b: any;");
    assert!(forward, "string -> any");
    assert!(backward, "any -> string");
    assert!(assignable("let a: string; let b: unknown;"), "string -> unknown");
    assert!(!assignable("let a: unknown; let b: string;"), "unknown -> string");
}

/// A source union needs *every* constituent related; a target union needs *one*.
///
/// The two halves are the two quantifiers, and swapping either one is a real bug
/// that this test is shaped to catch: `("x" | 1) -> string` must be **false**
/// because `1` is not a string, while `"x" -> (string | number)` must be true.
///
/// Reddened by: changing the source-union arm's `.all(` to `.any(`, which makes
/// the second assertion true.
#[test]
fn a_source_union_is_universal_and_a_target_union_existential() {
    assert!(assignable("let a: \"x\" | \"y\"; let b: string;"), "(\"x\"|\"y\") -> string");
    assert!(!assignable("let a: \"x\" | 1; let b: string;"), "(\"x\"|1) -> string");
    assert!(assignable("let a: \"x\"; let b: string | number;"), "\"x\" -> (string|number)");
    assert!(!assignable("let a: boolean; let b: string | number;"), "boolean -> (string|number)");
}

/// A source union is decomposed *before* a target union is searched.
///
/// This is the ordering claim in `structured_type_related_to`'s doc comment, and
/// it is not decoration: union interning makes `"x" | "y"` a single type that
/// does **not** appear in `("x" | "y" | "z")`'s constituent list, so a relation
/// that searched the target first would answer `false` for a pair that is
/// plainly assignable.
///
/// Reddened by: moving the target-union arm above the source-union arm.
#[test]
fn a_union_is_assignable_to_a_wider_union() {
    let (forward, backward) = both_ways("let a: \"x\" | \"y\"; let b: \"x\" | \"y\" | \"z\";");
    assert!(forward, "(\"x\"|\"y\") -> (\"x\"|\"y\"|\"z\")");
    assert!(!backward, "(\"x\"|\"y\"|\"z\") -> (\"x\"|\"y\")");
}

/// A target intersection needs every constituent; a source intersection needs one.
///
/// **The obvious fixture does not work, and that is the finding.** Written as
/// `string & number` this test was decoration: `crate::intersections` reduces an
/// intersection of disjoint primitives to `never`, so the target was `never` and
/// the intersection arm was never reached — the `.all(` → `.any(` mutation left
/// it green. Two interfaces do not reduce, so `I & J` is a real
/// `TypeData::Intersection` and the arm actually runs. Verified by probing what
/// `let a: string & number` resolves to, not by reading the reduction code.
///
/// **The interfaces gained members when structural comparison landed.** Written
/// as two *empty* interfaces the fixture stopped separating the arms the moment
/// object types were compared structurally: `I -> J` is genuinely `true` for two
/// empty interfaces — in TypeScript as much as here — so `I -> (I & J)`
/// succeeded on both halves and the first assertion was asserting a gap that had
/// closed. Disjoint members restore the property the test is about.
///
/// Reddened by: changing the target-intersection arm's `.all(` to `.any(`, which
/// makes `I -> (I & J)` true; and by changing the source-intersection arm's
/// `.any(` to `.all(`, which makes `(I & J) -> I` false.
#[test]
fn a_target_intersection_is_universal_and_a_source_intersection_existential() {
    let source = "interface I { a: string }\ninterface J { b: number }\nlet a: I;\nlet b: I & J;";
    with_checker(source, |checker, statements| {
        let i = annotation_type(checker, statements, 2);
        let both = annotation_type(checker, statements, 3);
        assert!(!checker.is_type_assignable_to(i, both), "I -> (I & J) must fail on the J half");
        assert!(checker.is_type_assignable_to(both, i), "(I & J) -> I");
    });
}

/// `undefined` and `null` are *not* interchangeable under the strict reading.
///
/// This is the `strictNullChecks`-on assumption the module documents, asserted
/// so that anyone who later plumbs the option through has a test that tells them
/// which reading is currently hard-coded.
///
/// Reddened by: adding `|| t.intersects(TypeFlags::NULL)` to the `UNDEFINED`
/// arm's target test.
#[test]
fn undefined_and_null_do_not_relate_to_each_other() {
    let (forward, backward) = both_ways("let a: undefined; let b: null;");
    assert!(!forward, "undefined -> null");
    assert!(!backward, "null -> undefined");
    assert!(assignable("let a: undefined; let b: void;"), "undefined -> void");
}

/// Two structurally identical interfaces relate, in both directions.
///
/// This replaces `two_structurally_identical_interfaces_are_a_gap`, the
/// characterisation test that pinned the answer `false` while structural
/// comparison was unported. Deleted rather than inverted in place, because the
/// thing it asserted no longer exists.
///
/// Reddened by: deleting the `has_members(source) && has_members(target)` arm
/// from the gate in `is_related_to`, which sends a pair of object types back to
/// the trailing `false`.
#[test]
fn two_structurally_identical_interfaces_relate() {
    let source = "interface I { x: string }\ninterface J { x: string }\n\
                  let a: I;\nlet b: J;\nlet c: I;";
    with_checker(source, |checker, statements| {
        let i = annotation_type(checker, statements, 2);
        let j = annotation_type(checker, statements, 3);
        let i_again = annotation_type(checker, statements, 4);
        assert!(checker.is_type_assignable_to(i, i_again), "a type is assignable to itself");
        assert!(checker.is_type_assignable_to(i, j), "I -> J");
        assert!(checker.is_type_assignable_to(j, i), "J -> I");
    });
}

/// A property the target requires and the source lacks means **not related**,
/// and a property whose *type* is wrong means the same.
///
/// Both directions are asserted on the same fixture, which is what makes the
/// test hard to satisfy by accident: `{ x: string }` and `{ x: string, y: number }`
/// relate one way and not the other, so a fix that answers `true` unconditionally
/// fails the second assertion and one that answers `false` unconditionally fails
/// the first.
///
/// Reddened by: replacing the `return false` on a missing source property in
/// `properties_related_to` with `continue`, which makes `narrow -> wide` answer
/// `true`. The type-mismatch half is reddened by dropping the
/// `!self.is_related_to(source_type, target_type)` check.
#[test]
fn a_missing_or_mistyped_property_does_not_relate() {
    let source = "interface Narrow { x: string }\n\
                  interface Wide { x: string; y: number }\n\
                  interface Wrong { x: number }\n\
                  let a: Narrow;\nlet b: Wide;\nlet c: Wrong;";
    with_checker(source, |checker, statements| {
        let narrow = annotation_type(checker, statements, 3);
        let wide = annotation_type(checker, statements, 4);
        let wrong = annotation_type(checker, statements, 5);
        assert!(checker.is_type_assignable_to(wide, narrow), "extra properties are fine");
        assert!(!checker.is_type_assignable_to(narrow, wide), "y is required and missing");
        assert!(!checker.is_type_assignable_to(wrong, narrow), "x: number -> x: string");
        assert!(!checker.is_type_assignable_to(narrow, wrong), "x: string -> x: number");
    });
}

/// An **inherited** requirement counts, on both sides.
///
/// This is the assertion that the deliberate gap was protecting: enumerating
/// only a target's *own* members would answer `true` for a source that fails the
/// base's requirements.
///
/// Reddened by: deleting the `base_symbols_of` recursion from
/// `collect_property_names` (returning `true` before it), which drops `x` from
/// `Derived`'s requirements and makes the second assertion answer `true`.
#[test]
fn an_inherited_property_is_a_requirement() {
    let source = "interface Base { x: string }\n\
                  interface Derived extends Base { y: number }\n\
                  interface Both { x: string; y: number }\n\
                  interface OnlyY { y: number }\n\
                  let a: Derived;\nlet b: Both;\nlet c: OnlyY;";
    with_checker(source, |checker, statements| {
        let derived = annotation_type(checker, statements, 4);
        let both = annotation_type(checker, statements, 5);
        let only_y = annotation_type(checker, statements, 6);
        assert!(checker.is_type_assignable_to(both, derived), "{{x,y}} -> Derived");
        assert!(
            !checker.is_type_assignable_to(only_y, derived),
            "Derived's inherited x is missing"
        );
        assert!(checker.is_type_assignable_to(derived, both), "Derived -> {{x,y}}");
    });
}

/// A self-referential type terminates.
///
/// When this test was written the walk stopped at object types, so
/// `interface I { x: I | string }` was a cycle in the type graph and *not* a
/// cycle in this walk — deleting the cycle guard left it green. Structural
/// comparison closed that loop; see
/// [`mutually_recursive_interfaces_terminate`] for the fixture that now
/// exercises the guard, and `relater.rs` for the measurement.
#[test]
fn a_recursive_type_terminates() {
    let source = "interface I { x: I | string }\nlet a: I | string;\nlet b: I | string | number;";
    with_checker(source, |checker, statements| {
        // Statement 0 is the interface, so the two annotations are 1 and 2.
        let narrow = annotation_type(checker, statements, 1);
        let wide = annotation_type(checker, statements, 2);
        assert!(checker.is_type_assignable_to(narrow, wide), "(I|string) -> (I|string|number)");
        assert!(!checker.is_type_assignable_to(wide, narrow), "(I|string|number) -> (I|string)");
    });
}

/// Mutually recursive interfaces terminate, and the cycle cache is what makes
/// them.
///
/// `interface A { x: B }` with `interface B { x: A }` is the loop the module
/// docs promised would appear the moment structural comparison landed:
/// relating `A -> A2` asks whether `B -> B2`, which asks whether `A -> A2`
/// again. Nothing about the *types* is recursive in a way the earlier fixture
/// caught — the walk has to enter members for the cycle to exist.
///
/// Reddened by: deleting the `self.results.insert((source, target), true)` that
/// parks the pair before recursing in `recursive_type_related_to`. The test then
/// fails — and fails *fast*, in the same milliseconds, because
/// [`tsr_checker::relater::MAX_DEPTH`] catches the walk the cache no longer
/// closes and answers `false`. So the two guards are not interchangeable and
/// this fixture separates them: the cache is what makes the answer **`true`**,
/// the depth cap is what makes the run **terminate**. Neither was exercised
/// before structural comparison landed.
#[test]
fn mutually_recursive_interfaces_terminate() {
    let source = "interface A { x: B }\ninterface B { x: A }\n\
                  interface A2 { x: B2 }\ninterface B2 { x: A2 }\n\
                  let a: A;\nlet b: A2;";
    with_checker(source, |checker, statements| {
        let a = annotation_type(checker, statements, 4);
        let a2 = annotation_type(checker, statements, 5);
        assert!(checker.is_type_assignable_to(a, a2), "A -> A2 through the cycle");
    });
}

/// The depth cap answers `false` rather than overflowing the stack.
///
/// A chain of interfaces deeper than [`tsr_checker::relater::MAX_DEPTH`] nests
/// the walk one frame per link. The assertion is the *honest gap*: upstream
/// relates these two and this port gives up, which is the direction the module
/// docs commit to.
///
/// Reddened by: raising `MAX_DEPTH` to `10_000`. The test does not merely flip
/// to `true` — the process **aborts with a stack overflow**, measured. That is
/// the sharpest available statement of what the cap buys: at 110 links the walk
/// nests one frame per link and the native stack does not hold it. The cap is
/// load-bearing for *safety*, not only for answers.
#[test]
fn a_chain_deeper_than_the_cap_gives_up() {
    let mut source = String::new();
    let depth = tsr_checker::relater::MAX_DEPTH + 10;
    source.push_str("interface L0 { x: string }\ninterface R0 { x: string }\n");
    for i in 1..=depth {
        writeln!(source, "interface L{i} {{ x: L{} }}", i - 1).unwrap();
        writeln!(source, "interface R{i} {{ x: R{} }}", i - 1).unwrap();
    }
    writeln!(source, "let a: L{depth};\nlet b: R{depth};").unwrap();
    let statement_count = 2 + depth * 2;
    with_checker(&source, |checker, statements| {
        let left = annotation_type(checker, statements, statement_count);
        let right = annotation_type(checker, statements, statement_count + 1);
        assert!(!checker.is_type_assignable_to(left, right), "GAP: the depth cap gives up");
    });
}

// ---------------------------------------------------------------------------
// The third answer (`bd tsr-kmzf`).
//
// These assert the CONTRACT in `docs/architecture/checker-notes-assign.md` §2 —
// which of the six "answers `false` without knowing" sites now say `Unknown` —
// rather than whatever the walk happens to do. A test written by reading the
// implementation back cannot fail, and this suite has two properties that only
// exist because a test was allowed to disagree with the code.
//
// The projection property (`is_type_assignable_to` == `relate_ternary(..) ==
// Related`) is asserted directly, because it is what makes every test above
// this line a statement about the ternary walk too.
// ---------------------------------------------------------------------------

/// The three-valued verdict for the same two-annotation fixture shape.
fn verdict(source: &str) -> tsr_checker::relater::Ternary {
    with_checker(source, |checker, statements| {
        let a = annotation_type(checker, statements, 0);
        let b = annotation_type(checker, statements, 1);
        checker.relate_ternary(a, b, tsr_checker::relater::Relation::Assignable)
    })
}

/// `string -> number` is a real negative: both sides are in the domain where
/// `isSimpleTypeRelatedTo` is a complete decision procedure, so its silence is
/// an answer.
///
/// Reddened by: adding `TypeFlags::OBJECT` to `FLAG_DECIDABLE` would not move
/// this one, but removing `STRING` or `NUMBER` from it turns this `NotRelated`
/// into `Unknown` — which is the mutation that matters, because a relation that
/// cannot say "no" about two primitives refuses every overload set.
#[test]
fn two_unrelated_primitives_are_decidably_not_related() {
    assert_eq!(verdict("let a: string; let b: number;"), tsr_checker::relater::Ternary::NotRelated);
}

/// Nothing but `never` is assignable to `never`, and that is a **decision**.
///
/// This is the one `Some(false)` arm in `is_simple_type_related_to`. Upstream
/// returns there rather than falling through (`internal/checker/relater.go`), so
/// folding it into "no arm fired" would make every `X -> never` pair `Unknown`
/// as soon as `X` is an object type — the reason the arm is not an absence.
#[test]
fn nothing_but_never_is_assignable_to_never_and_that_is_decided() {
    assert_eq!(
        verdict("let a: { x: string }; let b: never;"),
        tsr_checker::relater::Ternary::NotRelated
    );
}

/// Two object types that genuinely match still answer `Related` — the ternary
/// did not turn structural comparison into a mass refusal.
#[test]
fn a_structural_match_is_still_related() {
    assert_eq!(
        verdict("let a: { x: string }; let b: { x: string };"),
        tsr_checker::relater::Ternary::Related
    );
}

/// A property present on both sides with decidably unrelated types is a real
/// negative, not an absence — the property walk must not launder a `NotRelated`
/// constituent into `Unknown`.
#[test]
fn a_property_that_decidably_mismatches_is_not_related() {
    assert_eq!(
        verdict("let a: { x: string }; let b: { x: number };"),
        tsr_checker::relater::Ternary::NotRelated
    );
}

/// **Row 2 of §2 — half-retired by §15, the nineteenth stand-in to come
/// due.** Optionality is now read off the declaration's postfix `?`: an
/// optional target property missing from the source is `Related` under
/// assignability, exactly as upstream's `propertiesRelatedTo` skips it.
/// The subtype relations still require it from interface-backed sources
/// (`requireOptionalProperties`), which keeps reduction ordered.
#[test]
fn an_absent_property_that_may_be_optional_is_unknown() {
    assert_eq!(
        verdict("let a: { x: string }; let b: { x: string, y?: number };"),
        tsr_checker::relater::Ternary::Related
    );
}

/// **Row 6 of §2**, the load-bearing one: for a signature-bearing pair the
/// comparison is unsound in *both* directions at once — the signatures are
/// never compared, so the property walk can neither reject nor accept honestly.
///
/// This is why no per-type flag predicate could separate the trustworthy pairs
/// from the rest, and therefore why `SELECTABLE` could never have been widened
/// into the fix.
#[test]
fn a_signature_bearing_pair_is_unknown() {
    assert_eq!(
        verdict("let a: { x: string }; let b: { x: string, (): void };"),
        tsr_checker::relater::Ternary::Unknown
    );
}

/// Kleene conjunction over a source union: every constituent must be related,
/// and an `Unknown` constituent makes the whole thing `Unknown` rather than a
/// rejection.
#[test]
fn a_source_union_composes_by_kleene_conjunction() {
    assert_eq!(
        verdict("let a: \"x\" | \"y\"; let b: string;"),
        tsr_checker::relater::Ternary::Related
    );
    // One constituent is decidably not related, which beats any `Unknown`
    // elsewhere in the list: a definite negative is the better answer.
    assert_eq!(
        verdict("let a: string | number; let b: string;"),
        tsr_checker::relater::Ternary::NotRelated
    );
}

/// **The projection property.** `is_type_assignable_to` is *defined* as
/// `relate_ternary(..) == Related`, which is what makes this file's other
/// twenty-odd tests statements about the ternary walk as well. Asserted over
/// every shape above so the two can never drift apart silently.
#[test]
fn the_binary_relation_is_the_ternary_projected() {
    for source in [
        "let a: string; let b: number;",
        "let a: \"x\"; let b: string;",
        "let a: { x: string }; let b: { x: string };",
        "let a: { x: string }; let b: { x: number };",
        "let a: { x: string }; let b: { x: string, y?: number };",
        "let a: { x: string }; let b: { x: string, (): void };",
        "let a: string | number; let b: string;",
    ] {
        let ternary = verdict(source);
        let binary = assignable(source);
        assert_eq!(
            binary,
            ternary == tsr_checker::relater::Ternary::Related,
            "projection broke for `{source}`: binary {binary}, ternary {ternary:?}"
        );
    }
}

/// `isTypeComparableTo` in one direction (§750).
fn comparable(source: &str) -> bool {
    with_checker(source, |checker, statements| {
        let a = annotation_type(checker, statements, 0);
        let b = annotation_type(checker, statements, 1);
        checker.is_type_comparable_to(a, b)
    })
}

/// §750. Comparability tries the simple arms in BOTH directions
/// (`relater.go:181`): `string` is comparable to `"a"` although it is not
/// assignable to it.
///
/// Reddened by: removing the reversed simple-arm test in `is_related_to`.
#[test]
fn a_primitive_is_comparable_to_its_own_literal_in_both_directions() {
    assert!(comparable("let a: string; let b: \"a\";"));
    assert!(comparable("let a: \"a\"; let b: string;"));
    assert_eq!(both_ways("let a: string; let b: \"a\";"), (false, true));
}

/// §750. Two distinct literals of one base are NOT comparable — the reversed
/// arm relates a literal to its base, never one literal to another.
#[test]
fn two_distinct_literals_are_not_comparable() {
    assert!(!comparable("let a: \"a\"; let b: \"b\";"));
    assert!(!comparable("let a: 1; let b: 2;"));
    assert!(!comparable("let a: string; let b: number;"));
}

/// §750. A source UNION is comparable when SOME constituent is
/// (`relater.go:2870`), where assignability needs EVERY constituent.
///
/// Reddened by: dropping the `Comparable` branch in the source-union arm.
#[test]
fn a_source_union_needs_only_one_comparable_constituent() {
    let fixture = "let a: \"a\" | \"b\"; let b: \"a\";";
    assert!(comparable(fixture));
    assert!(!assignable(fixture));
}

/// §750. `never` is never a comparable TARGET through the reversed arm: the
/// reversal is gated on the target not being `never` (`relater.go:181`), so
/// `string` against `never` stays unrelated even though `never` relates to
/// `string`.
#[test]
fn never_does_not_become_comparable_through_the_reversed_arm() {
    assert!(!comparable("let a: string; let b: never;"));
    assert!(comparable("let a: never; let b: string;"));
}

/// §750. `any` is comparable in both directions (the assignable-only simple
/// arms are shared with the comparable relation, `relater.go:261`).
#[test]
fn any_is_comparable_to_everything_both_ways() {
    assert!(comparable("let a: any; let b: string;"));
    assert!(comparable("let a: string; let b: any;"));
}

/// The type of the `index`th statement's first declaration: its annotation
/// when it has one, else its initializer's REGULAR type. The initializer
/// road is how an enum member is reached here — the bare harness has no
/// program, and the qualified type-reference road (`E.A` in type position)
/// resolves its namespace root only after the enum's declared type exists;
/// the corpus pipeline never hits that order. §751.
fn declaration_type<'a>(
    checker: &mut Checker<'a, '_>,
    statements: &[Statement<'a>],
    index: usize,
) -> TypeId {
    let Statement::VariableStatement(statement) = statements[index] else {
        panic!("statement {index} must be a variable statement");
    };
    let declaration = statement
        .declaration_list
        .and_then(|list| list.declarations.first().copied())
        .expect("a declaration");
    if let Some(annotation) = declaration.r#type {
        return checker.get_type_from_type_node(annotation);
    }
    let initializer = declaration.initializer.expect("an annotation or an initializer");
    let fresh = checker.check_expression(initializer);
    checker.get_regular_type_of_literal_type(fresh)
}

/// Fixtures with a declaration ahead of the two `let`s: the two types are
/// statements 1 and 2.
fn related_after_decl(source: &str, ask: fn(&mut Checker<'_, '_>, TypeId, TypeId) -> bool) -> bool {
    with_checker(source, |checker, statements| {
        let a = declaration_type(checker, statements, 1);
        let b = declaration_type(checker, statements, 2);
        ask(checker, a, b)
    })
}

/// §751 (`relater.go:225`/`:267`/`:268`). A numeric enum member is
/// assignable to `number` and to the plain literal of its value; `number`
/// and that literal are assignable BACK to it under the assignable relation
/// (the bit-flag rules) and not under the subtype relation.
#[test]
#[allow(clippy::redundant_closure_for_method_calls)]
fn a_numeric_enum_member_relates_to_number_and_its_value_literal() {
    let to_number = "enum E { A, B } const a = E.A; let b: number;";
    assert!(related_after_decl(to_number, |c, a, b| c.is_type_assignable_to(a, b)));
    assert!(related_after_decl(to_number, |c, a, b| c.is_type_assignable_to(b, a)));
    assert!(!related_after_decl(to_number, |c, a, b| c.is_type_subtype_of(b, a)));
    let to_literal = "enum E { A, B } const a = E.B; let b: 1;";
    assert!(related_after_decl(to_literal, |c, a, b| c.is_type_assignable_to(a, b)));
    assert!(related_after_decl(to_literal, |c, a, b| c.is_type_assignable_to(b, a)));
    let wrong_literal = "enum E { A, B } const a = E.B; let b: 0;";
    assert!(!related_after_decl(wrong_literal, |c, a, b| c.is_type_assignable_to(a, b)));
    assert!(!related_after_decl(wrong_literal, |c, a, b| c.is_type_assignable_to(b, a)));
}

/// §751. A STRING enum member is string-like, not number-like — the port's
/// `ENUM` bit sits inside `NUMBER_LIKE`, which is exactly the misfire the
/// enum arms are placed ahead of.
///
/// Reddened by: moving the enum block below the `NUMBER_LIKE` arm.
#[test]
#[allow(clippy::redundant_closure_for_method_calls)]
fn a_string_enum_member_is_a_string_and_not_a_number() {
    let to_string = "enum S { X = \"x\", Y = \"y\" } const a = S.X; let b: string;";
    assert!(related_after_decl(to_string, |c, a, b| c.is_type_assignable_to(a, b)));
    let to_number = "enum S { X = \"x\", Y = \"y\" } const a = S.X; let b: number;";
    assert!(!related_after_decl(to_number, |c, a, b| c.is_type_assignable_to(a, b)));
    let to_literal = "enum S { X = \"x\", Y = \"y\" } const a = S.X; let b: \"x\";";
    assert!(related_after_decl(to_literal, |c, a, b| c.is_type_assignable_to(a, b)));
}

/// §751 (`relater.go:236-243`). Two members of one enum are unrelated, and
/// that is DECIDED (`relate_ternary` reads `NotRelated`, not `Unknown`).
#[test]
#[allow(clippy::redundant_closure_for_method_calls)]
fn two_members_of_one_enum_are_decidably_unrelated() {
    use tsr_checker::relater::{Relation, Ternary};
    let fixture = "enum E { A, B } const a = E.A; const b = E.B;";
    assert!(!related_after_decl(fixture, |c, a, b| c.is_type_assignable_to(a, b)));
    let verdict = with_checker(fixture, |checker, statements| {
        let a = declaration_type(checker, statements, 1);
        let b = declaration_type(checker, statements, 2);
        checker.relate_ternary(a, b, Relation::Assignable)
    });
    assert_eq!(verdict, Ternary::NotRelated);
}
