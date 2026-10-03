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

use tsr_ast::{Node, NodeId, NodeMap, NodeTable, SyntaxKind, push_children};
use tsr_binder::SymbolFlags;
use tsr_checker::{Checker, resolution::ModuleHost};
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
    // Enum member spellings are assigned when the parent enum is forced.
    // Keep this naming fixture independent of the cold quoted-member dispatch.
    if bound.symbols().get(member).flags.intersects(SymbolFlags::ENUM_MEMBER) {
        checker.get_declared_type_of_symbol(symbol);
    }
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

/// Pinned native executable (5b1047d1): `instance : Local.C`, but
/// `object : typeof Hidden`. The alias's exports provide the leaf's chain;
/// the direct name Hidden only wins when naming the namespace itself.
#[test]
fn a_private_container_alias_names_the_leaf_not_the_namespace_object() {
    let source = "namespace Outer { namespace Hidden { export class C { p: number; } } import Local = Hidden; export let instance = new Local.C(); export let object = Local; }";
    let arena = Arena::new();
    let parsed = tsr_parser::parse(&arena, source);
    let bound = tsr_binder::bind(
        &arena,
        parsed.source_file,
        &parsed.nodes,
        tsr_binder::FileInfo { name: "t.ts", text: source },
    );
    let mut sites = Vec::new();
    identifiers(&parsed.node_map, parsed.source_file.node_id.unwrap(), "instance", &mut sites);
    let hidden = bound
        .resolve_name(&parsed.nodes, &parsed.node_map, sites[0], "Hidden", SymbolFlags::NAMESPACE)
        .unwrap();
    let class = *bound.symbols().get(hidden).exports.get("C").unwrap();
    let mut checker = Checker::new(&bound, &parsed.nodes, &parsed.node_map);
    let instance = checker.get_declared_type_of_symbol(class);
    let object = checker.get_type_of_symbol(hidden);
    assert_eq!(checker.type_to_string_at(instance, sites[0]).unwrap(), "Local.C");
    assert_eq!(checker.type_to_string_at(object, sites[0]).unwrap(), "typeof Hidden");
}

/// A qualified import-equals aliases the leaf itself, unlike the unqualified
/// namespace-object control above. Native prints `item : Renamed` and
/// `Renamed : typeof Renamed` for this exact asymmetric source.
#[test]
fn a_qualified_import_equals_can_name_the_class_itself() {
    let source = "namespace Source { export class C { p: number; } } namespace Use { export import Renamed = Source.C; export let item = new Renamed(); }";
    assert_eq!(declared_type_at(source, &["Source"], "C", "item", 0, false), "Renamed");
    assert_eq!(declared_type_at(source, &["Source"], "C", "item", 0, true), "typeof Renamed");
}

/// This queries the value alias at its declaration and at its use. It fails
/// when namespace exports reject ALIAS flags before the checker can resolve it.
#[test]
fn an_exported_qualified_alias_resolves_as_a_value_inside_its_namespace() {
    let source = "namespace Source { export class C { p: number; } } namespace Use { export import Renamed = Source.C; export let item = new Renamed(); }";
    let arena = Arena::new();
    let parsed = tsr_parser::parse(&arena, source);
    let bound = tsr_binder::bind(
        &arena,
        parsed.source_file,
        &parsed.nodes,
        tsr_binder::FileInfo { name: "t.ts", text: source },
    );
    let mut sites = Vec::new();
    identifiers(&parsed.node_map, parsed.source_file.node_id.unwrap(), "Renamed", &mut sites);
    let mut checker = Checker::new(&bound, &parsed.nodes, &parsed.node_map);
    for site in sites {
        let symbol = bound
            .resolve_name(&parsed.nodes, &parsed.node_map, site, "Renamed", SymbolFlags::VALUE)
            .unwrap();
        let ty = checker.get_type_of_symbol(symbol);
        assert_eq!(checker.type_to_string_at(ty, site).unwrap(), "typeof Renamed");
    }
}

/// Native prints the accessible enum owner in both the bare single-member
/// enum and an indexed member name. Neither rule changes enum values.
#[test]
fn an_enum_owner_alias_applies_to_single_and_quoted_member_spellings() {
    let source = "namespace Source { export enum Single { Only = 19 } export enum E { First = 3, 'odd-key' = 11 } } namespace Use { export import One = Source.Single; export import Mode = Source.E; export const single = One.Only; export const quoted = Mode['odd-key']; }";
    assert_eq!(declared_type_at(source, &["Source"], "Single", "single", 0, false), "One");
    assert_eq!(
        declared_type_at(source, &["Source", "Single"], "Only", "single", 0, true),
        "One.Only"
    );
    assert_eq!(
        declared_type_at(source, &["Source", "E"], "odd-key", "quoted", 0, true),
        "(typeof Mode)[\"odd-key\"]"
    );
}

