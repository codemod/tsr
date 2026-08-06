//! `{ ...a }` — object spread, and the two properties that are not obvious.
//!
//! Ported behaviour is `Checker.getSpreadType` (`checker.go:13387`) reduced to
//! an object-typed source; `crate::objects::spread_members_of` documents what is
//! gapped and why. These tests pin the two things a naive implementation gets
//! wrong, and both were chosen because a fixture that merely *works* passes
//! either way.
//!
//! | # | mutation | reddens |
//! |---|---|---|
//! | 1 | `upsert_member` becomes `members.push` | [`a_later_member_replaces_an_earlier_one_in_place`] |
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

/// **Position from the first occurrence, type from the last.**
///
/// This is the assertion a `push` cannot satisfy: pushing prints `a` twice,
/// which is not a type upstream can produce, and appending-after-removal would
/// print `{ b: number; a: string; }` with `a` in the wrong place. Only
/// replace-in-place gives upstream's answer.
///
/// Red under **mutation 1**.
#[test]
fn a_later_member_replaces_an_earlier_one_in_place() {
    assert_eq!(
        type_of("const o = { a: 1, b: 2 };\nconst s = { ...o, a: \"x\" };", "s"),
        "{ a: string; b: number; }",
        "`a` keeps the spread's position and takes the later type"
    );
    // The mirror, and it is what makes the first assertion about *ordering*
    // rather than about spreading: with the override written first, `a` is
    // still first, so a passing implementation cannot be one that simply
    // appends overrides.
    assert_eq!(
        type_of("const o = { a: 1, b: 2 };\nconst s = { a: \"x\", ...o };", "s"),
        "{ a: number; b: number; }",
        "the spread's `a` wins, and `a` is still the first member"
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
/// Red under **mutation 2**, and red under a sort-by-name implementation too.
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
