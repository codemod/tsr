//! **Design P** — a name declared inside a namespace prints with the qualifier
//! it needs to be read correctly from the reference site.
//!
//! `Checker::type_to_string_at` puts every printed name through the port of
//! `getSymbolChain` (`internal/checker/nodebuilderimpl.go:1087`), which climbs
//! to the symbol's containers for as long as `needsQualification`
//! (`internal/checker/symbolaccessibility.go:688`) says the bare name would not
//! resolve back to this symbol here. Sized before it was built in
//! `docs/architecture/checker-notes-qualname.md` §10 at **2,990 conversions
//! against 14 lines at risk**.
//!
//! # Every fixture names the `.types` baseline it was derived from
//!
//! Five expectations written from intuition were wrong in an earlier session and
//! the port was right every time. The load-bearing fixtures here are the
//! **negatives**, because this mechanism's whole risk is printing a qualifier
//! where upstream prints none — the failure that cost cycle 20b 3,202 lines:
//!
//! - `conformance/ClassAndModuleWithSameNameAndCommonRoot.types:11` records
//!   `>Point : Point` — **bare** — for `export class Point` written inside
//!   `namespace X.Y`. The site is inside the container, so `needsQualification`
//!   answers *no qualifier needed* and the chain stops before it starts.
//! - `conformance/ClassAndModuleWithSameNameAndCommonRoot.types:62` records
//!   `>Point : typeof Point` for `export namespace Point` in a **second**
//!   `namespace X.Y` block, where a class and a namespace of the same name
//!   merge. **Read that test's own comment before trusting it**: it was written
//!   to pin `getMergedSymbol` (`internal/checker/symbolaccessibility.go:696`,
//!   read before the identity test at `:702`), it was checked by deleting the
//!   merge, and it does **not** — the binder resolves a single-file merge as it
//!   binds. The clause is pinned by a corpus measurement instead, and the number
//!   is recorded there.
//!
//! and the positives are the shape the corpus actually converted:
//!
//! - `compiler/vararg.types:145` records `>x : M.C` for `var x=new M.C();`,
//!   where `class C` is declared inside `namespace M`. The instance type's baked
//!   text is the bare `C`; read from outside `M` it must print `M.C`.
//! - `conformance/instantiatedModule.types:131` records
//!   `>Point : typeof M2.Point` — the same rule on the static side, which is
//!   705 of the 2,990 sized conversions (§10.3).

use tsr_ast::{Node, NodeId, NodeMap, push_children};
use tsr_binder::SymbolFlags;
use tsr_checker::Checker;
use tsr_core::Arena;

/// Every identifier in the file whose text is `text`, in source order.
fn identifiers(map: &NodeMap<'_>, root: NodeId, text: &str, out: &mut Vec<NodeId>) {
    let Some(node) = map.get(root) else { return };
    if let Node::Identifier(identifier) = node
        && identifier.text == text
    {
        out.push(root);
    }
    let mut children = Vec::new();
    push_children(node, &mut children);
    for child in children {
        if let Some(id) = child.node_id() {
            identifiers(map, id, text, out);
        }
    }
}

/// Bind `source`, then print the type of `namespace_path`'s export `member`
/// **as seen from** the `site_index`-th occurrence of the identifier
/// `site_name`.
///
/// The two halves are deliberately separate: the type comes from the symbol, so
/// its baked text is the bare declared name, and the only thing that can put a
/// qualifier on it is the reference site.
fn declared_type_at(
    source: &str,
    namespace_path: &[&str],
    member: &str,
    site_name: &str,
    site_index: usize,
    of_value: bool,
) -> String {
    let arena = Arena::new();
    let parsed = tsr_parser::parse(&arena, source);
    assert!(parsed.diagnostics.is_empty(), "fixture must parse");
    let bound = tsr_binder::bind(
        &arena,
        parsed.source_file,
        &parsed.nodes,
        tsr_binder::FileInfo { name: "t.ts", text: source },
    );

    // Walk down the namespace path through `exports`, which is where the binder
    // files a namespace's exported members.
    let mut symbol = *bound.globals().get(namespace_path[0]).expect("root namespace");
    symbol = bound.merged_symbol(symbol);
    for step in &namespace_path[1..] {
        symbol = *bound.symbols().get(symbol).exports.get(*step).expect("namespace step");
        symbol = bound.merged_symbol(symbol);
    }
    let member = bound.merged_symbol(
        *bound.symbols().get(symbol).exports.get(member).expect("the namespace exports it"),
    );

    let mut sites = Vec::new();
    identifiers(&parsed.node_map, parsed.source_file.node_id.unwrap(), site_name, &mut sites);
    let site = *sites.get(site_index).expect("the site identifier occurs that many times");

    let mut checker = Checker::new(&bound, &parsed.nodes, &parsed.node_map);
    let id = if of_value {
        checker.get_type_of_symbol(member)
    } else {
        checker.get_declared_type_of_symbol(member)
    };
    checker.type_to_string_at(id, site).expect("nameable")
}