/// The direct leaf name wins over an alias's export route in the same scope.
#[test]
fn an_alias_to_the_container_does_not_qualify_an_in_scope_leaf() {
    let source = "namespace M { export class C { p: number; } import Local = M; export let item = new C(); }";
    assert_eq!(declared_type_at(source, &["M"], "C", "item", 0, false), "C");
}

/// Native accepts the class alias as a constructor, but the interface alias
/// has no value meaning (`TypeOnly` and `new TypeOnly()` recover as any).
/// This port declines that invalid value expression rather than inventing a
/// constructor type from the qualified spelling.
#[test]
fn a_qualified_type_only_alias_does_not_acquire_a_value_meaning() {
    let source = "namespace Source { export interface Shape { tag: string; } export class C { count: number; } } namespace Use { export import TypeOnly = Source.Shape; export import Value = Source.C; export let good = new Value(); export let bad = new TypeOnly(); }";
    let arena = Arena::new();
    let parsed = tsr_parser::parse(&arena, source);
    let bound = tsr_binder::bind(
        &arena,
        parsed.source_file,
        &parsed.nodes,
        tsr_binder::FileInfo { name: "t.ts", text: source },
    );
    let mut checker = Checker::new(&bound, &parsed.nodes, &parsed.node_map);
    for (name, expected) in [("Value", Some("typeof Value")), ("TypeOnly", None)] {
        let mut sites = Vec::new();
        identifiers(&parsed.node_map, parsed.source_file.node_id.unwrap(), name, &mut sites);
        let site = *sites.last().unwrap();
        let symbol = bound
            .resolve_name(&parsed.nodes, &parsed.node_map, site, name, SymbolFlags::VALUE)
            .unwrap();
        let ty = checker.get_type_of_symbol(symbol);
        if let Some(expected) = expected {
            assert_eq!(checker.type_to_string_at(ty, site).unwrap(), expected);
        } else {
            assert_eq!(ty, checker.intrinsics().error, "a type-only alias has no value type");
            let Some(Node::NewExpression(expression)) =
                parsed.node_map.get(parsed.nodes.parent(site).unwrap())
            else {
                panic!("the last alias use is a constructor expression");
            };
            let constructed =
                checker.check_expression(tsr_ast::Expression::NewExpression(expression));
            assert_eq!(
                constructed,
                checker.intrinsics().error,
                "a type-only alias cannot be constructed"
            );
        }
    }
}

/// The same enum member type must be named independently at the two sites.
/// Pinned native prints `First.E.A` and `Second.E.B` (values 5 and 17).
#[test]
fn enum_export_routes_are_chosen_at_each_site() {
    let source = "namespace Source { export enum E { A = 5, B = 17 } } namespace FirstUse { import First = Source; export const first = First.E.A; } namespace SecondUse { import Second = Source; export const second = Second.E.B; }";
    assert_eq!(declared_type_at(source, &["Source", "E"], "A", "first", 0, true), "First.E.A");
    assert_eq!(declared_type_at(source, &["Source", "E"], "B", "second", 0, true), "Second.E.B");
}

/// An enum alias can also have the same name as its target. Native prints the
/// member bare under that alias in App, but still qualifies it outside App.
#[test]
fn a_same_name_enum_alias_stops_the_owner_qualifier_only_in_its_scope() {
    let source = "namespace Keyboard { export enum Key { UP = 5, DOWN = 17 } } namespace App { import Key = Keyboard.Key; export const selected = Key.UP; } const outside = Keyboard.Key.DOWN;";
    assert_eq!(declared_type_at(source, &["Keyboard", "Key"], "UP", "selected", 0, true), "Key.UP");
    assert_eq!(
        declared_type_at(source, &["Keyboard", "Key"], "DOWN", "outside", 0, true),
        "Keyboard.Key.DOWN"
    );
}

/// A same-name pure alias is usable, while the namespace it shadows still
/// needs its container. Native records `M : typeof A.M` at the namespace
/// declaration and `M : typeof M` at the expression below.
#[test]
fn an_equal_name_alias_does_not_hide_the_namespaces_own_meaning() {
    let source = "namespace Z.M { export function bar() { return ''; } } namespace A.M { export import M = Z.M; export function bar() {} M.bar(); }";
    assert_eq!(declared_type_at(source, &["Z"], "M", "M", 4, true), "typeof M");
    assert_eq!(declared_type_at(source, &["A"], "M", "M", 1, true), "typeof A.M");
}

