//! A type reference `M.I` whose leftmost name **resolves as a namespace**
//! prints the entity name that was written — **design W** of
//! `docs/architecture/checker-notes-qualname.md` — and refuses to print it when
//! the reference site is inside the namespace it qualifies.
//!
//! # Every fixture here is a real baseline, and two of them are refusals
//!
//! Five expectations written from intuition were wrong in an earlier session
//! and the port was right every time, so each test below names the
//! `.types` baseline it was derived from. The two most load-bearing are
//! **negative**:
//!
//! - `compiler/privacyFunctionParameterDeclFile.types:779` records
//!   `>param : publicClass` — **bare** — for a parameter written
//!   `param: privateModule.publicClass` *inside* `namespace privateModule`
//!   (that namespace opens at `:580`). `needsQualification`
//!   (`symbolaccessibility.go:688`) is why: `publicClass` is already in scope
//!   there, so upstream drops the qualifier and the written text is
//!   over-qualified by construction. 79 of the counterfactual's 99 would-be
//!   wrong lines came from that one position.
//! - `compiler/moduleVisibilityTest4.types:13` records `>a1 : M.num` for a name
//!   `M` **does not export**. Upstream mints its unresolved symbol and prints
//!   the dotted text; this port gaps instead, because
//!   [the export lookup] is the only part of `resolveQualifiedName` it has and
//!   a miss there is not the same as a miss on the whole entity name. That is
//!   the counterfactual's 291-line `the namespace has no export of that name`
//!   decline bucket, asserted so that closing it is a visible change rather
//!   than a silent one.

use tsr_ast::{Node, NodeId, NodeMap, TypeNode, push_children};
use tsr_checker::Checker;
use tsr_core::Arena;

/// Every `TypeReferenceNode` in the file, in source order.
fn type_references(map: &NodeMap<'_>, root: NodeId, out: &mut Vec<NodeId>) {
    let Some(node) = map.get(root) else { return };
    if matches!(node, Node::TypeReferenceNode(_)) {
        out.push(root);
    }
    let mut children = Vec::new();
    push_children(node, &mut children);
    for child in children {
        if let Some(id) = child.node_id() {
            type_references(map, id, out);
        }
    }
}

/// What the `index`-th type reference in `source` prints.
fn type_of_reference(source: &str, index: usize) -> String {
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
    let mut references = Vec::new();
    let root = parsed.source_file.node_id.expect("the file has a node id");
    type_references(&parsed.node_map, root, &mut references);
    let id = *references.get(index).expect("that many type references");
    let Some(Node::TypeReferenceNode(reference)) = parsed.node_map.get(id) else {
        panic!("a type reference")
    };
    let id = checker.get_type_from_type_node(TypeNode::TypeReferenceNode(reference));
    checker.type_to_string(id)
}

/// Baseline: `compiler/moduleAndInterfaceSharingName.types:13` records
/// `>z2 : X.Y` for `var z2: X.Y;`.
///
/// The site is outside `X`, so upstream qualifies and the written text is what
/// upstream's chain would have rebuilt. Note that `Y` is *both* a namespace and
/// an interface here — the export lookup takes it on `SymbolFlags::TYPE`, which
/// is `resolveTypeReferenceName`'s meaning.
#[test]
fn a_qualified_name_resolved_from_outside_prints_what_was_written() {
    let source = "namespace X { export namespace Y { export interface Z { } } export interface Y { } }\nvar z2: X.Y;";
    assert_eq!(type_of_reference(source, 0), "X.Y");
}

/// Baseline: `compiler/moduleAndInterfaceSharingName.types:8` records
/// `>z : X.Y.Z` for `var z: X.Y.Z = null;`.
///
/// Three segments, so `resolveEntityName` recurses: `X.Y` resolves with meaning
/// `SymbolFlagsNamespace` and `Z` is then read out of its exports
/// (`checker.go:15851`).
#[test]
fn a_three_segment_name_resolves_left_to_right() {
    let source = "namespace X { export namespace Y { export interface Z { } } export interface Y { } }\nvar z: X.Y.Z;";
    assert_eq!(type_of_reference(source, 0), "X.Y.Z");
}

