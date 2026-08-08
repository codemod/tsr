//! Which union constituents are parenthesised, and which must not be.
//!
//! Upstream decides on the **node kind** the builder emitted:
//! `emitUnionTypeConstituent` (`printer.go:2038`) is
//! `emitTypeNode(node, TypePrecedenceTypeOperator)`, and `emitTypeNode`
//! (`printer.go:2274`) writes a `(` when `GetTypeNodePrecedence(node)` is below
//! that. This port computes a type's text at creation and has no node, so the
//! same distinction is recovered from what the type carries — which is exactly
//! where it can go wrong, and every case below is one that did or could.
//!
//! **Every expected string here was taken from a `.types` baseline under
//! `vendor/typescript-go/testdata/baselines/reference/submodule` before it was
//! written down**, with the corpus count beside it. That rule is in this
//! project's conventions because three expectations written from intuition in
//! `tests/logical_and.rs` were wrong in one session, and a fourth — filed as
//! `bd tsr-iiu`, "`undefined | null` prints backwards" — turned out to be a
//! defect report against correct code.

use tsr_ast::Statement;
use tsr_checker::Checker;
use tsr_core::Arena;

/// The minimal global `Array`, as `tests/arrays.rs` declares it. An array type
/// is a reference to the global symbol, so without this every `T[]` fixture
/// answers `error` and the test passes or fails for a reason that has nothing
/// to do with parenthesisation.
const LIB: &str = "interface Array<T> {}\n";

/// The rendered type of the **last** statement's declared annotation.
///
/// The last rather than a chosen index: every fixture here needs interfaces or
/// a type alias declared first, and threading an index through was how the
/// first version of this file panicked on six of seven tests.
fn type_of_annotation(source: &str) -> String {
    let source = &format!("{LIB}{source}");
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

    let last = *parsed.source_file.statements.last().expect("a statement");
    let Statement::VariableStatement(statement) = last else {
        panic!("the last statement must be a variable statement");
    };
    let declaration = statement
        .declaration_list
        .and_then(|list| list.declarations.first().copied())
        .expect("one declaration");
    let annotation = declaration.r#type.expect("an annotation");
    let id = checker.get_type_from_type_node(annotation);
    checker.type_to_string(id)
}

/// An **anonymous** intersection is an `IntersectionTypeNode`, which sorts below
/// `TypeOperator` and is parenthesised.
///
/// Baseline: `conformance/intersectionReduction` and neighbours record
/// `(A & B) | (C & D)`.
#[test]
fn an_anonymous_intersection_is_parenthesised() {
    let source = "interface A {} interface B {} interface C {} interface D {} \
                  let x: (A & B) | (C & D);";
    assert_eq!(type_of_annotation(source), "(A & B) | (C & D)");
}

/// **The case that cost 19 lines.** An intersection a type alias names prints as
/// that name, which the builder emits as a `TypeReferenceNode` — `NonArray`,
/// the highest precedence, never parenthesised.
///
/// Baseline: `conformance/indexSignatures1` records
/// `TaggedString1 | TaggedString2`, and the first version of this rule printed
/// `(TaggedString1) | (TaggedString2)`. It tested the *type's variant* where
/// upstream tests the *node kind*, and for an aliased intersection those differ.
#[test]
fn an_aliased_intersection_is_not_parenthesised() {
    let source = "interface A {} interface B {} type Tagged = A & B; \
                  let x: Tagged | number;";
    assert_eq!(type_of_annotation(source), "number | Tagged");
}

/// A `FunctionTypeNode` sorts below `TypeOperator` and is parenthesised.
///
/// Baseline: `((a: number) => number) | undefined` appears 8 times,
/// `((_: A) => boolean) | undefined` and `((a: any) => any) | undefined` once
/// each.
#[test]
fn a_function_type_is_parenthesised() {
    let source = "let x: ((a: number) => number) | undefined;";
    assert_eq!(type_of_annotation(source), "((a: number) => number) | undefined");
}

// **The `typeof` case has no test here, and that is stated rather than papered
// over.** A `TypeQueryNode` must not be parenthesised as a union constituent:
// upstream gives it `TypePrecedenceTypeOperator`, *equal* to the constituent
// precedence, so the `<` test fails — and its own comment says the precedence
// exists so that `typeof C` parenthesises in **postfix** position, `(typeof C)[]`.
// The baselines agree: `typeof A | undefined` 6 times, `typeof Baz | undefined`
// 4, `typeof x | undefined` 1, and `(typeof A) | undefined` never.
//
// This paragraph used to say the fixture `class A {} let x: typeof A |
// undefined;` answers `error` because type queries were unported, so a test
// here would be decorative. The `bd tsr-4sc.10` arm landed and made it
// reachable, so the test it deferred now exists below. The baseline is
// `compiler/optionalChainWithInstantiationExpression1.types`:
//   declare const a: typeof A | undefined;   >a : typeof A | undefined
// and `(typeof A) | undefined` appears in no baseline.

