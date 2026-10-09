//! `import a = b` where `b` is itself an alias of a type-only namespace
//! (`getSymbolOfPartOfRightHandSideOfImportEquals`, `checker.go:14486`):
//! `getSymbol` accepts the alias by its target's flags, so `a` resolves and a
//! type reference through it reads the namespace's members
//! (`docs/parity/notes/r5-errorsplit6.md` §4.1).

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
    checker.type_to_string(id)
}

#[test]
fn an_alias_of_an_alias_of_a_type_only_namespace_resolves() {
    let source = "namespace N { export interface I { a: number } }\n\
                  import Y = N;\nimport X = Y;\n\
                  declare let v: X.I;\nconst a = v.a;\n";
    assert_eq!(type_of_declaration(source, "a"), "number");
}

/// The control: one hop, which the arm before this one already resolved.
#[test]
fn a_direct_alias_of_a_type_only_namespace_resolves() {
    let source = "namespace N { export interface I { a: number } }\n\
                  import Y = N;\n\
                  declare let v: Y.I;\nconst a = v.a;\n";
    assert_eq!(type_of_declaration(source, "a"), "number");
}