/// **The refusal.** Baseline:
/// `compiler/privacyFunctionParameterDeclFile.types:779` records
/// `>param : publicClass` for `param: privateModule.publicClass` written inside
/// `namespace privateModule` (`:580`).
///
/// Reprinting the written text there would print `privateModule.publicClass`
/// where upstream prints `publicClass` — a manufactured wrong answer, not a
/// missing one. `needsQualification` (`symbolaccessibility.go:688`) answers *no
/// qualifier needed* the moment a symbol table in scope holds the symbol
/// itself, and inside `privateModule` one does.
///
/// **§925: the refusal is LIFTED, and the number is why.** This test recorded
/// *"removing the refusal turns this test green and adds 222 wrong lines to the
/// corpus, measured"*, and explicitly said the gap was not permanent. Re-measured
/// on a checker many entries further along, removal is **320 `WRONG->RIGHT` + 92
/// `GAP->RIGHT` against 48 `GAP->WRONG` and 1 `RIGHT->WRONG`** —
/// `bluebirdStaticThis` 69, `complexRecursiveCollections` 51,
/// `resolvingClassDeclarationWhenInBaseTypeResolution` 51. The ratio has flipped
/// from −222 to +411.
///
/// The correctness concern the refusal named is real and survives as the 48
/// `GAP->WRONG`: inside `privateModule`, upstream prints `publicClass` where this
/// port now prints `privateModule.publicClass`. That is `needsQualification`
/// (`symbolaccessibility.go:688`) — *no qualifier needed once a symbol table in
/// scope holds the symbol* — and it is the named completion. **48 knowingly
/// imprecise names against 412 recovered ones is the trade taken**, and it is
/// `GAP->WRONG`, §620's accepted direction, not `RIGHT->WRONG`.
#[test]
fn a_reference_inside_the_namespace_it_qualifies_now_resolves() {
    let source = "namespace privateModule { export class publicClass { }\n export function f(param: privateModule.publicClass) { } }";
    assert_eq!(type_of_reference(source, 0), "privateModule.publicClass");
}

/// The **pair** of the test above: the same namespace and the same interface,
/// referenced from outside, still converts. Without this the refusal above
/// would be indistinguishable from the arm not firing at all.
#[test]
fn the_same_name_from_outside_that_namespace_still_prints() {
    let source = "namespace privateModule { export class publicClass { } }\nlet p: privateModule.publicClass;";
    assert_eq!(type_of_reference(source, 0), "privateModule.publicClass");
}

/// Baseline: `compiler/moduleVisibilityTest4.types:13` records `>a1 : M.num`
/// where `M` exports `nums` and not `num`.
///
/// **§605 CLOSED the shortfall this fixture pinned, and the fixture named its
/// own fix.** What stood here read: *"This port answers `error` and upstream
/// answers `M.num` … upstream falls through to
/// `getUnresolvedSymbolForEntityName` (`checker.go:23102`) when
/// `resolveQualifiedName` returns nil; this port reaches that path only when
/// the *leftmost* name fails to resolve … 291 corpus lines sit here."*
///
/// That is exactly what §605 changed, and the estimate was good: **305 lines
/// converted (78 GAP→RIGHT, 227 WRONG→RIGHT) for +18 cases, zero R→W.** The
/// comment had carried the diagnosis, the upstream anchor and the line count
/// for as long as it had existed — the work was reading it, not finding it.
///
/// The test now pins the MATCH, so a regression here is a regression.
#[test]
fn a_name_the_namespace_does_not_export_prints_the_written_text() {
    let source = "namespace M { export type nums = number; }\nlet a1: M.num;";
    assert_eq!(type_of_reference(source, 0), "M.num");
}

/// Baseline: `compiler/moduleVisibilityTest4.types:17` records `>b1 : number`
/// for `let b1: M.nums;` where `M` exports `type nums = number`.
///
/// **This port prints `M.nums` and upstream prints `number`, so this fixture
/// pins a known wrong line.** Reprinting the written text is right for a class
/// or an interface, whose printed form *is* a name; it is wrong for an alias to
/// a primitive, which upstream resolves through and prints the aliased type.
/// Two corpus lines, and the shape is in
/// `docs/architecture/checker-notes-qualname.md` §9's residual. Asserted so
/// that a build teaching this arm to consult the resolved symbol's declared
/// type flips a test rather than moving a number nobody is watching.
#[test]
fn an_alias_to_a_primitive_prints_the_written_name_and_upstream_does_not() {
    let source = "namespace M { export type nums = number; }\nlet b1: M.nums;";
    assert_eq!(type_of_reference(source, 0), "M.nums");
}

/// A gap **inside** a type argument gaps the whole reference, which is
/// [`crate::Checker::unresolved_type_reference`]'s rule reached through the same
/// door. The counterfactual's largest decline bucket — 1,085 lines, 23.8% —
/// is exactly this, and it is the `docs/conventions.md` *"every part is handled,
/// something downstream gapped"* shape rather than a shortfall of this arm.
#[test]
fn a_gapped_type_argument_gaps_the_whole_qualified_reference() {
    let source = "namespace M { export interface I<T> { } }\nlet x: M.I<Unresolvable[]>;";
    assert_eq!(type_of_reference(source, 0), "error");
}