/// Native's baseline flags retain `unique symbol` for an explicitly preserved
/// identity and widen an inferred copy to `symbol`. Neither becomes a typeof
/// export name just because the namespace has an accessible alias.
#[test]
fn an_export_alias_does_not_rename_or_preserve_a_copied_unique_symbol() {
    let source = "namespace Source { export declare const brand: unique symbol; } namespace Use { import Local = Source; export const identity: typeof Source.brand = Local.brand; export const copy = Local.brand; }";
    let arena = Arena::new();
    let parsed = tsr_parser::parse(&arena, source);
    let bound = tsr_binder::bind(
        &arena,
        parsed.source_file,
        &parsed.nodes,
        tsr_binder::FileInfo { name: "t.ts", text: source },
    );
    let mut checker = Checker::new(&bound, &parsed.nodes, &parsed.node_map);
    for (name, expected) in [("identity", "unique symbol"), ("copy", "symbol")] {
        let mut sites = Vec::new();
        identifiers(&parsed.node_map, parsed.source_file.node_id.unwrap(), name, &mut sites);
        let symbol = bound
            .resolve_name(&parsed.nodes, &parsed.node_map, sites[0], name, SymbolFlags::VALUE)
            .unwrap();
        let ty = checker.get_type_of_symbol(symbol);
        assert_eq!(checker.type_to_string_at(ty, sites[0]).unwrap(), expected);
    }
}

/// Direct aliases have equal-length native chains. `trySymbolTable` sorts
/// those by `compareSymbols`, so declaration order beats lexical name order.
#[test]
fn direct_alias_ties_use_declaration_order() {
    for (aliases, expected) in [
        ("import Zulu = Source.C; import Alpha = Source.C;", "Zulu"),
        ("import Alpha = Source.C; import Zulu = Source.C;", "Alpha"),
    ] {
        let source = format!(
            "namespace Source {{ export class C {{ p: number; }} }} {aliases} let outside = 1;"
        );
        assert_eq!(declared_type_at(&source, &["Source"], "C", "outside", 0, false), expected);
        assert_eq!(
            declared_type_at(&source, &["Source"], "C", "outside", 0, true),
            format!("typeof {expected}")
        );
    }
}

/// Ordering is per table, not global: an earlier outer alias cannot preempt
/// the innermost table, and the symbol's direct own name still wins there.
#[test]
fn direct_alias_ties_preserve_scope_and_own_name_priority() {
    let source = "namespace Source { export class C { p: number; } import Zulu = Source.C; import Alpha = Source.C; export let own = 1; } import Outer = Source.C; namespace Use { import Zulu = Source.C; import Alpha = Source.C; export let inside = 1; } let outside = 1;";
    for (site, expected) in [("own", "C"), ("inside", "Zulu"), ("outside", "Outer")] {
        assert_eq!(declared_type_at(source, &["Source"], "C", site, 0, false), expected);
        assert_eq!(
            declared_type_at(source, &["Source"], "C", site, 0, true),
            format!("typeof {expected}")
        );
    }
}

/// The two-file controls share one parser/binder identity space. Only the
/// namespace-import resolution seam differs from the one-file controls.
struct ExportLibrary(NodeId);

impl ModuleHost for ExportLibrary {
    fn resolved_module(&self, _importing_file: NodeId, specifier: &str) -> Option<NodeId> {
        (specifier == "./lib").then_some(self.0)
    }

    fn module_resolution_found(&self, importing_file: NodeId, specifier: &str) -> bool {
        self.resolved_module(importing_file, specifier).is_some()
    }
}

fn imported_class_names_at(library: &str) -> (String, String) {
    let arena = Arena::new();
    let library = arena.alloc_str(library);
    let source = "import * as Head from './lib'; const item = new Head.Zzz();";
    let mut nodes = NodeTable::new();
    let mut map = NodeMap::new();
    let mut bound = tsr_binder::BindResult::empty();
    let mut roots = Vec::new();
    for (name, text) in [("/lib.ts", &*library), ("/use.ts", source)] {
        let parsed = tsr_parser::parse_into(
            &arena,
            text,
            tsr_parser::ParseOptions::default(),
            &mut nodes,
            &mut map,
        );
        assert!(parsed.diagnostics.is_empty(), "fixture must parse");
        roots.push(parsed.source_file.node_id.unwrap());
        bound = tsr_binder::bind_into(
            bound,
            &arena,
            parsed.source_file,
            &nodes,
            tsr_binder::FileInfo { name, text },
        );
    }
    let class = (0..u32::try_from(nodes.len()).unwrap())
        .map(NodeId::new)
        .find(|&node| nodes.kind(node) == SyntaxKind::ClassDeclaration)
        .and_then(|node| bound.symbol_of(node))
        .unwrap();
    let class = bound.symbols().get(class).export_symbol.unwrap_or(class);
    let mut sites = Vec::new();
    identifiers(&map, roots[1], "item", &mut sites);
    let host = ExportLibrary(roots[0]);
    let mut checker = Checker::with_module_host(&bound, &nodes, &map, Some(&host));
    let instance = checker.get_declared_type_of_symbol(class);
    let constructor = checker.get_type_of_symbol(class);
    (
        checker.type_to_string_at(instance, sites[0]).unwrap(),
        checker.type_to_string_at(constructor, sites[0]).unwrap(),
    )
}