/// `compiler/vararg.types:145` — `>x : M.C`.
///
/// `class C` lives in `namespace M`; the reference site is the `x` of
/// `var x = new M.C()`, outside `M`. `resolve_name("C")` finds nothing there, so
/// `needsQualification` is true and `getSymbolChain` climbs to `M`.
#[test]
fn a_class_in_a_namespace_prints_qualified_from_outside() {
    let source = "namespace M { export class C { p: number; } }\nvar x = 1;\n";
    assert_eq!(declared_type_at(source, &["M"], "C", "x", 0, false), "M.C");
}

/// `conformance/instantiatedModule.types:131` — `>Point : typeof M2.Point`.
///
/// The static side takes the qualifier the same way, and
/// `Checker::split_around_name` has to look past the `typeof ` prefix to find
/// the name. 705 of the 2,990 sized conversions are this position (§10.3).
#[test]
fn the_static_side_takes_the_qualifier_under_typeof() {
    let source = "namespace M2 { export class Point { x: number; } }\nvar a2 = 1;\n";
    assert_eq!(declared_type_at(source, &["M2"], "Point", "a2", 0, true), "typeof M2.Point");
}

/// A nested namespace produces a **multi-segment** chain, because
/// `getSymbolChain` recurses on the parent with `getQualifiedLeftMeaning`
/// (`internal/checker/nodebuilderimpl.go:1111`) rather than stopping at one
/// hop. `compiler/vararg`'s sibling shape; §10.3 measures 36 conversions at 2
/// segments and 7 at 3.
#[test]
fn a_nested_namespace_produces_a_multi_segment_chain() {
    let source =
        "namespace A { export namespace B { export class C { p: number; } } }\nvar x = 1;\n";
    assert_eq!(declared_type_at(source, &["A", "B"], "C", "x", 0, false), "A.B.C");
}

/// `conformance/ClassAndModuleWithSameNameAndCommonRoot.types:11` — `>Point : Point`.
///
/// **The stop condition.** The site is the `x` inside `class Point`'s own body,
/// which is inside `namespace X`, so `Point` resolves bare there and
/// `needsQualification` (`internal/checker/nodebuilderimpl.go:1094`) ends the
/// chain before a container is ever consulted. Without this test the mechanism
/// is the ungated one that lost 3,202 lines.
#[test]
fn a_name_in_scope_at_the_site_is_not_qualified() {
    let source = "namespace X { export class Point { inner: number; } }\n";
    assert_eq!(declared_type_at(source, &["X"], "Point", "inner", 0, false), "Point");
}

