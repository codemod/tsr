//! §473 — `isAssignmentToReadonlyEntity`'s element-access half
//! (`checker.go:11377` via the write-position dispatch): a READONLY property
//! as an element-access assignment target answers upstream's `errorType`,
//! printed `any`. The property-access twin has carried this since §27; the
//! element road gapped it (`constDeclarations-access3/4/5`).

use tsr_checker::Checker;
use tsr_core::Arena;

/// The printed type of the first element access in the fixture.
fn type_of_element_access(source: &str) -> String {
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
    let mut stack = vec![tsr_ast::Node::SourceFile(parsed.source_file)];
    let mut children = Vec::new();
    while let Some(node) = stack.pop() {
        if let tsr_ast::Node::ElementAccessExpression(access) = node {
            let ty = checker.check_expression(tsr_ast::Expression::ElementAccessExpression(access));
            return checker.type_to_string(ty);
        }
        children.clear();
        tsr_ast::push_children(node, &mut children);
        stack.extend(children.iter().copied());
    }
    panic!("the fixture must contain an element access");
}

/// `constDeclarations-access3` records `M["x"] : any` for a write against an
/// exported `const` — `isReadonlySymbol`'s const-variable arm through the
/// element road. Deleting §473's readonly test in `indexed.rs` reddens this.
#[test]
fn a_const_export_written_through_an_element_access_is_error_any() {
    let source = "namespace M { export const x = 0; }
M[\"x\"] = 1;";
    assert_eq!(type_of_element_access(source), "any");
}

/// The control: the same shape against a mutable export answers the
/// property's declared type — the write-position dispatch, not a blanket
/// rule about namespaces or element writes.
#[test]
fn a_mutable_export_written_the_same_way_keeps_its_type() {
    let source = "namespace M { export let x = 0; }
M[\"x\"] = 1;";
    assert_eq!(type_of_element_access(source), "number");
}

/// §477 `isForInVariableForNumericPropertyNames` (`checker.go:8179`): the
/// for-in variable of a loop over a NUMERIC-only-indexed object reads as
/// `number` at the access, though its own type is `string`
/// (`capturedLetConstInLoop1` records `iobj[ix] : any` from
/// `{ [x: number]: any }`). Deleting the effective-index substitution in
/// `indexed.rs` reddens this to `error` (no applicable string signature).
#[test]
fn a_for_in_variable_over_a_numeric_index_reads_the_numeric_signature() {
    let source = "declare const iobj: { [x: number]: any };
for (let ix in iobj) {
    iobj[ix];
}";
    assert_eq!(type_of_element_access(source), "any");
}

/// The control: a for-in over an object that ALSO has a string signature is
/// not the numeric special case — `hasNumericPropertyNames` requires exactly
/// one index info.
#[test]
fn a_for_in_over_a_string_indexed_object_stays_on_the_string_signature() {
    let source = "declare const sobj: { [x: string]: boolean };
for (let ix in sobj) {
    sobj[ix];
}";
    assert_eq!(type_of_element_access(source), "boolean");
}
