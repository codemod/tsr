//! An interface declared in two files: the binder merges it, and the lookup
//! finds only half of it.
//!
//! # The premise this file corrects
//!
//! `bd tsr-9or.1` was first briefed as *"the globals are never merged across
//! files"*. **That is wrong** — `merge_globals` → `merge_symbol`
//! (`crates/tsr-binder/src/binder.rs:630`) unions the members tables, and the
//! tests below show the merged table holding both.
//!
//! It was then handed to this workstream corrected, as *"the lookup that
//! answers `false` is downstream of both — in `get_property_of_type`, in
//! `members.rs`"*. **That is also wrong, and this file is the reproduction that
//! shows why.**
//!
//! The lookup is handed a **different symbol**. `merge_symbol` unions `source`
//! into `target` and records **no link back**: `grep` for `merge_id`,
//! `merged_symbol` or `mergeId` across the binder and the checker returns
//! nothing. So a reference *inside the source's file* resolves through
//! `resolve_name` → `lookup_local`, reaches the **source** symbol, and that
//! symbol's members table legitimately holds only its own file's members.
//! `get_property_of_type` then answers correctly for the symbol it was given.
//!
//! Upstream's mechanism is `getMergedSymbol` (`checker.go:14355`), backed by a
//! `mergedSymbols` source→target map, applied at **every** symbol read. This
//! port has neither the map nor the redirect. **The information the fix needs
//! does not exist outside the binder**, so there is no version of this fix that
//! can be written in `members.rs` at all — not a partial one, not a local one.
//!
//! Reproduced first and diagnosed second, because a finding handed on from
//! another agent is a claim until this crate's own harness produces it — and
//! this one changed owner twice on contact with a reproduction.
//!
//! # The harness is `tests/cross_file_aliases.rs`'s, reduced
//!
//! Two files must share one node table and one `SymbolStore`, or a symbol from
//! the second is not safe to hand the checker at all
//! ([ADR-0034](../../../docs/adr/0034-a-program-needs-one-identity-space.md)).
//! No `ModuleHost` is needed here — these are **script** files whose top-level
//! declarations become globals, which is the configuration `lib.*.d.ts` is in
//! and is exactly what the corpus hits.

use tsr_ast::{NodeMap, NodeTable};
use tsr_binder::BindResult;
use tsr_checker::Checker;
use tsr_core::Arena;

/// Everything one fixture program produced. Held together because the checker
/// borrows all three and the borrows must outlive it.
struct Fixture<'a> {
    nodes: NodeTable,
    node_map: NodeMap<'a>,
    bound: BindResult<'a>,
}

