//! A spread member's declaration reuse is decided when the member is printed
//! (`docs/parity/notes/r6-lazytext.md` §1). Native's
//! addPropertyToElementList asks serializeTypeForDeclaration
//! (`nodebuilderimpl.go:2486`, `:2181`) only while the node builder prints;
//! the mint keeps the displayed type's own print.

use tsr_ast::{Node, NodeId, Statement};
use tsr_checker::{Checker, TypeId};
use tsr_core::Arena;

/// Run `ask` against the type of the final expression statement, with that
/// statement's expression as the print site.
fn at_last_expression<R>(
    source: &str,
    ask: impl FnOnce(&mut Checker<'_, '_>, TypeId, NodeId) -> R,
) -> R {
    let arena = Arena::new();
    let parsed = tsr_parser::parse(&arena, source);
    assert!(parsed.diagnostics.is_empty(), "fixture must parse");
    let bound = tsr_binder::bind(
        &arena,
        parsed.source_file,
        &parsed.nodes,
        tsr_binder::FileInfo { name: "test.ts", text: source },
    );
    let mut checker = Checker::new(&bound, &parsed.nodes, &parsed.node_map);
    let options = tsr_core::CompilerOptions {
        strict_null_checks: tsr_core::Tristate::True,
        ..Default::default()
    };
    checker.apply_compiler_options(&options);
    let last = parsed
        .source_file
        .statements
        .iter()
        .rev()
        .find_map(|statement| match statement {
            Statement::ExpressionStatement(node) => node.expression,
            _ => None,
        })
        .expect("the fixture must end with an expression statement");
    let site = Node::from(last).node_id().expect("a parsed node");
    let id = checker.check_expression(last);
    ask(&mut checker, id, site)
}

#[test]
fn a_spread_member_reuses_its_declaration_when_printed() {
    // unionExcessPropsWithPartialMember: tsgo prints `unused?: string`, the
    // optional property's written annotation. The baked (site-free) text
    // is the displayed type's own print: no reuse ran at the mint.
    at_last_expression(
        "interface A { unused?: string; x: string; }\ndeclare var a: A;\n({ ...a });",
        |checker, id, site| {
            assert_eq!(checker.type_to_string(id), "{ unused?: string | undefined; x: string; }");
            assert_eq!(
                checker.type_to_string_at(id, site).as_deref(),
                Some("{ unused?: string; x: string; }")
            );
        },
    );
}

#[test]
fn a_spread_member_whose_type_moved_prints_its_type() {
    // An optional right `x` merges with the left's: the member's type is
    // `string | number`, to which neither declaration's annotation is
    // equivalent, so the reuse declines at print and the baked print stays.
    at_last_expression(
        "declare const a: { x: string };\ndeclare const c: { x?: number };\n({ ...a, ...c });",
        |checker, id, site| {
            assert_eq!(checker.type_to_string(id), "{ x: string | number; }");
            assert_eq!(
                checker.type_to_string_at(id, site).as_deref(),
                Some("{ x: string | number; }")
            );
        },
    );
}