/// `conformance/ClassAndModuleWithSameNameAndCommonRoot.types:62` — `>Point : typeof Point`.
///
/// A class and a namespace of the same name, in two `namespace X.Y` blocks, and
/// the name still prints bare from a site inside them.
///
/// # What this test does NOT pin, stated rather than implied
///
/// It does **not** exercise `getMergedSymbol`
/// (`internal/checker/symbolaccessibility.go:696`, read before the identity test
/// at `:702`), and it was written believing it did. Checked the only way that
/// settles it — by deleting the merge from
/// `Checker::needs_qualification` and re-running — this test **still passes**.
/// The binder resolves a *single-file* merge into one symbol as it binds, so
/// `resolve_name` already returns the same id and there is nothing for the merge
/// to do. Every one of the 13 lines the merge protects is a **cross-file**
/// namespace merge (`compiler/emitMemberAccessExpression` declares
/// `namespace Microsoft.PeopleAtWork.Model` in two files), which no single-file
/// fixture in this crate can reach.
///
/// The clause is pinned by **measurement on the corpus** instead, and the number
/// is on record rather than asserted: deleting the merge and re-running
/// `examples/verdictdump.rs` over the whole corpus takes the build from
/// **17 lines lost to 30** — exactly the 13 that `checker-notes-qualname.md`
/// §10.4 predicted — while converting the identical **2,990**. The clause costs
/// zero conversions and removes 13 losses.
///
/// **This comes due**: a multi-file fixture through `Checker::with_module_host`
/// would make it a unit test, and until one exists the protection lives in a
/// paragraph and a corpus run.
#[test]
fn a_merged_declaration_prints_bare_from_inside() {
    let source = "namespace X.Y {\n  export class Point { p: number; }\n}\nnamespace X.Y {\n  export namespace Point { export var origin = 1; }\n  var here = 1;\n}\n";
    assert_eq!(declared_type_at(source, &["X", "Y"], "Point", "here", 0, true), "typeof Point");
}

/// A namespace **local** — not exported — has no container to climb to.
///
/// `getParentOfSymbol` (`internal/checker/checker.go:14365`) reads
/// `Symbol.Parent`, which the binder sets only for a symbol that lives in
/// another symbol's table; a local lives in the module's `locals`. Upstream's
/// fallback loop (`internal/checker/symbolaccessibility.go:288`) recovers
/// containers only for external-module children, so upstream does not qualify
/// one either. §10.1 measures 229 such lines, left bare.
#[test]
fn a_namespace_local_has_no_container_and_stays_bare() {
    let source =
        "namespace M { class Hidden { p: number; } export var seen: Hidden; }\nvar x = 1;\n";
    let arena = Arena::new();
    let parsed = tsr_parser::parse(&arena, source);
    assert!(parsed.diagnostics.is_empty(), "fixture must parse");
    let bound = tsr_binder::bind(
        &arena,
        parsed.source_file,
        &parsed.nodes,
        tsr_binder::FileInfo { name: "t.ts", text: source },
    );
    let namespace = bound.merged_symbol(*bound.globals().get("M").expect("namespace M"));
    // `Hidden` is a LOCAL of `M`, so it is absent from `exports` — which is the
    // structural fact this test is about.
    assert!(
        !bound.symbols().get(namespace).exports.contains_key("Hidden"),
        "the fixture's premise: a non-exported class is not an export"
    );
    // Reached the way any reference inside `M` reaches it — through scope,
    // not through the export table.
    let mut inside = Vec::new();
    identifiers(&parsed.node_map, parsed.source_file.node_id.unwrap(), "seen", &mut inside);
    let hidden = bound
        .resolve_name(&parsed.nodes, &parsed.node_map, inside[0], "Hidden", SymbolFlags::TYPE)
        .expect("a local resolves from inside its namespace");
    let mut sites = Vec::new();
    identifiers(&parsed.node_map, parsed.source_file.node_id.unwrap(), "x", &mut sites);
    let mut checker = Checker::new(&bound, &parsed.nodes, &parsed.node_map);
    let id = checker.get_declared_type_of_symbol(bound.merged_symbol(hidden));
    assert_eq!(checker.type_to_string_at(id, sites[0]).expect("nameable"), "Hidden");
}