/// A `typeof C` constituent is never parenthesised: upstream gives
/// `TypeQueryNode` `TypePrecedenceTypeOperator`, equal to the constituent
/// precedence, so the `<` test fails. Deferred to `bd tsr-4sc.10` by the
/// comment above, and delivered with it.
#[test]
fn a_type_query_is_not_parenthesised_as_a_union_constituent() {
    assert_eq!(
        type_of_annotation("class A {}\nlet x: typeof A | undefined;"),
        "typeof A | undefined"
    );
}

/// `boolean` is the union `false | true` and carries `TypeFlags::UNION`, so
/// every structural test for "is this a union" says yes. It prints as a
/// **keyword**, which the builder emits as `KindBooleanKeyword` at the highest
/// precedence.
///
/// Baseline: `boolean[]` appears **98** times and `(boolean)[]` never. This is
/// the one case here pinned by a test rather than by the corpus delta — the
/// array-element parenthesiser wrapped on `TypeFlags::UNION` alone, and this
/// asserts the fix rather than waiting for a case to exercise it. It needs
/// [`LIB`], which is the whole reason this file declares one.
#[test]
fn boolean_is_not_parenthesised_as_an_array_element() {
    assert_eq!(type_of_annotation("let x: boolean[];"), "boolean[]");
}

/// The same aliased-name rule in the *array element* position, which had the
/// identical defect and a doc comment admitting it was unverified:
/// *"intersections are wrapped on the same precedence grounds and no baseline
/// exercises one."*
///
/// Baseline: `compiler/inferTypePredicates` records `Bar[]`, and this port
/// printed `(Bar)[]`.
#[test]
fn an_aliased_intersection_is_not_parenthesised_as_an_array_element() {
    let source = "interface A {} interface B {} type Bar = A & B; let x: Bar[];";
    assert_eq!(type_of_annotation(source), "Bar[]");
}

/// An anonymous union *is* wrapped in the element position, which is the clause
/// the fix had to leave alone. Baseline: `(string | number)[]`.
#[test]
fn an_anonymous_union_is_still_parenthesised_as_an_array_element() {
    assert_eq!(type_of_annotation("let x: (string | number)[];"), "(string | number)[]");
}

// ---------------------------------------------------------------------------
// Constituent order: `compareTypeNames` for type references. `bd tsr-bgz`.
// ---------------------------------------------------------------------------

/// Two references to the **same** target compare by their type-argument lists,
/// not by their printed text. `getTypeNameSymbol` (`utilities.go:607`) answers
/// the *target* symbol for a reference, so `number[]` and `string[]` both say
/// `Array` and `compareTypeNames` returns 0; upstream then reaches
/// `compareTypeLists`, and `CompareTypes(number, string)` is `1 << 6` against
/// `1 << 5`.
///
/// Baseline: `string[] | number[]`, 25 lines. Comparing the printed text
/// answers `"number[]" < "string[]"` and reverses them.
#[test]
fn references_to_one_target_are_ordered_by_their_type_arguments() {
    assert_eq!(type_of_annotation("let x: number[] | string[];"), "string[] | number[]");
}

/// The same, through a user-declared generic rather than `Array`, so the rule is
/// not accidentally about arrays. Baseline: `Set<string> | Set<number>`, 20
/// lines.
#[test]
fn a_user_generic_orders_the_same_way() {
    let source = "interface Box<T> {} let x: Box<number> | Box<string>;";
    assert_eq!(type_of_annotation(source), "Box<string> | Box<number>");
}

/// References to **different** targets compare by the target symbol's *name*.
///
/// This is the fixture that pinned the change's one lost line, and it is worth
/// keeping because the baseline it comes from prints the same type two ways.
/// `conformance/unionAndIntersectionInference3` writes
/// `(Maybe<T> | Maybe<T>[])[]` in source and records
/// `>args : (Maybe<T>[] | Maybe<T>)[]` — reordered, because `Array` sorts
/// before `Maybe`. It also records the enclosing *signature* in source order,
/// which is `bd tsr-a2c` and a different mechanism entirely.
#[test]
fn references_to_different_targets_are_ordered_by_the_target_name() {
    let source = "interface Maybe<T> {} let x: Maybe<number> | Maybe<number>[];";
    assert_eq!(type_of_annotation(source), "Maybe<number>[] | Maybe<number>");
}
