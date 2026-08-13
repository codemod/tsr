//! §230. The expression of an `export = x` is not flow-narrowed.
//!
//! It is the entity name of an ALIAS declaration
//! (`getTargetOfExportAssignment`, `checker.go:14889`), so it answers the
//! symbol's declared type and never enters the flow walk. For an auto-typed
//! `var x;` that means `any`, where an ordinary reference to the same symbol
//! narrows to `undefined`.
//!
//! # Both halves come from upstream executed, not upstream read
//!
//! A static read of `checker.go:11149-11190` predicted `undefined` in both
//! positions — which is what this port did, and it is wrong. The answer came
//! from running upstream's own `TestLocal` baseline runner over a probe
//! fixture, which records, in one file and on one symbol:
//!
//! ```text
//! x;
//! >x : undefined
//!
//! export = x;
//! >x : any
//! ```
//!
//! **A symbol answering two ways at two sites is what rules out every
//! symbol-level disjunct of `assumeInitialized`** (`:11150-11158`) — `isAlias`,
//! `isModuleExports`, `isParameter` and the rest cannot vary by reference site.
//! That elimination is the finding; the arm is the easy part.
//!
//! The control below is therefore load-bearing in a way controls usually are
//! not: it pins the half upstream says this port already had **right**, and a
//! wider fix — "an auto-typed variable always reads `any`" — passes the first
//! assertion and fails it.

use tsr_ast::{Node, NodeId, SyntaxKind};
use tsr_checker::Checker;
use tsr_core::Arena;

/// The type of the identifier named `name` whose parent kind is `parent_kind`.
fn type_of_identifier_under(source: &str, name: &str, parent_kind: SyntaxKind) -> String {
    let arena = Arena::new();
    let parsed = tsr_parser::parse(&arena, source);
    let bound = tsr_binder::bind(
        &arena,
        parsed.source_file,
        &parsed.nodes,
        tsr_binder::FileInfo { name: "t.ts", text: source },
    );
    let mut checker = Checker::new(&bound, &parsed.nodes, &parsed.node_map);
    let found = (0..u32::try_from(parsed.nodes.len()).expect("fits"))
        .map(NodeId::new)
        .filter(|&id| parsed.nodes.kind(id) == SyntaxKind::Identifier)
        .filter(|&id| parsed.nodes.parent(id).is_some_and(|p| parsed.nodes.kind(p) == parent_kind))
        .find(|&id| matches!(parsed.node_map.get(id), Some(Node::Identifier(i)) if i.text == name))
        .unwrap_or_else(|| panic!("no `{name}` under {parent_kind:?}"));
    let Some(Node::Identifier(identifier)) = parsed.node_map.get(found) else {
        panic!("an identifier")
    };
    let id = checker.check_expression(tsr_ast::Expression::Identifier(identifier));
    checker.type_to_string(id)
}

#[test]
fn the_expression_of_an_export_assignment_is_not_flow_narrowed() {
    assert_eq!(
        type_of_identifier_under("var x;\nx;\nexport = x;", "x", SyntaxKind::ExportAssignment),
        "any"
    );
}

/// The half upstream says this port already had right, and the reason a wider
/// rule is wrong rather than merely broader.
#[test]
fn an_ordinary_reference_to_the_same_symbol_still_narrows() {
    assert_eq!(
        type_of_identifier_under("var x;\nx;\nexport = x;", "x", SyntaxKind::ExpressionStatement),
        "undefined"
    );
}
