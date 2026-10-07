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
fn primitive_values_cannot_inhabit_an_arbitrary_type_parameter() {
    use tsr_checker::relater::{Relation, Ternary};
    for strict in [false, true] {
        for source in [
            "string",
            "number",
            "boolean",
            "bigint",
            "symbol",
            "\"literal\"",
            "42",
            "true",
            "1n",
            "null",
            "undefined",
            "void",
        ] {
            with_checker(
                &format!("function f<T>(source:{source}, target:T) {{}}"),
                |checker, statements| {
                    checker.set_strict_null_checks(strict);
                    let Statement::FunctionDeclaration(function) = statements[0] else {
                        panic!("function")
                    };
                    let from =
                        checker.get_type_from_type_node(function.parameters[0].r#type.unwrap());
                    let to =
                        checker.get_type_from_type_node(function.parameters[1].r#type.unwrap());
                    let expected = if !strict && matches!(source, "null" | "undefined") {
                        Ternary::Related
                    } else {
                        Ternary::NotRelated
                    };
                    assert_eq!(
                        checker.relate_ternary(from, to, Relation::Assignable),
                        expected,
                        "{source}, strict={strict}"
                    );
                },
            );
        }
    }
}

#[test]
fn unknown_does_not_inhabit_a_parameter_even_when_it_satisfies_the_constraint() {
    use tsr_checker::relater::{Relation, Ternary};
    for strict in [false, true] {
        for bound in ["", " extends string", " extends unknown", " extends any", " extends never"] {
            with_checker(
                &format!("function f<T{bound}>(source:unknown, target:T) {{}}"),
                |checker, statements| {
                    checker.set_strict_null_checks(strict);
                    let Statement::FunctionDeclaration(function) = statements[0] else {
                        panic!("function")
                    };
                    let from =
                        checker.get_type_from_type_node(function.parameters[0].r#type.unwrap());
                    let to =
                        checker.get_type_from_type_node(function.parameters[1].r#type.unwrap());
                    assert_eq!(
                        checker.relate_ternary(from, to, Relation::Assignable),
                        Ternary::NotRelated
                    );
                    assert_eq!(
                        checker.relate_ternary(to, from, Relation::Assignable),
                        Ternary::Related
                    );
                    assert_eq!(
                        checker.relate_ternary(from, to, Relation::Comparable),
                        Ternary::Related
                    );
                },
            );
        }
    }
}

#[test]
fn any_and_never_keep_their_type_parameter_assignability() {
    use tsr_checker::relater::{Relation, Ternary};
    for source in ["any", "never"] {
        with_checker(
            &format!("function f<T>(source:{source}, target:T) {{}}"),
            |checker, statements| {
                let Statement::FunctionDeclaration(function) = statements[0] else {
                    panic!("function")
                };
                let from = checker.get_type_from_type_node(function.parameters[0].r#type.unwrap());
                let to = checker.get_type_from_type_node(function.parameters[1].r#type.unwrap());
                assert_eq!(
                    checker.relate_ternary(from, to, Relation::Assignable),
                    Ternary::Related
                );
            },
        );
    }
}

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

#[test]
fn assignability_follows_constraints_without_accepting_the_reverse() {
    use tsr_checker::relater::{Relation, Ternary};
    for (parameters, source, target) in [
        ("T extends string", "T", "string"),
        ("T extends string, U extends T", "U", "string"),
        ("T extends string | number", "T", "string | number"),
        ("T, U extends T", "U", "T"),
        ("T", "T", "unknown"),
    ] {
        with_checker(
            &format!("function f<{parameters}>(x: {source}, y: {target}) {{}}"),
            |checker, statements| {
                let Statement::FunctionDeclaration(function) = statements[0] else {
                    panic!("function")
                };
                let source =
                    checker.get_type_from_type_node(function.parameters[0].r#type.unwrap());
                let target =
                    checker.get_type_from_type_node(function.parameters[1].r#type.unwrap());
                assert_eq!(
                    checker.relate_ternary(source, target, Relation::Assignable),
                    Ternary::Related
                );
                assert_ne!(
                    checker.relate_ternary(target, source, Relation::Assignable),
                    Ternary::Related
                );
            },
        );
    }
}

#[test]
fn type_variable_union_members_are_compared_before_their_constraints() {
    use tsr_checker::relater::{Relation, Ternary};
    with_checker("function f<T, U>(x: T, y: T | U) {}", |checker, statements| {
        let Statement::FunctionDeclaration(function) = statements[0] else { panic!("function") };
        let source = checker.get_type_from_type_node(function.parameters[0].r#type.unwrap());
        let target = checker.get_type_from_type_node(function.parameters[1].r#type.unwrap());
        for relation in [Relation::Assignable, Relation::Subtype] {
            assert_eq!(checker.relate_ternary(source, target, relation), Ternary::Related);
        }
    });
}

#[test]
fn exact_never_constraints_and_circular_constraints_remain_distinct() {
    use tsr_checker::relater::{Relation, Ternary};
    for (source, expected) in [
        ("function f<T extends never>(x:T,y:never) {}", Ternary::Related),
        ("function f<T extends U,U extends T>(x:T,y:{value:number}) {}", Ternary::Unknown),
    ] {
        with_checker(source, |checker, statements| {
            let Statement::FunctionDeclaration(function) = statements[0] else {
                panic!("function")
            };
            let source = checker.get_type_from_type_node(function.parameters[0].r#type.unwrap());
            let target = checker.get_type_from_type_node(function.parameters[1].r#type.unwrap());
            assert_eq!(checker.relate_ternary(source, target, Relation::Assignable), expected);
        });
    }
}

#[test]
fn combined_constraints_preserve_identities_and_reduce_disjoint_domains() {
    use tsr_checker::relater::{Relation, Ternary};
    for (parameters, source, target) in [
        ("T extends string | number", "T", "string | number | boolean"),
        ("T extends string | number, U extends number | boolean", "T & U", "number | bigint"),
        ("T extends 1 | 2, U extends 2 | 3", "T & U", "T & U & (1 | 2 | 3)"),
        ("T extends string | undefined", "T & {}", "string"),
        (
            "T extends { a: string | number }, U extends number | boolean",
            "T['a'] & U",
            "number | bigint",
        ),
        ("T extends string | number, U extends boolean | bigint", "T & U", "bigint | null"),
    ] {
        with_checker(
            &format!("function f<{parameters}>(x: {source}, y: {target}) {{}}"),
            |checker, statements| {
                let Statement::FunctionDeclaration(function) = statements[0] else {
                    panic!("function")
                };
                let from = checker.get_type_from_type_node(function.parameters[0].r#type.unwrap());
                let to = checker.get_type_from_type_node(function.parameters[1].r#type.unwrap());
                for relation in [Relation::Assignable, Relation::Subtype] {
                    assert_eq!(
                        checker.relate_ternary(from, to, relation),
                        Ternary::Related,
                        "{source} -> {target}"
                    );
                }
            },
        );
    }
}

#[test]
fn construct_signatures_compare_parameters_returns_and_abstractness() {
    use tsr_checker::relater::{Relation, Ternary};
    for (source, target, expected) in [
        ("new (x:number)=>{value:number;extra:string}", "new (x:number)=>{value:number}", true),
        ("new (x:number)=>string", "new (x:number)=>number", false),
        ("new (x:string)=>number", "new (x:number)=>number", false),
        ("new <T>(x:T)=>T", "new (x:number)=>number", true),
        ("abstract new (x:number)=>number", "new (x:number)=>number", false),
        ("new (x:number)=>number", "abstract new (x:number)=>number", true),
        ("() => number", "new () => number", false),
        ("new () => number", "() => number", false),
        ("{new (x:number):number;new (x:string):string;}", "new(x:string)=>string", true),
        ("{():number;new():string;}", "{():number;new():number;}", false),
    ] {
        with_checker(
            &format!("let source:{source};let target:{target};"),
            |checker, statements| {
                let source_type = annotation_type(checker, statements, 0);
                let target_type = annotation_type(checker, statements, 1);
                for relation in [Relation::Assignable, Relation::Subtype] {
                    assert_eq!(
                        checker.relate_ternary(source_type, target_type, relation),
                        if expected { Ternary::Related } else { Ternary::NotRelated },
                        "{source} -> {target} ({relation:?})"
                    );
                }
            },
        );
    }
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
    // getTypeFromOptionalTypeNode applies optionality when the element type is
    // created, so the options must be fixed before the annotations resolve.
    for exact in [false, true] {
        with_checker(
            "let source: [number | undefined]; let target: [number?];",
            |checker, statements| {
                checker.apply_compiler_options(&tsr_core::CompilerOptions {
                    strict: tsr_core::Tristate::True,
                    exact_optional_property_types: if exact {
                        tsr_core::Tristate::True
                    } else {
                        tsr_core::Tristate::False
                    },
                    ..Default::default()
                });
                let source = annotation_type(checker, statements, 0);
                let target = annotation_type(checker, statements, 1);
                assert_eq!(checker.is_type_assignable_to(source, target), !exact);
            },
        );
    }
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
fn recursive_generic_methods_preserve_covariant_values() {
    let source = "interface Recursive<T> {
        value: T; next: Recursive<T>;
        map<U>(mapper: (value: T) => U): Recursive<U>;
    }
    let broad: Recursive<string>; let literal: Recursive<'a'>; let numeric: Recursive<number>;";
    with_checker(source, |checker, statements| {
        let broad = annotation_type(checker, statements, 1);
        let literal = annotation_type(checker, statements, 2);
        let numeric = annotation_type(checker, statements, 3);
        assert!(checker.is_type_assignable_to(literal, broad));
        assert!(!checker.is_type_assignable_to(broad, literal));
        assert!(!checker.is_type_assignable_to(broad, numeric));
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
/// Reddened by: removing the active pair from `maybe_keys_set` before recursing
/// in `recursive_type_related_to`. The test then
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

/// A recursive proof from a rejected union branch cannot prove a later branch.
/// Pinned tsgo rejects both union orders: `next.next.value` eventually differs.
#[test]
fn failed_union_branch_discards_dependent_recursive_assumptions() {
    use tsr_checker::relater::{Relation, Ternary};
    for target in ["C | E", "E | C"] {
        let source = format!(
            "interface A {{ next: B; value: string }}\n\
             interface B {{ next: A }}\n\
             interface C {{ next: D; value: number }}\n\
             interface D {{ next: C }}\n\
             interface E {{ next: D; value: string }}\n\
             let source: A; let target: {target};"
        );
        with_checker(&source, |checker, statements| {
            let from = annotation_type(checker, statements, 5);
            let to = annotation_type(checker, statements, 6);
            assert_eq!(
                checker.relate_ternary(from, to, Relation::Assignable),
                Ternary::NotRelated,
                "failed branch assumptions escaped into {target}"
            );
        });
    }
}

#[test]
fn failed_union_branch_preserves_a_later_valid_recursive_proof() {
    use tsr_checker::relater::{Relation, Ternary};
    let source = "interface A { next: B; value: string }\n\
                  interface B { next: A }\n\
                  interface C { next: D; value: number }\n\
                  interface D { next: C }\n\
                  interface E { next: F; value: string }\n\
                  interface F { next: E }\n\
                  let source: A; let target: C | E;";
    with_checker(source, |checker, statements| {
        let from = annotation_type(checker, statements, 6);
        let to = annotation_type(checker, statements, 7);
        assert_eq!(checker.relate_ternary(from, to, Relation::Assignable), Ternary::Related);
    });
}

/// A depth refusal must not discharge a recursive child for a later branch.
#[test]
fn unknown_union_branch_discards_dependent_recursive_assumptions() {
    use tsr_checker::relater::{Relation, Ternary};
    let depth = tsr_checker::relater::MAX_DEPTH + 10;
    let mut source = String::from("interface L0 { x: string }\ninterface R0 { x: string }\n");
    for index in 1..=depth {
        writeln!(source, "interface L{index} {{ x: L{} }}", index - 1).unwrap();
        writeln!(source, "interface R{index} {{ x: R{} }}", index - 1).unwrap();
    }
    writeln!(
        source,
        "interface A {{ next: B; value: L{depth} }}\n\
         interface B {{ next: A }}\n\
         interface C {{ next: D; value: R{depth} }}\n\
         interface D {{ next: C }}\n\
         interface E {{ next: D; value: L{depth} }}\n\
         let source: A; let target: C | E;"
    )
    .unwrap();
    let annotations = 2 + 2 * depth + 5;
    with_checker(&source, |checker, statements| {
        let from = annotation_type(checker, statements, annotations);
        let to = annotation_type(checker, statements, annotations + 1);
        assert_eq!(checker.relate_ternary(from, to, Relation::Assignable), Ternary::Unknown);
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
fn a_missing_required_call_signature_rejects() {
    assert_eq!(
        verdict("let a: { x: string }; let b: { x: string, (): void };"),
        tsr_checker::relater::Ternary::NotRelated
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

#[test]
#[allow(clippy::redundant_closure_for_method_calls)]
fn nonfinite_enum_values_match_plain_literals_without_losing_owner_identity() {
    for value in ["1e999", "-2e999"] {
        let fixture = format!("enum E {{ A = {value}, B = 7 }} const a = E.A; let b: {value};");
        assert!(related_after_decl(&fixture, |c, a, b| c.is_type_assignable_to(a, b)));
        assert!(related_after_decl(&fixture, |c, a, b| c.is_type_assignable_to(b, a)));
    }
    let opposite = "enum E { A = 1e999, B = 7 } const a = E.A; let b: -1e999;";
    assert!(!related_after_decl(opposite, |c, a, b| c.is_type_assignable_to(a, b)));
    assert!(!related_after_decl(opposite, |c, a, b| c.is_type_assignable_to(b, a)));
    with_checker(
        "enum E { A = 1e999, B = 7 } enum F { A = 1e999, B = 7 } const a = E.A; const b = F.A;",
        |checker, statements| {
            let a = declaration_type(checker, statements, 2);
            let b = declaration_type(checker, statements, 3);
            assert!(!checker.is_type_assignable_to(a, b));
            assert!(!checker.is_type_assignable_to(b, a));
        },
    );
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

#[test]
fn source_intersection_members_decide_both_success_and_missing_requirements() {
    use tsr_checker::relater::{Relation, Ternary};
    for (source, member, expected) in [
        ("{ a: string } & { d: boolean }", "string", Ternary::Related),
        ("{ d: boolean } & { a: string }", "string", Ternary::Related),
        ("{ a: number } & { d: boolean }", "string", Ternary::NotRelated),
        ("{ a: string } & { other: number }", "string", Ternary::NotRelated),
        ("{ a?: string } & { d: boolean }", "any", Ternary::NotRelated),
    ] {
        for exact in [false, true] {
            with_checker(
                &format!("let source: {source}; let target: {{ a: {member}; d: boolean }};"),
                |checker, statements| {
                    checker.apply_compiler_options(&tsr_core::CompilerOptions {
                        strict: tsr_core::Tristate::True,
                        exact_optional_property_types: if exact {
                            tsr_core::Tristate::True
                        } else {
                            tsr_core::Tristate::False
                        },
                        ..Default::default()
                    });
                    let a = annotation_type(checker, statements, 0);
                    let b = annotation_type(checker, statements, 1);
                    assert_eq!(checker.relate_ternary(a, b, Relation::Assignable), expected);
                    if member == "any" {
                        assert_eq!(
                            checker.relate_ternary(a, b, Relation::Comparable),
                            Ternary::Related,
                            "comparability skips optional-source/required-target rejection"
                        );
                    }
                },
            );
        }
    }
}

#[test]
fn combined_readonly_flags_order_only_strict_subtypes_and_use_all_contributions() {
    use tsr_checker::relater::{Relation, Ternary};
    for (source, strict_expected) in [
        ("{ readonly a: string } & { d: boolean }", Ternary::NotRelated),
        ("{ readonly a: string } & { a: string } & { d: boolean }", Ternary::Related),
        ("{ a: string } & { readonly a: string } & { d: boolean }", Ternary::Related),
    ] {
        with_checker(
            &format!("let source: {source}; let target: {{ a: string; d: boolean }};"),
            |checker, statements| {
                let a = annotation_type(checker, statements, 0);
                let b = annotation_type(checker, statements, 1);
                assert_eq!(checker.relate_ternary(a, b, Relation::Assignable), Ternary::Related);
                assert_eq!(checker.relate_ternary(a, b, Relation::Subtype), Ternary::Related);
                assert_eq!(checker.relate_ternary(a, b, Relation::StrictSubtype), strict_expected);
            },
        );
    }
}

#[test]
fn bounded_mapped_source_intersections_admit_complete_member_proofs() {
    use tsr_checker::relater::{Relation, Ternary};
    for (bound, source, target, expected) in [
        ("{ a: string; extra: number }", "Req<T> & { d: boolean }", "State", Ternary::Related),
        ("{ a: string; extra: number }", "{ d: boolean } & Req<T>", "State", Ternary::Related),
        (
            "{ a: string; extra: number }",
            "Part<T> & { d: boolean }",
            "OptionalState",
            Ternary::Related,
        ),
        (
            "{ a: string; extra: number }",
            "{ d: boolean } & Part<T>",
            "OptionalState",
            Ternary::Related,
        ),
        ("{ a?: string }", "Req<T> & { d: boolean }", "State", Ternary::NotRelated),
        ("{ a: number }", "Req<T> & { d: boolean }", "State", Ternary::NotRelated),
        ("{ a: string }", "Part<T> & { d: boolean }", "RequiredAny", Ternary::NotRelated),
        ("{ other: string }", "Req<T> & { d: boolean }", "State", Ternary::NotRelated),
    ] {
        for exact in [false, true] {
            let source_text = format!(
                "type Req<T> = {{ [P in keyof T]-?: T[P] }};
                type Part<T> = {{ [P in keyof T]?: T[P] }};
                interface State {{ a: string; d: boolean }}
                interface OptionalState {{ a?: string; d: boolean }}
                interface RequiredAny {{ a: any; d: boolean }}
                function f<T extends {bound}>(source: {source}, target: {target}) {{}}"
            );
            with_checker(&source_text, |checker, statements| {
                checker.apply_compiler_options(&tsr_core::CompilerOptions {
                    strict: tsr_core::Tristate::True,
                    exact_optional_property_types: if exact {
                        tsr_core::Tristate::True
                    } else {
                        tsr_core::Tristate::False
                    },
                    ..Default::default()
                });
                let Statement::FunctionDeclaration(function) = statements.last().unwrap() else {
                    panic!("function");
                };
                let from = checker.get_type_from_type_node(function.parameters[0].r#type.unwrap());
                let to = checker.get_type_from_type_node(function.parameters[1].r#type.unwrap());
                assert_eq!(
                    checker.relate_ternary(from, to, Relation::Assignable),
                    expected,
                    "{source}, T extends {bound}, target {target}, exact={exact}"
                );
            });
        }
    }
}

#[test]
fn unsupported_mapped_source_intersection_shapes_retain_unknown() {
    use tsr_checker::relater::{Relation, Ternary};
    for (parameters, source, target) in [
        ("K extends 'a'", "{ [P in K]: string } & { d: boolean }", "State"),
        (
            "T extends { a: string; first: number } | { a: string; second: boolean }",
            "Req<T> & { d: boolean }",
            "State",
        ),
        ("T extends { a: string }", "Req<T> & { d: boolean }", "ProtectedState"),
        ("T extends PrivateState", "Req<T> & { extra: number }", "PrivateState"),
    ] {
        let source_text = format!(
            "type Req<T> = {{ [P in keyof T]-?: T[P] }};
            interface State {{ a: string; d: boolean }}
            declare class ProtectedState {{ protected a: string; d: boolean }}
            declare class PrivateState {{ private a: string; d: boolean }}
            function f<{parameters}>(source: {source}, target: {target}) {{}}"
        );
        with_checker(&source_text, |checker, statements| {
            checker.set_strict_null_checks(true);
            let Statement::FunctionDeclaration(function) = statements.last().unwrap() else {
                panic!("function");
            };
            let from = checker.get_type_from_type_node(function.parameters[0].r#type.unwrap());
            let to = checker.get_type_from_type_node(function.parameters[1].r#type.unwrap());
            assert_eq!(
                checker.relate_ternary(from, to, Relation::Assignable),
                Ternary::Unknown,
                "{source}, {parameters}, target {target}"
            );
        });
    }
}

#[test]
fn bounded_mapped_source_admission_preserves_constraint_call_results() {
    for (parameters, source, expected) in [
        ("T extends { a: string }", "T & { d: boolean }", "T & { d: boolean; }"),
        (
            "T extends { a: string; first: number } | { a: string; second: boolean }",
            "T & { d: boolean }",
            "State",
        ),
        ("T extends { a: string }", "Req<T> & { d: boolean }", "Req<T> & { d: boolean; }"),
    ] {
        let source_text = format!(
            "type Req<T> = {{ [P in keyof T]-?: T[P] }};
            interface State {{ a: string; d: boolean }}
            declare function identity<T extends State>(value: T): T;
            function f<{parameters}>(source: {source}) {{ const result = identity(source); }}"
        );
        with_checker(&source_text, |checker, statements| {
            checker.set_strict_null_checks(true);
            let Statement::FunctionDeclaration(function) = statements.last().unwrap() else {
                panic!("function");
            };
            let tsr_ast::FunctionBody::Block(body) = function.body.unwrap();
            let Statement::VariableStatement(statement) = body.statements[0] else {
                panic!("variable");
            };
            let initializer =
                statement.declaration_list.unwrap().declarations[0].initializer.unwrap();
            let result = checker.check_expression(initializer);
            assert_eq!(checker.type_to_string(result), expected, "{parameters}, {source}");
        });
    }
}

#[test]
fn closed_mapped_targets_keep_their_intersection_callback_context() {
    let source = "declare function createSlice<T>(
        reducers: { [K: string]: (state: string) => void } & { [K in keyof T]: object }
    ): void;
    const result = createSlice({ f(a) {} });";
    with_checker(source, |checker, statements| {
        checker.apply_compiler_options(&tsr_core::CompilerOptions {
            strict: tsr_core::Tristate::True,
            ..Default::default()
        });
        let Statement::VariableStatement(statement) = statements.last().unwrap() else {
            panic!("variable");
        };
        let initializer = statement.declaration_list.unwrap().declarations[0].initializer.unwrap();
        let tsr_ast::Expression::CallExpression(call) = initializer else {
            panic!("call");
        };
        checker.check_expression(initializer);
        let argument = checker.check_expression(call.arguments[0]);
        assert_eq!(checker.type_to_string(argument), "{ f(a: string): void; }");
    });
}

#[test]
fn source_intersections_reduce_only_certified_whole_object_conflicts() {
    use tsr_checker::relater::{Relation, Ternary};
    for (bound, left, right, reduced) in [
        (" extends { a?: number }", "T", "{ a: 'ok' }", true),
        ("", "{ a: 'left' }", "{ a: 'right' }", true),
        ("", "{ a: number }", "{ a: 'ok' }", true),
        (" extends { a?: number }", "T", "{ a?: 'ok' }", false),
        ("", "{ a?: 'left' }", "{ a?: 'right' }", false),
        ("", "{ a: string }", "{ a: number }", false),
        ("", "{ a: never }", "{ a: 'ok' }", false),
        ("", "FirstPrivate", "SecondPrivate", true),
        ("", "FirstPrivate", "{ a: string }", true),
        ("", "PrivateBase", "InheritedPrivate", false),
        ("", "T", "{ a: 'ok' }", false),
        (" extends { a: string }", "T", "{}", false),
        (" extends { a: number }", "Part<T>", "{ a?: 'ok' }", false),
    ] {
        for exact in [false, true] {
            for reverse in [false, true] {
                let source = if reverse {
                    format!("{{ d: boolean }} & {right} & {left}")
                } else {
                    format!("{left} & {right} & {{ d: boolean }}")
                };
                let text = format!(
                    "type Part<T> = {{ [P in keyof T]?: T[P] }};
                    declare class FirstPrivate {{ private a: string; d: boolean }}
                    declare class SecondPrivate {{ private a: string; d: boolean }}
                    declare class PrivateBase {{ private a: string; d: boolean }}
                    declare class InheritedPrivate extends PrivateBase {{ extra: number }}
                    interface MissingState {{ a: string; d: boolean; absent: number }}
                    function f<T{bound}>(source: {source}, bottom: never,
                        primitive: number, missing: MissingState) {{}}"
                );
                with_checker(&text, |checker, statements| {
                    checker.apply_compiler_options(&tsr_core::CompilerOptions {
                        strict: tsr_core::Tristate::True,
                        exact_optional_property_types: if exact {
                            tsr_core::Tristate::True
                        } else {
                            tsr_core::Tristate::False
                        },
                        ..Default::default()
                    });
                    let Statement::FunctionDeclaration(function) = statements.last().unwrap()
                    else {
                        panic!("function");
                    };
                    let from =
                        checker.get_type_from_type_node(function.parameters[0].r#type.unwrap());
                    let bottom =
                        checker.get_type_from_type_node(function.parameters[1].r#type.unwrap());
                    assert_eq!(
                        checker.relate_ternary(from, bottom, Relation::Assignable),
                        if reduced { Ternary::Related } else { Ternary::NotRelated },
                        "{source}, T{bound}, exact={exact}"
                    );
                    if reduced {
                        // A property merely valued never cannot satisfy a
                        // primitive target or a genuinely missing member.
                        for parameter in &function.parameters[2..] {
                            let target = checker.get_type_from_type_node(parameter.r#type.unwrap());
                            assert_eq!(
                                checker.relate_ternary(from, target, Relation::Assignable),
                                Ternary::Related,
                                "{source}, exact={exact}"
                            );
                        }
                    }
                });
            }
        }
    }
}

#[test]
fn source_intersection_relation_reduction_preserves_written_identity_and_reads() {
    use tsr_checker::relater::{Relation, Ternary};
    with_checker(
        "let source: { a: 'left' } & { a: 'right' } & { d: boolean }; let target: never;",
        |checker, statements| {
            checker.set_strict_null_checks(true);
            let source = annotation_type(checker, statements, 0);
            let target = annotation_type(checker, statements, 1);
            let written = checker.type_to_string(source);
            let member = checker.get_type_of_property_of_type(source, "d");
            assert_eq!(member, None, "whole-never intersection has no apparent members");
            assert_eq!(
                checker.relate_ternary(source, target, Relation::Assignable),
                Ternary::Related
            );
            assert_eq!(annotation_type(checker, statements, 0), source);
            assert_eq!(checker.type_to_string(source), written);
            assert_eq!(checker.get_type_of_property_of_type(source, "d"), member);
        },
    );
}

#[test]
fn comparable_primitive_targets_hoist_intersection_constraints_before_some() {
    use tsr_checker::relater::{Relation, Ternary};
    // Pinned 5b1047d relater.go:2884: an unconstrained T cannot make
    // T & null comparable to 42, nor may T extends 1 | 2 make T & 1
    // comparable to 2. The literal/structural positive controls prevent
    // rejecting every generic intersection instead of reducing it locally.
    for (bound, source, target, expected) in [
        ("", "T & null", "42", Ternary::NotRelated),
        (" extends {} | null", "T & null", "42", Ternary::NotRelated),
        (" extends 1 | 2", "T & 1", "2", Ternary::NotRelated),
        (" extends 1 | 2", "T & 1", "1", Ternary::Related),
        ("", "T & null", "null", Ternary::Related),
        (" extends string", "T & { tag: 'x' }", "number", Ternary::NotRelated),
        ("", "T & { tag: 'x' }", "{ tag: 'x' }", Ternary::Related),
    ] {
        for reverse_first in [false, true] {
            with_checker(
                &format!("function f<T{bound}>(source: {source}, target: {target}) {{}}"),
                |checker, statements| {
                    checker.set_strict_null_checks(true);
                    let Statement::FunctionDeclaration(function) = statements[0] else {
                        panic!("function")
                    };
                    let source_node = function.parameters[0].r#type.unwrap();
                    let from = checker.get_type_from_type_node(source_node);
                    let to =
                        checker.get_type_from_type_node(function.parameters[1].r#type.unwrap());
                    let written = checker.type_to_string(from);
                    for reverse in [reverse_first, !reverse_first, reverse_first] {
                        // Comparable is directional. Reverse queries exercise
                        // order/reuse, but only the intersection-source arm
                        // owns this primitive-target constraint projection.
                        let (left, right) = if reverse { (to, from) } else { (from, to) };
                        let answer = checker.relate_ternary(left, right, Relation::Comparable);
                        if !reverse {
                            assert_eq!(answer, expected, "{source} -> {target}, bound={bound}");
                        }
                    }
                    assert_eq!(
                        checker.are_types_comparable(from, to),
                        expected == Ternary::Related,
                        "{source}, {target}, bound={bound}",
                    );
                    assert_eq!(checker.get_type_from_type_node(source_node), from);
                    assert_eq!(checker.type_to_string(from), written);
                },
            );
        }
    }
}

#[test]
fn inferred_parameter_constraint_absence_does_not_certify_primitive_projection() {
    use tsr_ast::TypeNode;
    use tsr_checker::relater::{Relation, Ternary};
    // Native infer U in a rest parameter has an inferred array constraint.
    // This port's base-constraint supplier does not supply that constraint;
    // an unannotated infer declaration is not an unconstrained ordinary T.
    with_checker(
        "type Rest<X> = X extends (...args: infer U) => void ? U & null : never;
         let target: 42;",
        |checker, statements| {
            checker.set_strict_null_checks(true);
            let Statement::TypeAliasDeclaration(alias) = statements[0] else { panic!("alias") };
            let Some(TypeNode::ConditionalTypeNode(conditional)) = alias.r#type else {
                panic!("conditional")
            };
            let from = checker.get_type_from_type_node(conditional.true_type.unwrap());
            let to = annotation_type(checker, statements, 1);
            let written = checker.type_to_string(from);
            assert_eq!(checker.relate_ternary(from, to, Relation::Comparable), Ternary::Unknown,);
            assert_eq!(checker.type_to_string(from), written);
        },
    );
}
