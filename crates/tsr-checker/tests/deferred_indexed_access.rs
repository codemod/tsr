//! `x[k]` with a generic index is a DEFERRED indexed access, not `any`. §786.
//!
//! Upstream's `getIndexedAccessType` (`checker.go`) does not resolve when
//! `isGenericObjectType(objectType) || isGenericIndexType(indexType)`. It
//! builds an `IndexedAccessType` that prints as written and is resolved only at
//! instantiation. This port resolved eagerly through properties, index
//! signatures and the array/tuple road, and fell to `errorType` — printed
//! `any` — for every generic access.
//!
//! The ANNOTATION half has been ported since §619-§626: `declared.rs`'s
//! `IndexedAccessTypeNode` arm mints `T[K]` through the §31 mint. This is its
//! EXPRESSION twin, minting the same way so the two spellings cannot print
//! differently.
//!
//! **Where the arm sits is the whole story of the first attempt failing.** Put
//! at the tail of `element_access_lookup` it measured **zero movement**,
//! because a non-literal index names no property and the function returns
//! `error` from the `property_name_from_index` branch long before the tail. It
//! belongs in that branch.
//!
//! **And the gate is `keyof` of THIS object, not merely "the index is
//! generic".** The corpus states it sharply
//! (`mappedTypeRelationships.types:107-123`):
//!
//! ```text
//! function f6<T, U extends T, K extends keyof U>(x: T, y: U, k: K) {
//!     x[k] = y[k];
//! }
//! >y[k] : U[K]      <- defers
//! >x[k] : any       <- does NOT defer
//! ```
//!
//! `K` indexes `U`, and `U extends T` makes `keyof T` a SUBSET of `keyof U`,
//! not the reverse — so `x[k]` is upstream's error, printed `any`. Deferring
//! both measured **6 RIGHT→WRONG** in that one case.

use tsr_ast::{FunctionBody, Statement};
use tsr_checker::Checker;
use tsr_core::Arena;

/// Type the initialiser of the first statement in the first function's body.
fn type_of_first_local(source: &str) -> String {
    let arena = Arena::new();
    let parsed = tsr_parser::parse(&arena, source);
    assert!(parsed.diagnostics.is_empty(), "fixture must parse");
    let bound = tsr_binder::bind(
        &arena,
        parsed.source_file,
        &parsed.nodes,
        tsr_binder::FileInfo { name: "t.ts", text: source },
    );
    let mut checker = Checker::new(&bound, &parsed.nodes, &parsed.node_map);
    let Statement::FunctionDeclaration(function) = parsed.source_file.statements[0] else {
        panic!("the fixture must start with a function declaration");
    };
    let FunctionBody::Block(body) = function.body.expect("a body");
    let Statement::VariableStatement(statement) = body.statements[0] else {
        panic!("the body must start with a variable statement");
    };
    let initialiser = statement
        .declaration_list
        .and_then(|list| list.declarations.first().copied())
        .and_then(|declaration| declaration.initializer)
        .expect("an initialiser");
    let id = checker.check_expression(initialiser);
    checker.type_to_string(id)
}

/// The head shape: `K extends keyof T` indexing a `T`.
#[test]
fn a_generic_index_over_its_own_object_defers() {
    assert_eq!(
        type_of_first_local("function f<T, K extends keyof T>(x: T, k: K) { const a = x[k]; }"),
        "T[K]"
    );
}

/// The index may be the `keyof` mint itself rather than a parameter bound to
/// it — `k: keyof T` is the same access.
#[test]
fn a_keyof_typed_index_defers_too() {
    assert_eq!(
        type_of_first_local("function f<T>(x: T, k: keyof T) { const a = x[k]; }"),
        "T[keyof T]"
    );
}

/// The gate, and the shape that cost 6 RIGHT→WRONG without it: `K` indexes
/// `U`, so indexing `T` with it is upstream's error, printed `any` — this port
/// spells that `errorType`.
#[test]
fn a_generic_index_over_a_different_object_does_not_defer() {
    assert_eq!(
        type_of_first_local(
            "function f<T, U extends T, K extends keyof U>(x: T, y: U, k: K) { const a = x[k]; }"
        ),
        "error"
    );
}

/// The other side of the same fixture still defers.
#[test]
fn the_matching_object_in_that_same_fixture_still_defers() {
    assert_eq!(
        type_of_first_local(
            "function f<T, U extends T, K extends keyof U>(x: T, y: U, k: K) { const a = y[k]; }"
        ),
        "U[K]"
    );
}

/// A CONCRETE index is not deferred — it names a property and resolves.
/// Deferring here would print `{ a: string; }["a"]` over a right answer, which
/// is what §625 measured on the annotation road as 27 GAP→WRONG.
#[test]
fn a_concrete_index_still_resolves() {
    assert_eq!(
        type_of_first_local("function f(x: { a: string }) { const a = x[\"a\"]; }"),
        "string"
    );
}

/// `keyof (A & B)` is `keyof A | keyof B`, so a key of one intersection
/// constituent keys the whole intersection. This is the non-null narrowed
/// receiver of `typeVariableTypeGuards.types:284`
/// (`>obj[key] : NonNullable<T>[K]`): without a lib `NonNullable` alias,
/// native's fallback spelling of the receiver is `T & {}`.
#[test]
fn a_key_of_an_intersection_constituent_defers_over_the_intersection() {
    assert_eq!(
        type_of_first_local(
            "function f<T, K extends keyof T>(x: T & {}, k: K) { const a = x[k]; }"
        ),
        "(T & {})[K]"
    );
    // The constituent must be the keyed operand itself: `U` is not a
    // constituent of `T & {}`, so the different-object refusal still holds.
    assert_eq!(
        type_of_first_local(
            "function f<T, U extends T, K extends keyof U>(x: T & {}, k: K) { const a = x[k]; }"
        ),
        "error"
    );
}