/// Pinned native executable: a direct C export beats earlier Zzz and later
/// Aaa aliases. With no direct export, declaration order selects Zzz, even
/// when a later export specifier happens to use the target's own name C.
#[test]
fn alias_export_tables_check_the_direct_name_before_renamed_aliases() {
    for (library, name) in [
        ("export { C as Zzz }; export class C { tag = 7; } export { C as Aaa };", "Head.C"),
        ("export { C as Zzz }; class C { tag = 7; } export { C as Aaa };", "Head.Zzz"),
        (
            "export { C as Zzz }; class C { tag = 7; } export { C }; export { C as Aaa };",
            "Head.Zzz",
        ),
    ] {
        assert_eq!(imported_class_names_at(library), (name.to_string(), format!("typeof {name}")));
    }
}

/// Native 5b1047d: Head/Twin are separate nonconstructable module values;
/// Raw retains the constructor and names the copied prototype's instance.
#[test]
fn class_namespace_imports_keep_distinct_module_value_identity() {
    let arena = Arena::new();
    let library = "class Foo { static left = 7; own!: string; } namespace Foo { export const right = 'right'; } export = Foo;";
    let source = "import * as Head from './lib'; import Raw = require('./lib'); import * as Twin from './lib'; const copy = Head; Head.left; Head.right; Head.prototype; Head.default; new Head(); new Raw();";
    let mut nodes = NodeTable::new();
    let mut map = NodeMap::new();
    let mut bound = tsr_binder::BindResult::empty();
    let mut roots = Vec::new();
    for (name, text) in [("/lib.ts", library), ("/use.ts", source)] {
        let parsed = tsr_parser::parse_into(
            &arena,
            text,
            tsr_parser::ParseOptions::default(),
            &mut nodes,
            &mut map,
        );
        assert!(parsed.diagnostics.is_empty());
        roots.push(parsed.source_file.node_id.unwrap());
        bound = tsr_binder::bind_into(
            bound,
            &arena,
            parsed.source_file,
            &nodes,
            tsr_binder::FileInfo { name, text },
        );
    }
    let locals = bound.locals(roots[1]).unwrap();
    let head = locals["Head"];
    let raw = locals["Raw"];
    let twin = locals["Twin"];
    let copy = locals["copy"];
    let host = ExportLibrary(roots[0]);
    let mut checker = Checker::with_module_host(&bound, &nodes, &map, Some(&host));
    let mut sites = Vec::new();
    identifiers(&map, roots[1], "copy", &mut sites);
    let site = sites[0];
    let head_type = checker.get_type_of_symbol(head);
    let raw_type = checker.get_type_of_symbol(raw);
    let twin_type = checker.get_type_of_symbol(twin);
    assert_ne!(head_type, raw_type);
    assert_ne!(head_type, twin_type);
    assert_eq!(checker.get_type_of_symbol(copy), head_type);
    assert_eq!(checker.type_to_string_at(head_type, site).as_deref(), Some("typeof Head"));
    assert_eq!(checker.type_to_string_at(twin_type, site).as_deref(), Some("typeof Twin"));
    assert_eq!(checker.type_to_string_at(raw_type, site).as_deref(), Some("typeof Raw"));
    assert!(checker.signatures_of_type(head_type).unwrap().is_empty());
    for name in ["left", "right"] {
        assert_eq!(
            checker.get_property_of_type(head_type, name).expect("clone retains property"),
            checker.get_property_of_type(raw_type, name).expect("source has property"),
            "copied property {name} retains its source symbol",
        );
    }
    for (name, expected) in [("left", "number"), ("right", "\"right\"")] {
        let property = checker.get_type_of_property_of_type(head_type, name).unwrap();
        assert_eq!(checker.type_to_string_at(property, site).as_deref(), Some(expected));
    }
    assert_eq!(checker.get_property_of_type(head_type, "own"), None);
    let prototype = checker.get_type_of_property_of_type(head_type, "prototype").unwrap();
    assert_eq!(checker.type_to_string_at(prototype, site).as_deref(), Some("Raw"));
    let default = checker.get_type_of_property_of_type(head_type, "default").unwrap();
    assert_eq!(default, raw_type);
    for name in ["Head", "Raw"] {
        let mut uses = Vec::new();
        identifiers(&map, roots[1], name, &mut uses);
        let last_use = *uses.last().unwrap();
        let Some(Node::NewExpression(expression)) = map.get(nodes.parent(last_use).unwrap()) else {
            panic!("the last alias use is a constructor expression");
        };
        let constructed = checker.check_expression(tsr_ast::Expression::NewExpression(expression));
        if name == "Raw" {
            assert_eq!(constructed, prototype, "require alias retains the class constructor");
        } else {
            // Native identity probe: intrinsic "error", not intrinsic "any".
            // Preserve errorType rather than borrowing Raw's constructor.
            assert_eq!(constructed, checker.intrinsics().error);
        }
    }
}

