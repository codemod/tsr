//! A type reference whose name does not resolve prints **the name that was
//! written** — `bd tsr-eep`.
//!
//! This looks like the one thing this port refuses everywhere else: answering
//! something for a name it could not resolve. It is the opposite. Upstream
//! reports `TS2304 Cannot find name` *and prints the name anyway*
//! (`getUnresolvedSymbolForEntityName`, `checker.go:23102`; the
//! `CheckFlagsUnresolved` branch of `getTypeFromTypeAliasReference`,
//! `checker.go:23580`). `conformance/parserRealSource11` carries **1,006** of
//! those errors, records `>nodeType : NodeType` throughout, and contains
//! **zero** ` : any` lines. Answering `errorType` was the divergence.

use tsr_ast::Statement;
use tsr_checker::Checker;
use tsr_core::Arena;

fn type_of_annotation(source: &str) -> String {
    let arena = Arena::new();
    let parsed = tsr_parser::parse(&arena, source);
    assert!(parsed.diagnostics.is_empty(), "fixture must parse");
    let bound = tsr_binder::bind(
        parsed.source_file,
        &parsed.nodes,
        tsr_binder::FileInfo { name: "t.ts", text: source },
    );
    let mut checker = Checker::new(&bound, &parsed.nodes, &parsed.node_map);
    let last = *parsed.source_file.statements.last().expect("a statement");
    let Statement::VariableStatement(statement) = last else { panic!("want a var statement") };
    let declaration = statement
        .declaration_list
        .and_then(|list| list.declarations.first().copied())
        .expect("one declaration");
    let annotation = declaration.r#type.expect("an annotation");
    let id = checker.get_type_from_type_node(annotation);
    checker.type_to_string(id)
}

/// Baseline: `conformance/parserRealSource11` records `>nodeType : NodeType`
/// with 348 `Cannot find name 'NodeType'` errors against it.
#[test]
fn an_unresolved_name_prints_itself() {
    assert_eq!(type_of_annotation("let x: NodeType;"), "NodeType");
}

/// A qualified name is minted **only when its root does not resolve**.
///
/// `resolveEntityName` is unported, so `M.I` where `M` is a real namespace has
/// to keep gapping — this port cannot compute it, and upstream's answer there
/// depends on `getAccessibleSymbolChain` (`bd tsr-awa`). The discriminator is
/// upstream's own control flow: `getUnresolvedSymbolForEntityName` is reached
/// *only* when `resolveEntityName` failed, and that begins by resolving the
/// leftmost name.
///
/// Shipping this arm ungated gained 8,797 corpus lines against 4,645 gated —
/// and a `matched`-count bar could not tell how many of the extra 4,152 were
/// gap→**wrong**, because a gap that becomes wrong moves nothing the bar
/// watches. Measured through the gap count instead: the gated arm converts
/// 4,645 and turns 135 gaps into wrong lines, 34:1.
#[test]
fn an_unresolved_qualified_name_is_minted_when_its_root_does_not_resolve() {
    assert_eq!(type_of_annotation("let x: TypeScript.AST;"), "TypeScript.AST");
}

/// The other arm: the root resolves, so upstream had a real symbol and this
/// port must keep gapping until `resolveEntityName` lands.
#[test]
fn a_qualified_name_whose_root_resolves_still_gaps() {
    let source = "namespace M { export interface I {} }\nlet x: M.I;";
    assert_eq!(type_of_annotation(source), "error");
}

/// Type arguments go on the alias upstream, so they are printed.
#[test]
fn type_arguments_are_printed() {
    assert_eq!(type_of_annotation("let x: Foo<string>;"), "Foo<string>");
}

/// A gap **inside** an argument makes the whole reference a gap: printing
/// `Foo<error>` would be a wrong line rather than a missing one.
/// The assertion here was first written as the *printed* form, contradicting
/// the test's own name — `Foo<…>` is exactly what must **not** come out. The
/// argument `Bar[]` gaps (no global `Array` in this harness), so the whole
/// reference gaps.
#[test]
fn a_gapped_type_argument_gaps_the_whole_reference() {
    assert_eq!(type_of_annotation("let x: Foo<Bar[]>;"), "error");
}

/// **The design constraint.** The minted type still answers `is_error`, so
/// every consumer keeps treating it as a gap and keeps propagating. Without
/// that, `check_arithmetic_operation` would see `TypeFlags::ANY` and answer
/// `number` for an operand nobody could resolve — and `check_addition` would
/// answer `any`, which ADR-0038 and ADR-0039 both forbid.
///
/// Deleting the `unresolved_types` clause in `Checker::is_error` turns this
/// red: the answer becomes `number`.
#[test]
fn an_unresolved_operand_still_propagates_as_a_gap() {
    let source = "let a: NodeType; let x = a * 2;";
    let arena = Arena::new();
    let parsed = tsr_parser::parse(&arena, source);
    let bound = tsr_binder::bind(
        parsed.source_file,
        &parsed.nodes,
        tsr_binder::FileInfo { name: "t.ts", text: source },
    );
    let mut checker = Checker::new(&bound, &parsed.nodes, &parsed.node_map);
    let Statement::VariableStatement(statement) = parsed.source_file.statements[1] else {
        panic!("want a var statement")
    };
    let initialiser = statement
        .declaration_list
        .and_then(|list| list.declarations.first().copied())
        .and_then(|d| d.initializer)
        .expect("an initialiser");
    let id = checker.check_expression(initialiser);
    assert_eq!(checker.type_to_string(id), "error");
}
