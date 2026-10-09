//! The unqualified, type-meaning import type node resolves through
//! `export =` (`getTypeFromImportTypeNode`, `checker.go:24575`). Exercised
//! end to end by `compiler/declarationImportTypeAliasInferredAndEmittable`
//! (multi-file, so not reproducible on a single bound file); this pins the
//! single-file declines. `docs/parity/notes/r6-typesroots.md` §8.

use tsr_checker::Checker;
use tsr_core::Arena;

fn type_of_declaration(source: &str, name: &str) -> String {
    let arena = Arena::new();
    let parsed = tsr_parser::parse(&arena, source);
    let bound = tsr_binder::bind(
        &arena,
        parsed.source_file,
        &parsed.nodes,
        tsr_binder::FileInfo { name: "test.ts", text: source },
    );
    let root = tsr_ast::Node::SourceFile(parsed.source_file).node_id().expect("registered");
    let symbol = bound.lookup_local(root, name).unwrap_or_else(|| panic!("`{name}` is declared"));
    let mut checker = Checker::new(&bound, &parsed.nodes, &parsed.node_map);
    let id = checker.get_type_of_symbol(symbol);
    let site = bound.symbols().get(symbol).declarations[0];
    checker.type_to_string_at(id, site).expect("nameable type")
}

/// With no module host the specifier resolves nowhere: the gap, as before.
#[test]
fn an_unresolvable_import_type_keeps_the_gap() {
    assert_eq!(type_of_declaration("declare var x: import(\"./nowhere\");", "x"), "error");
}

/// A local class prints by name: the instance-side import spelling is only
/// for a class no chain names.
#[test]
fn a_nameable_class_instance_prints_its_name() {
    assert_eq!(type_of_declaration("class C { }\ndeclare var x: C;", "x"), "C");
}