#[test]
fn module_class_clone_boundary_does_not_admit_other_export_meanings() {
    for library in [
        "interface Foo { tag: string; } export = Foo;",
        "const Foo = class Inner { tag!: string; }; export = Foo;",
        "function Foo() {} export = Foo;",
        "import Foo = Foo; export = Foo;",
    ] {
        let arena = Arena::new();
        let source = "import * as Head from './lib'; import Raw = require('./lib'); Head; Raw;";
        let mut nodes = NodeTable::new();
        let mut map = NodeMap::new();
        let mut bound = tsr_binder::BindResult::empty();
        let mut roots = Vec::new();
        for (name, text) in [("/lib.ts", library), ("/use.ts", source)] {
            let parsed = tsr_parser::parse_into(
                &arena,
                text,
                tsr_parser::ParseOptions::default(),
                &mut nodes,
                &mut map,
            );
            assert!(parsed.diagnostics.is_empty());
            roots.push(parsed.source_file.node_id.unwrap());
            bound = tsr_binder::bind_into(
                bound,
                &arena,
                parsed.source_file,
                &nodes,
                tsr_binder::FileInfo { name, text },
            );
        }
        let locals = bound.locals(roots[1]).unwrap();
        let host = ExportLibrary(roots[0]);
        let mut checker = Checker::with_module_host(&bound, &nodes, &map, Some(&host));
        let head = checker.get_type_of_symbol(locals["Head"]);
        let raw = checker.get_type_of_symbol(locals["Raw"]);
        assert_eq!(head, raw, "outside this class-symbol clone unit: {library}");
        if library.starts_with("interface") || library.starts_with("import") {
            assert_eq!(head, checker.intrinsics().error, "unsupported meaning/cycle keeps a gap");
        }
    }
}

/// Native prints import("./lib") for the prototype, not Head, when the
/// original class has no accessible constructor alias. That import-type
/// fallback remains unsupported; the cloned value still has its own name.
#[test]
fn a_module_clone_alias_does_not_name_an_inaccessible_original_class() {
    let arena = Arena::new();
    let library = "class Foo { own!: string; } export = Foo;";
    let source = "import * as Head from './lib'; Head.prototype;";
    let mut nodes = NodeTable::new();
    let mut map = NodeMap::new();
    let mut bound = tsr_binder::BindResult::empty();
    let mut roots = Vec::new();
    for (name, text) in [("/lib.ts", library), ("/use.ts", source)] {
        let parsed = tsr_parser::parse_into(
            &arena,
            text,
            tsr_parser::ParseOptions::default(),
            &mut nodes,
            &mut map,
        );
        assert!(parsed.diagnostics.is_empty());
        roots.push(parsed.source_file.node_id.unwrap());
        bound = tsr_binder::bind_into(
            bound,
            &arena,
            parsed.source_file,
            &nodes,
            tsr_binder::FileInfo { name, text },
        );
    }
    let head = bound.locals(roots[1]).unwrap()["Head"];
    let host = ExportLibrary(roots[0]);
    let mut checker = Checker::with_module_host(&bound, &nodes, &map, Some(&host));
    let mut sites = Vec::new();
    identifiers(&map, roots[1], "Head", &mut sites);
    let value = checker.get_type_of_symbol(head);
    assert_eq!(checker.type_to_string_at(value, sites[1]).as_deref(), Some("typeof Head"));
    let instance = checker.get_type_of_property_of_type(value, "prototype").unwrap();
    assert_eq!(checker.type_to_string_at(instance, sites[1]), None);
}
