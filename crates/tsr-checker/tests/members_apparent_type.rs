//! `getApparentType` for a primitive receiver: `"a".length` finds `String`.
//!
//! Separate from `tests/members.rs` because the question is different. That file
//! asks *which symbol* a name resolves to **on a type that already has
//! members**; this one asks whether a receiver with no members of its own is
//! redirected to the global interface that has them, which is a step earlier and
//! is the only thing `Checker::apparent_type` does.
//!
//! # Why these fixtures declare `interface String` themselves
//!
//! A unit test has no lib files: `tsr_binder::bind` is handed one source and
//! `globals()` holds only what that source puts there. But `merge_globals`
//! (`crates/tsr-binder/src/binder.rs:552`) folds a **script** file's top-level
//! locals into `globals`, and a source with no import or export is a script — so
//! a fixture that declares `interface String` at the top level puts it exactly
//! where `lib.es5.d.ts` would. That makes the redirect observable here without a
//! program, and it is the same table the corpus run reads.
//!
//! Both tests were proven red under a named mutation; the mutations are recorded
//! in `docs/architecture/checker-notes-gaproot.md` Part 2.

use tsr_checker::Checker;
use tsr_core::Arena;

/// The printed type of the initialiser of the top-level `const name`.
///
/// Through the initialiser rather than through `get_property_of_type`, because
/// the redirect this file is about happens in
/// `check_property_access_expression` and **not** in `get_property_of_type` —
/// upstream calls `getApparentType` at the access site
/// (`checker.go:11265`), and putting it inside the lookup instead would change
/// what the relater sees. A test that went through the lookup would pass with
/// the code in the wrong place.
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

/// The redirect itself, on the two receiver shapes that reach the same arm: the
/// `string` primitive and a string **literal** type.
///
/// Both are `TypeFlags::STRING_LIKE`, and the literal is the one that says the
/// arm tests the family rather than the intrinsic — upstream's `StringLike`, not
/// an identity test against `stringType`.
///
/// Red under **MA1** (`apparent_type` returns `id` unchanged): both read `any`,
/// because the lookup misses on a primitive and
/// `check_property_access_expression` answers `errorType`, which prints `any`.
#[test]
fn a_primitive_receiver_is_looked_up_in_its_global_interface() {
    let source = "\
interface String { length: number; charAt(pos: number): string; }
const a = \"abc\".length;
const b = \"abc\".charAt;
";
    assert_eq!(type_of(source, "a"), "number", "a string literal's apparent type is `String`");
    assert_eq!(
        type_of(source, "b"),
        "(pos: number) => string",
        "and its methods come through as signatures, which is 42.2% of the corpus slice"
    );
}

/// **The arm must not answer `any` when the global is absent.**
///
/// This is the direction that matters: with no `interface String` in scope the
/// receiver has no members anywhere, and the honest answer is the gap. A version
/// that fell back to `anyType` or to an empty object type would convert every
/// primitive member access in a lib-less configuration into a confident wrong
/// answer, and would look like a large win in the gradient.
///
/// The expected string is **`error`**, which is `errorType`'s printed form here
/// and is literally the marker `examples/gaproot.rs` counts as a gap. The first
/// draft of this test asserted `any` and went red for the right reason: `any`
/// and `error` are different answers and this project's whole gap/wrong split
/// rests on them staying different.
///
/// Red under **MA2** (`apparent_type` returns `self.intrinsics.any` instead of
/// `id` when the global is missing): the first assertion reads `any` instead of
/// `error` — a confident wrong answer where there was an honest gap.
#[test]
fn a_missing_global_is_a_gap_and_not_an_answer() {
    // No `interface String` anywhere: `globals()` cannot hold it.
    assert_eq!(
        type_of("const a = \"abc\".length;", "a"),
        "error",
        "with no global `String`, the member is not found and the line gaps"
    );
    // The mirror, and it is what makes the assertion above mean something: the
    // *same* source with the global present answers a real type. Without this
    // pair, `any` above is consistent both with the arm working and with the
    // arm never firing at all.
    assert_eq!(
        type_of("interface String { length: number; }\nconst a = \"abc\".length;", "a"),
        "number",
        "the mirror: the only difference is whether the global is in scope"
    );
}
