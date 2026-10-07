//! Missing declarations still publish native addDeclarationToSymbol metadata.
use tsr_ast::Node;
use tsr_binder::SymbolFlags;
use tsr_core::Arena;

#[test]
fn missing_alias_name_retains_alias_meaning_without_entering_locals() {
    let arena = Arena::new();
    let source = "import * as missing from './module';";
    let parsed = tsr_parser::parse(&arena, source);
    let tsr_ast::Statement::ImportDeclaration(import) = parsed.source_file.statements[0] else {
        panic!("import declaration")
    };
    let clause = import.import_clause.expect("import clause");
    let tsr_ast::NamedImportBindings::NamespaceImport(namespace) =
        clause.named_bindings.expect("namespace import")
    else {
        panic!("namespace import")
    };
    let declaration = namespace.node_id.expect("registered namespace import");
    // Construct native's absent name rather than a parser's empty identifier.
    let namespace =
        arena.alloc(tsr_ast::NamespaceImport { node_id: namespace.node_id, name: None });
    let clause = arena.alloc(tsr_ast::ImportClause {
        node_id: clause.node_id,
        phase_modifier: clause.phase_modifier,
        name: clause.name,
        named_bindings: Some(tsr_ast::NamedImportBindings::NamespaceImport(namespace)),
    });
    let import = arena.alloc(tsr_ast::ImportDeclaration {
        node_id: import.node_id,
        modifiers: import.modifiers,
        import_clause: Some(clause),
        module_specifier: import.module_specifier,
        attributes: import.attributes,
    });
    let file = tsr_ast::SourceFile {
        node_id: parsed.source_file.node_id,
        statements: arena.alloc_slice(&[tsr_ast::Statement::ImportDeclaration(import)]),
        end_of_file_token: parsed.source_file.end_of_file_token,
    };
    let bound = tsr_binder::bind(
        &arena,
        arena.alloc(file),
        &parsed.nodes,
        tsr_binder::FileInfo { name: "test.ts", text: source },
    );
    let symbol = bound.symbol_of(declaration).expect("missing-name symbol");
    let symbol = bound.symbols().get(symbol);
    assert!(symbol.flags.contains(SymbolFlags::ALIAS));
    assert_eq!(symbol.declarations.as_slice(), &[declaration]);
    let root = Node::SourceFile(parsed.source_file).node_id().unwrap();
    assert!(bound.lookup_local(root, "__missing").is_none());
}