/// A type with no symbol at all — `number` — is untouched.
///
/// `Checker::split_around_name` is the gate, and it can only fire on a printed
/// form that *is* a symbol's name. `checker-notes-nameres.md` §49 records what
/// removing the gate does: it climbs from an anonymous `__function` symbol to no
/// parent and gaps every function and object type in the corpus.
#[test]
fn an_intrinsic_is_not_a_name_the_chain_can_touch() {
    let source = "namespace M { export class C { p: number; } }\nvar x = 1;\n";
    let arena = Arena::new();
    let parsed = tsr_parser::parse(&arena, source);
    let bound = tsr_binder::bind(
        &arena,
        parsed.source_file,
        &parsed.nodes,
        tsr_binder::FileInfo { name: "t.ts", text: source },
    );
    let mut sites = Vec::new();
    identifiers(&parsed.node_map, parsed.source_file.node_id.unwrap(), "x", &mut sites);
    let mut checker = Checker::new(&bound, &parsed.nodes, &parsed.node_map);
    let number = checker.intrinsics().number;
    assert_eq!(checker.type_to_string_at(number, sites[0]).expect("nameable"), "number");
}

/// The chain never reports a meaning it did not ask for: the recursion asks its
/// parent with `SymbolFlagsNamespace` (`internal/checker/nodebuilderimpl.go:1111`),
/// so a *value* of the same name as the container cannot end the chain early.
#[test]
fn the_parent_is_looked_up_as_a_namespace() {
    let source = "namespace A { export namespace B { export class C { p: number; } } }\nvar B = 1;\nvar x = 1;\n";
    // A value named `B` is in scope at the site. The chain must still be
    // `A.B.C`, because the parent hop resolves `B` with NAMESPACE meaning.
    assert_eq!(declared_type_at(source, &["A", "B"], "C", "x", 0, false), "A.B.C");
    let arena = Arena::new();
    let parsed = tsr_parser::parse(&arena, source);
    let bound = tsr_binder::bind(
        &arena,
        parsed.source_file,
        &parsed.nodes,
        tsr_binder::FileInfo { name: "t.ts", text: source },
    );
    assert!(
        bound.globals().get("B").is_some_and(|&s| bound
            .symbols()
            .get(s)
            .flags
            .intersects(SymbolFlags::VARIABLE)),
        "the fixture's premise: a VALUE named B shadows nothing in namespace position"
    );
}

// ---------------------------------------------------------------------------
// The split: `Checker::is_shadowed_at` versus `Checker::needs_qualification`
// ---------------------------------------------------------------------------
//
// `checker-notes-sitename.md` §8. The predicate wired into `symbol_chain` is
// NOT upstream's `needsQualification`: it is that predicate OR'd with *no
// accessible chain exists*, which upstream keeps as a separate disjunct at
// `internal/checker/nodebuilderimpl.go:1093-1094`. `is_shadowed_at` is the
// honest transcription of the second disjunct alone
// (`internal/checker/symbolaccessibility.go:688-726`, the table walk).
//
// These three tests are the only place the difference is visible, because the
// conflated predicate is still the one every printing path calls — by design,
// until each call site is re-pointed with its own measurement. Delete them and
// the next session re-derives §8 from the −6,850 the hard way.

/// Bind `source` and ask [`Checker::is_shadowed_at`] about `symbol_path` —
/// resolved from globals through `exports` — at the `site_index`-th occurrence
/// of the identifier `site_name`.
fn shadowed_at(
    source: &str,
    symbol_path: &[&str],
    site_name: &str,
    site_index: usize,
    meaning: SymbolFlags,
) -> bool {
    let arena = Arena::new();
    let parsed = tsr_parser::parse(&arena, source);
    assert!(parsed.diagnostics.is_empty(), "fixture must parse");
    let bound = tsr_binder::bind(
        &arena,
        parsed.source_file,
        &parsed.nodes,
        tsr_binder::FileInfo { name: "t.ts", text: source },
    );
    let mut symbol = bound.merged_symbol(*bound.globals().get(symbol_path[0]).expect("root name"));
    for step in &symbol_path[1..] {
        symbol = bound
            .merged_symbol(*bound.symbols().get(symbol).exports.get(*step).expect("export step"));
    }
    let mut sites = Vec::new();
    identifiers(&parsed.node_map, parsed.source_file.node_id.unwrap(), site_name, &mut sites);
    let site = *sites.get(site_index).expect("the site identifier occurs that many times");
    let name = bound.symbols().get(symbol).name;
    let mut checker = Checker::new(&bound, &parsed.nodes, &parsed.node_map);
    checker.is_shadowed_at(symbol, name, site, meaning)
}

