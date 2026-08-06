//! References to exported names inside the module that declares them.
//!
//! When a declaration is exported, the binder declares it twice: the real symbol
//! into the module's `exports`, and a **marker** into the file's `locals`
//! carrying `SymbolFlags::EXPORT_VALUE` and nothing else (`binder.rs:3086`).
//! That is upstream's design. A reference inside the module resolves to the
//! marker, whose flags carry no `VARIABLE`/`FUNCTION`/`CLASS` bit, so every arm
//! of `get_type_of_symbol` missed it and the answer was `errorType`.
//!
//! Measured at 3,144 lines across 463 cases before this landed — not one file
//! and not one shape, just every reference to every exported name.

use tsr_ast::Statement;
use tsr_checker::Checker;
use tsr_core::Arena;

/// Type the initialiser of the last statement.
fn type_of_last(source: &str) -> String {
    let arena = Arena::new();
    let parsed = tsr_parser::parse(&arena, source);
    assert!(
        parsed.diagnostics.is_empty(),
        "fixture must parse: {:?}",
        parsed.diagnostics.iter().map(tsr_diagnostics::Diagnostic::text).collect::<Vec<_>>()
    );
    let bound = tsr_binder::bind(
        parsed.source_file,
        &parsed.nodes,
        tsr_binder::FileInfo { name: "test.ts", text: source },
    );
    let mut checker = Checker::new(&bound, &parsed.nodes, &parsed.node_map);
    let index = parsed.source_file.statements.len() - 1;
    let Statement::VariableStatement(statement) = parsed.source_file.statements[index] else {
        panic!("the last statement must be a variable statement");
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
fn a_reference_to_an_exported_var_has_the_variables_type() {
    assert_eq!(type_of_last("export var x = 1;\nvar q = x;"), "number");
    assert_eq!(type_of_last("export const s: string = \"a\";\nvar q = s;"), "string");
}

#[test]
fn a_reference_to_an_exported_function_has_its_signature() {
    assert_eq!(type_of_last("export function f() {}\nvar q = f;"), "() => void");
}

#[test]
fn a_reference_to_an_exported_class_is_the_static_side() {
    // `typeof C`, not `C` — the value reference is the constructor, and a static
    // member access through it must resolve. This is the case that forced the
    // arm to reconstruct upstream's `ExportSymbol` link rather than re-derive a
    // type from the declaration's kind: a marker carries no `CLASS` flag, so
    // nothing local to it can know a class reference means the static side.
    assert_eq!(type_of_last("export class C { static s: string; }\nvar q = C;"), "typeof C");
    assert_eq!(type_of_last("export class C { static s: string; }\nvar q = C.s;"), "string");
}

#[test]
fn the_marker_answers_the_same_as_the_unexported_declaration() {
    // The property that makes this a port rather than a lookalike: exporting a
    // declaration must not change the type of a reference to it.
    for (exported, plain) in [
        ("export var x = 1;\nvar q = x;", "var x = 1;\nvar q = x;"),
        ("export function f() {}\nvar q = f;", "function f() {}\nvar q = f;"),
        ("export class C {}\nvar q = C;", "class C {}\nvar q = C;"),
    ] {
        assert_eq!(type_of_last(exported), type_of_last(plain), "exporting changed the type");
    }
}

#[test]
fn an_exported_declaration_whose_own_type_is_unported_is_still_a_gap() {
    // The marker inherits the export symbol's answer, gap included — it must not
    // manufacture one. The example was a tuple annotation until the plain form
    // was ported (`docs/architecture/checker-notes-tuple.md`); `keyof` is
    // unported and carries the test now. The positive control sits beside it,
    // so this measures *inheritance* rather than "everything exported gaps".
    assert_eq!(type_of_last("export var t: keyof string;\nvar q = t;"), "error");
    assert_eq!(type_of_last("export var t: [number, string];\nvar q = t;"), "[number, string]");
}

#[test]
fn a_non_module_file_is_unaffected() {
    // No `export` anywhere means no markers, so this exercises the path the arm
    // must not disturb.
    assert_eq!(type_of_last("var x = 1;\nvar q = x;"), "number");
    assert_eq!(type_of_last("class C {}\nvar q = C;"), "typeof C");
}
