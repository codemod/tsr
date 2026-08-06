//! An exported namespace member must resolve — `bd tsr-56r`.
//!
//! An exported declaration gets **two** symbols (`declareModuleMember`,
//! `binder.go:397`–`:409`): an *export* symbol in the container symbol's
//! `exports` carrying the real flags, and a *local* carrying only
//! `SymbolFlagsExportValue` — or **no flags at all** for a type-only
//! declaration. So resolution needs both halves of upstream's rule: the locals
//! lookup filters by meaning (`nameresolver.go:418`) so the flagless local does
//! not shadow, and the walk consults the container's `exports`
//! (`nameresolver.go:104`).
//!
//! Before both landed, `export` made a name **unresolvable**, which is the
//! defect these pin.

use tsr_binder::SymbolFlags;

/// Resolve `name` for `meaning`, starting from the last node of the given kind.
fn resolves(source: &str, name: &str, meaning: SymbolFlags) -> bool {
    let arena = tsr_core::Arena::new();
    let parsed = tsr_parser::parse(&arena, source);
    assert!(parsed.diagnostics.is_empty(), "fixture must parse");
    let bound = tsr_binder::bind(
        parsed.source_file,
        &parsed.nodes,
        tsr_binder::FileInfo { name: "t.ts", text: source },
    );
    // Start from the **last occurrence of the name in source order**, which in
    // every fixture here is the reference rather than the declaration. Starting
    // from the last node *by id* instead put the walk at the source file, where
    // the namespace's exports are not in scope — five of these seven tests
    // failed for that reason and not for the reason they are about.
    let start = (0..parsed.nodes.len())
        .map(|i| tsr_ast::NodeId::new(u32::try_from(i).unwrap()))
        .filter(|id| {
            parsed.nodes.kind(*id) == tsr_ast::SyntaxKind::Identifier
                && matches!(parsed.node_map.get(*id), Some(tsr_ast::Node::Identifier(n)) if n.text == name)
        })
        .max_by_key(|id| parsed.nodes.span(*id).start)
        .expect("the fixture must mention the name");
    bound.resolve_name(&parsed.nodes, &parsed.node_map, start, name, meaning).is_some()
}

#[test]
fn an_exported_class_resolves_as_a_type_inside_its_namespace() {
    assert!(resolves("namespace N { export class C {} let x: C; }", "C", SymbolFlags::TYPE));
}

#[test]
fn an_exported_class_resolves_as_a_value_inside_its_namespace() {
    assert!(resolves("namespace N { export class C {} let x = C; }", "C", SymbolFlags::VALUE));
}

/// The type-only case, where the local carries **no flags at all** — the one
/// that makes the meaning filter load-bearing rather than tidy.
#[test]
fn an_exported_interface_resolves_as_a_type() {
    assert!(resolves("namespace N { export interface I {} let x: I; }", "I", SymbolFlags::TYPE));
}

/// The control that says the filter did not simply open everything up: an
/// exported *interface* is not a value, and upstream's mask is what refuses it.
#[test]
fn an_exported_interface_is_not_a_value() {
    assert!(!resolves("namespace N { export interface I {} let x: I; }", "I", SymbolFlags::VALUE));
}

/// A non-exported member still resolves — the case that worked before and must
/// keep working, since the filter now rejects symbols it used to accept.
#[test]
fn a_non_exported_member_still_resolves() {
    assert!(resolves("namespace N { class C {} let x: C; }", "C", SymbolFlags::TYPE));
}

/// `case KindSourceFile:` falls through to the module arm upstream. Omitting it
/// cost 1,621 lines across 219 cases on a measured run.
#[test]
fn a_modules_own_top_level_export_is_in_scope_inside_it() {
    let source = "export class C {} let x: C; export {};";
    assert!(resolves(source, "C", SymbolFlags::TYPE));
}

/// A local shadows an export of the same name, because upstream tests locals
/// first at each location and so does this.
#[test]
fn a_local_shadows_an_export_of_the_same_name() {
    let source = "namespace N { export class C {} function f() { class C {} let x: C; } }";
    assert!(resolves(source, "C", SymbolFlags::TYPE));
}
