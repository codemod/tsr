//! `{ ...a }` — object spread, and the two properties that are not obvious.
//!
//! Ported behaviour is `Checker.getSpreadType` (`checker.go:13387`) reduced to
//! an object-typed source; `crate::spreads` documents what is gapped and why
//! (the former `spread_members_of` enumerator was removed when object rest
//! moved onto the semantic spread properties, `checker-99-rest-index-infos.md`). These tests pin the two things a naive implementation gets
//! wrong, and both were chosen because a fixture that merely *works* passes
//! either way.
//!
//! Historical mutation ledger (before final declaration-provenance sorting):
//!
//! | # | mutation | reddens |
//! |---|---|---|
//! | 1 | `upsert_member` becomes `members.push` | [`a_later_member_uses_its_own_declaration_order`] |
//! | 2 | `spread_members_of` iterates the symbol table instead of sorting by declaration position | [`spread_member_order_is_the_declaration_order`] — non-deterministically |
//! | 3 | `spread_members_of` returns `Some(vec![])` instead of `None` for a non-object source | [`a_spread_of_something_this_port_cannot_compute_gaps`] |

use tsr_checker::Checker;
use tsr_core::Arena;

/// The printed type of the top-level `const name`.
fn type_of(source: &str, name: &str) -> String {
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
    for index in 0..parsed.nodes.len() {
        #[allow(clippy::cast_possible_truncation)]
        let id = tsr_ast::NodeId::new(index as u32);
        if let Some(symbol) = bound.lookup_local(id, name) {
            let ty = checker.get_type_of_symbol(symbol);
            return checker.type_to_string(ty);
        }
    }
    panic!("`{name}` is declared nowhere");
}

/// The last required declaration supplies both the type and ordering origin.
/// Pinned tsgo 5b1047d10 emits `b, a` for the first case and `a, b` for the
/// second. The original test incorrectly asserted replace-in-place ordering;
/// see docs/architecture/checker-99-spread-origins.md.
///
/// Red under **mutation 1**, and under disabling final declaration ordering.
#[test]
fn a_later_member_uses_its_own_declaration_order() {
    assert_eq!(
        type_of("const o = { a: 1, b: 2 };\nconst s = { ...o, a: \"x\" };", "s"),
        "{ b: number; a: string; }",
        "the later `a` declaration follows the source's `b`"
    );
    // In the mirror the spread's original declaration wins, so `a` precedes
    // `b`. Always appending overwritten members cannot satisfy both cases.
    assert_eq!(
        type_of("const o = { a: 1, b: 2 };\nconst s = { a: \"x\", ...o };", "s"),
        "{ a: number; b: number; }",
        "the spread's `a` wins, and `a` is still the first member"
    );
}

/// `tryMergeUnionOfObjectTypeAndEmptyObject` treats `T extends undefined` as
/// the one nonempty alternative of `object | T`, then resolves that
/// alternative's properties through its constraint. Native therefore folds
/// `{ ...a }` to `{}`, not `T | {}` (`spreadObjectOrFalsy`). A primitive
/// constraint that can be truthy is still an invalid spread and must not be
/// admitted by this path.
#[test]
fn a_falsy_constrained_generic_union_spreads_to_empty_object() {
    assert_eq!(
        type_of("function f<T extends undefined>(a: object | T) { return { ...a }; }", "f"),
        "<T extends undefined>(a: object | T) => {}"
    );
    assert_eq!(
        type_of("function f<T extends string>(a: object | T) { return { ...a }; }", "f"),
        "<T extends string>(a: object | T) => any",
        "a truthy primitive constraint is not a valid spread source"
    );
}

/// The empty paths have two different native exits. A primitive-only union is
/// rejected by `isValidSpreadType`; with a real empty object among the
/// primitives, `tryMergeUnionOfObjectTypeAndEmptyObject` returns that actual
/// empty constituent. These controls keep the broader "spreads into empty"
/// classification from being mistaken for actual `isEmptyObjectType` and
/// returning the last nullish/primitive constituent.
#[test]
fn empty_only_unions_preserve_the_actual_empty_object_distinction() {
    assert_eq!(
        type_of("function f(a: null | undefined) { return { ...a }; }", "f"),
        "(a: null | undefined) => any",
        "primitive-only unions are invalid spread sources"
    );
    assert_eq!(
        type_of("function f(a: {} | null | undefined) { return { ...a }; }", "f"),
        "(a: {} | null | undefined) => {}",
        "a real empty-object constituent wins over nullish constituents"
    );
}

/// **Member order is the source declaration order, not the table's.**
///
/// `SymbolTable` is an `FxHashMap`, so iterating it yields **hash** order, and
/// `FxHash` is unseeded — so the wrong order is *deterministically* wrong for
/// some key sets and *deterministically right* for others.
///
/// **The first version of this test used `{ z, m, a }` and mutation 2 passed**:
/// those three names happen to hash into declaration order, so the fixture
/// could not see the bug. The names below were found by running the mutation
/// over candidate fixtures and printing what each produced. They distinguish
/// all three candidate orderings at once:
///
/// | ordering | result |
/// |---|---|
/// | declaration (correct) | `alpha, beta, gamma, delta, epsilon` |
/// | alphabetical | `alpha, beta, delta, epsilon, gamma` — `delta` before `gamma` |
/// | `FxHashMap` iteration | `epsilon, delta, alpha, gamma, beta` |
///
/// The original single-sort implementation was red under mutation 2. Final
/// provenance sorting now supplies a second ordering boundary; its isolated
/// mutation is covered by tsr-conformance/tests/spread_origins.rs. This test
/// continues to pin declaration order against a globally alphabetical result.
#[test]
fn spread_member_order_is_the_declaration_order() {
    assert_eq!(
        type_of(
            "const o = { alpha: 1, beta: 2, gamma: 3, delta: 4, epsilon: 5 };\n\
             const s = { ...o };",
            "s"
        ),
        "{ alpha: number; beta: number; gamma: number; delta: number; epsilon: number; }",
        "declaration order: not alphabetical (delta/gamma) and not hash order"
    );
}

/// **A source this port cannot compute gaps; it does not become `{}` or `any`.**
///
/// `errorType`, never `anyType`, and never a silently-empty object either: an
/// empty spread would print `{ x: number; }` for `{ ...unknownThing, x: 1 }`,
/// a confident wrong type where the honest answer is that the spread was not
/// computed.
///
/// `error` is `errorType`'s printed form here and is the marker the corpus
/// reads as a gap.
///
/// Red under **mutation 3**.
#[test]
fn a_spread_of_something_this_port_cannot_compute_gaps() {
    // A primitive source: upstream drops it, this port declines to guess.
    assert_eq!(
        type_of("const n = 1;\nconst s = { ...n, x: 1 };", "s"),
        "error",
        "a non-object spread source gaps the whole literal"
    );
    // The mirror: the identical literal with an object source computes. Without
    // it, the assertion above is consistent with object spread never working.
    assert_eq!(
        type_of("const o = { y: 2 };\nconst s = { ...o, x: 1 };", "s"),
        "{ y: number; x: number; }",
        "the mirror: only the source differs"
    );
}