/// Parse and bind several **script** files into one identity space.
fn program<'a>(arena: &'a Arena, files: &[(&'static str, &str)]) -> Fixture<'a> {
    let mut nodes = NodeTable::new();
    let mut node_map = NodeMap::new();
    let mut parsed = Vec::new();
    for (name, source) in files {
        let source: &'a str = arena.alloc_str(source);
        let file = tsr_parser::parse_into(
            arena,
            source,
            tsr_parser::ParseOptions::default(),
            &mut nodes,
            &mut node_map,
        );
        assert!(file.diagnostics.is_empty(), "fixture {name} must parse");
        parsed.push((*name, source, file.source_file));
    }
    let mut bound = BindResult::empty();
    for (name, source, source_file) in parsed {
        let file_name: &'a str = arena.alloc_str(&format!("/{name}.ts"));
        bound = tsr_binder::bind_into(
            bound,
            source_file,
            &nodes,
            tsr_binder::FileInfo { name: file_name, text: source },
        );
    }
    Fixture { nodes, node_map, bound }
}

/// `(what the binder holds, what the checker finds)` for one property of a
/// global interface — the two halves the finding says disagree.
///
/// Deliberately returns **both**, from one fixture, rather than asserting on
/// the checker alone. A test that only checked the lookup would be equally
/// consistent with "the binder did not merge", which is the premise this file
/// exists to refute, and the refutation has to be visible in the assertion
/// rather than in a comment.
fn binder_and_checker(fixture: &mut Fixture<'_>, interface: &str, property: &str) -> (bool, bool) {
    let symbol = *fixture.bound.globals().get(interface).expect("the interface is a global");
    let in_binder = fixture.bound.symbols().get(symbol).members.contains_key(property);
    let mut checker = Checker::new(&fixture.bound, &fixture.nodes, &fixture.node_map);
    let declared = checker.get_declared_type_of_symbol(symbol);
    let in_checker = checker.get_property_of_type(declared, property).is_some();
    (in_binder, in_checker)
}

/// The same question asked from a **reference inside a named file**, which is
/// how the corpus reaches the type and is not how [`binder_and_checker`] does.
///
/// `globals()` holds the merge *target*. A reference resolves through
/// `resolve_name`, which tries the enclosing file's locals first — so a
/// reference in the file whose declaration was merged *away* may reach a
/// different symbol than the global, and that symbol's members table is the one
/// the lookup reads.
fn from_reference(
    fixture: &mut Fixture<'_>,
    reference: &str,
    interface: &str,
    property: &str,
) -> bool {
    // The `x` of `var x: I` in the named file: any node inside that file will
    // do as a resolution start point, and this one is unambiguous.
    let mut start = None;
    for index in 0..u32::try_from(fixture.nodes.len()).expect("fits") {
        let id = tsr_ast::NodeId::new(index);
        if fixture.nodes.kind(id) != tsr_ast::SyntaxKind::Identifier {
            continue;
        }
        let Some(tsr_ast::Node::Identifier(name)) = fixture.node_map.get(id) else { continue };
        if name.text == reference {
            start = Some(id);
            break;
        }
    }
    let start = start.expect("the marker identifier is in the fixture");
    let symbol = fixture
        .bound
        .resolve_name(
            &fixture.nodes,
            &fixture.node_map,
            start,
            interface,
            tsr_binder::SymbolFlags::TYPE,
        )
        .expect("the interface resolves from this reference");
    let mut checker = Checker::new(&fixture.bound, &fixture.nodes, &fixture.node_map);
    let declared = checker.get_declared_type_of_symbol(symbol);
    checker.get_property_of_type(declared, property).is_some()
}

/// **The asymmetry, asked the way the corpus asks it.**
///
/// Marker identifiers `inA` and `inB` sit in their respective files purely as
/// resolution start points.
#[test]
fn the_lookup_depends_on_which_file_the_reference_is_in() {
    let arena = Arena::new();
    let mut fixture = program(
        &arena,
        &[
            ("a", "interface I { a: string; }\nvar inA: I;"),
            ("b", "interface I { b: number; }\nvar inB: I;"),
        ],
    );
    let from_a = (
        from_reference(&mut fixture, "inA", "I", "a"),
        from_reference(&mut fixture, "inA", "I", "b"),
    );
    let from_b = (
        from_reference(&mut fixture, "inB", "I", "a"),
        from_reference(&mut fixture, "inB", "I", "b"),
    );
    assert_eq!(
        from_a,
        (true, true),
        "from file a — the merge TARGET's file — both members are reachable"
    );
    // **Inverted, as the comment that stood here required.** It asserted
    // `(false, true)` on purpose, recording *which half of the pipeline* was
    // wrong rather than claiming the answer was right — the finding had been
    // handed on as a `members.rs` lookup bug and it was not one.
    //
    // `BindResult::merged_symbol` — the port of `getMergedSymbol`
    // (`internal/checker/checker.go:14355`) — now redirects every scope-table
    // hit in `resolve_name`, exactly as upstream's `NameResolver.Lookup` hook
    // `c.getSymbol` (`:1474`, `:2176`) does. A reference in the merge source's
    // file reaches the merged target, and both members are visible from both
    // files.
    assert_eq!(
        from_b,
        (true, true),
        "from file b — the merge SOURCE's file — the redirect reaches the merged target"
    );
}

/// **The reproduction.** Two files, one `interface I`, one property each.
///
/// The binder holds both members on one symbol, and since `a05bf94` the checker
/// finds both from either file.
///
/// **This doc comment described the defect until the redirect landed** — it read
/// "the checker finds only the one declared in the file the *first* declaration
/// is in", which was true when the test was written to record it and stopped
/// being true in the commit that inverted the assertions below. A stale comment
/// beside a passing test is how a fixed defect keeps being budgeted for.
#[test]
fn an_interface_declared_in_two_files_loses_the_second_file_s_members() {
    let arena = Arena::new();
    let mut fixture = program(
        &arena,
        &[("a", "interface I { a: string; }"), ("b", "interface I { b: number; }")],
    );

    assert_eq!(
        binder_and_checker(&mut fixture, "I", "a"),
        (true, true),
        "the property from the FIRST file: the binder holds it and the checker finds it"
    );
    assert_eq!(
        binder_and_checker(&mut fixture, "I", "b"),
        (true, true),
        "the property from the SECOND file, reached through `globals()`: the binder holds \
         it and the checker finds it. `globals()` is the merge TARGET, so this path was \
         never broken — which is the first half of the correction below."
    );
}

/// The control, pinned by **construction**: with both properties declared in
/// **one** file there is nothing to merge, so no arrangement of the merge code
/// can affect this and it must have passed before the fix as well.
///
/// Without it, the test above is consistent with the lookup being broken for
/// every interface rather than only for a merged one.
#[test]
fn a_single_file_interface_is_unaffected_and_was_always_right() {
    let arena = Arena::new();
    let mut fixture = program(&arena, &[("a", "interface I { a: string; b: number; }")]);
    assert_eq!(binder_and_checker(&mut fixture, "I", "a"), (true, true));
    assert_eq!(binder_and_checker(&mut fixture, "I", "b"), (true, true));
}