/// A name that resolves to the symbol itself is **not** shadowed —
/// `symbolaccessibility.go:702`, the identity stop.
///
/// Both predicates agree here, which is what makes the next two tests
/// meaningful: the split is not a wholesale disagreement.
#[test]
fn a_name_that_resolves_to_the_symbol_is_not_shadowed() {
    let source = "class C { p: number; }\nvar x = 1;\n";
    assert!(!shadowed_at(source, &["C"], "x", 0, SymbolFlags::TYPE));
}

/// **The divergence, and the whole reason for the split.**
///
/// `M.C` is in no symbol table in scope at the site, so upstream's
/// `needsQualification` never fires its callback and answers **false** — and
/// `is_shadowed_at` answers false with it. `needs_qualification` answers
/// **true**, because `resolve_name` returns `None` and its `None => true` arm
/// is standing in for upstream's *other* disjunct, `chain == nil`.
///
/// The qualifier this site prints (`M.C`, pinned by
/// `a_class_in_a_namespace_prints_qualified_from_outside` above) is therefore
/// owed to the CHAIN disjunct, not to shadowing. Re-pointing `symbol_chain` at
/// this function without also porting `getAccessibleSymbolChain` measures
/// **6,850 R→W** (§8.2) — `compiler/temporal` 3,318 of them — and this test is
/// the one-line version of that measurement.
#[test]
fn an_unreachable_name_is_not_shadowed_though_it_still_needs_a_qualifier() {
    let source = "namespace M { export class C { p: number; } }\nvar x = 1;\n";
    assert!(!shadowed_at(source, &["M", "C"], "x", 0, SymbolFlags::TYPE));
}

/// Real shadowing: a **different** symbol of the same name, carrying the
/// meaning being asked about, is in scope at the site
/// (`symbolaccessibility.go:721`). Here both predicates answer true, and only
/// here is the conflated one answering for the right reason.
#[test]
fn a_different_symbol_of_the_same_name_in_scope_is_shadowing() {
    let source =
        "namespace M { export class C { p: number; } }\nclass C { q: string; }\nvar x = 1;\n";
    assert!(shadowed_at(source, &["M", "C"], "x", 0, SymbolFlags::TYPE));
}

/// The meaning filter is upstream's (`symbolaccessibility.go:720`): a **value**
/// of the same name does not shadow a **type** question. Without the filter
/// this is the mechanism that would put qualifiers on names that read fine.
#[test]
fn a_value_of_the_same_name_does_not_shadow_a_type_question() {
    let source = "namespace M { export class C { p: number; } }\nvar C = 1;\nvar x = 1;\n";
    assert!(!shadowed_at(source, &["M", "C"], "x", 0, SymbolFlags::TYPE));
}

/// `getAccessibleSymbolChain` searches a namespace declaration's **exports**
/// after its locals. An exported import-equals alias lives only in that table,
/// and it can name an enclosing segment of the target chain.
///
/// This is the reduced `privacyImport` shape: the leaf is still `C`, but its
/// container must print under the alias visible at the reference site.
#[test]
fn an_exported_import_equals_alias_names_a_chain_segment() {
    let source = "namespace M { export class C { p: number; } }\nnamespace Use { export import Alias = M; export var x: Alias.C; }\n";
    assert_eq!(declared_type_at(source, &["M"], "C", "x", 0, false), "Alias.C");
}

/// Scope tables remain independent: a nearer exported alias that reaches a
/// different namespace does not stop the walk. The matching alias in the next
/// enclosing namespace supplies the target's chain segment.
#[test]
fn a_non_matching_inner_alias_does_not_hide_an_outer_matching_alias() {
    let source = "namespace M { export class C { p: number; } }\nnamespace N { export class C { q: string; } }\nnamespace Use { export import Wanted = M; export namespace Inner { export import Other = N; export var x: M.C; } }\n";
    assert_eq!(declared_type_at(source, &["M"], "C", "x", 0, false), "Wanted.C");
}
