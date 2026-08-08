//! An export marker declared inside a `namespace`.
//!
//! `tests/export_markers.rs` covers the marker at a module file's top level,
//! which is the only shape the old file-walking `export_symbol_of` could reach:
//! it walked to the enclosing `SourceFile` and read *that file's* module
//! symbol's `exports`.
//!
//! For `namespace N { export enum E {} }` the export symbol is on **`N`**, and
//! the file's exports have no `E` at all. Worse, a **script** file — no
//! top-level `import`/`export`, which is what every fixture here is — has no
//! module symbol, so the old lookup returned `None` before consulting any table.
//!
//! Measured at `058b4a9` at **2,175 assertion lines** over the corpus, 988 of
//! them in a script file. See `docs/architecture/checker-notes-nameres.md` §5.

use tsr_ast::{ModuleBody, Statement};
use tsr_checker::Checker;
use tsr_core::Arena;

/// Type the initialiser of the last statement **inside the first namespace**.
///
/// Deliberately not the file's last statement: an unqualified reference from
/// inside the namespace body is the only position that reaches the marker at
/// all. A reference from outside (`N.E`) goes through `N`'s exports directly
/// and would pass with no link whatever.
fn type_inside_namespace(source: &str) -> String {
    let arena = Arena::new();
    let parsed = tsr_parser::parse(&arena, source);
    assert!(
        parsed.diagnostics.is_empty(),
        "fixture must parse: {:?}",
        parsed.diagnostics.iter().map(tsr_diagnostics::Diagnostic::text).collect::<Vec<_>>()
    );
    let bound = tsr_binder::bind(
        &arena,
        parsed.source_file,
        &parsed.nodes,
        tsr_binder::FileInfo { name: "test.ts", text: source },
    );
    let mut checker = Checker::new(&bound, &parsed.nodes, &parsed.node_map);

    let Some(Statement::ModuleDeclaration(module)) = parsed.source_file.statements.first().copied()
    else {
        panic!("the first statement must be a namespace");
    };
    let Some(ModuleBody::ModuleBlock(block)) = module.body else {
        panic!("the namespace must have a block body");
    };
    let index = block.statements.len() - 1;
    let Statement::VariableStatement(statement) = block.statements[index] else {
        panic!("the namespace's last statement must be a variable statement");
    };
    let initialiser = statement
        .declaration_list
        .and_then(|list| list.declarations.first().copied())
        .and_then(|declaration| declaration.initializer)
        .expect("an initialiser");
    let id = checker.check_expression(initialiser);
    checker.type_to_string(id)
}

#[test]
fn a_reference_inside_a_namespace_reaches_the_namespaces_export() {
    // Each of these resolves `f`/`C`/`x` to the **marker** in the namespace's
    // locals. Its export symbol lives in `N`'s exports, which the file-level
    // lookup never consulted — and this file is a script, so that lookup had no
    // module symbol to start from either.
    assert_eq!(
        type_inside_namespace("namespace N { export function f() {} export var q = f; }"),
        "() => void"
    );
    assert_eq!(
        type_inside_namespace("namespace N { export class C {} export var q = C; }"),
        "typeof C"
    );
    assert_eq!(
        type_inside_namespace("namespace N { export var x: string; export var q = x; }"),
        "string"
    );
}

#[test]
fn exporting_from_a_namespace_does_not_change_the_type_of_a_reference() {
    // The property that makes this a port rather than a lookalike. Without the
    // comparison a test could pass on a marker arm that *invented* an answer
    // from the declaration's kind rather than finding the export symbol; the
    // unexported column is answered by the ordinary dispatch and cannot.
    for (exported, plain) in [
        (
            "namespace N { export var x: string; export var q = x; }",
            "namespace N { var x: string; export var q = x; }",
        ),
        (
            "namespace N { export function f() {} export var q = f; }",
            "namespace N { function f() {} export var q = f; }",
        ),
        (
            "namespace N { export class C {} export var q = C; }",
            "namespace N { class C {} export var q = C; }",
        ),
    ] {
        assert_eq!(
            type_inside_namespace(exported),
            type_inside_namespace(plain),
            "exporting changed the type"
        );
    }
}

#[test]
fn a_namespace_marker_whose_own_type_is_unported_is_still_a_gap() {
    // The link must inherit the export symbol's answer, gap included, rather
    // than manufacturing one on the way through. The example was a tuple
    // annotation until the plain form was ported
    // (`docs/architecture/checker-notes-tuple.md`); `keyof` is unported and
    // carries the test now, with the tuple kept beside it as the positive
    // control so this measures inheritance in *both* directions.
    assert_eq!(
        type_inside_namespace("namespace N { export var t: keyof string; export var q = t; }"),
        "error"
    );
    assert_eq!(
        type_inside_namespace("namespace N { export var t: [number, string]; export var q = t; }"),
        "[number, string]"
    );
}
