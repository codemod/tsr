//! `Symbol::export_symbol` — the marker-to-export link.
//!
//! Ported from `ast.Symbol.ExportSymbol` (`internal/ast/symbol.go:20`), set in
//! `declareModuleMember` (`internal/binder/binder.go:407`).
//!
//! The link is the binder's because the binder is the only thing that knows
//! *which container's* `exports` the real symbol went into. Everything
//! downstream that tries to re-derive it has to guess, and the guess this port
//! shipped — "walk to the source file" — is wrong for a namespace member and for
//! any declaration in a script file. See
//! `docs/architecture/checker-notes-nameres.md`.

use tsr_ast::{Node, NodeTable};
use tsr_binder::{BindResult, SymbolId};
use tsr_core::Arena;
use tsr_parser::ParsedSourceFile;

struct Bound<'a> {
    parsed: ParsedSourceFile<'a>,
    result: BindResult<'a>,
}

fn bind<'a>(arena: &'a Arena, source: &'a str) -> Bound<'a> {
    let parsed = tsr_parser::parse(arena, source);
    assert!(
        parsed.diagnostics.is_empty(),
        "the fixture should parse cleanly: {:?}",
        parsed.diagnostics.iter().map(tsr_diagnostics::Diagnostic::text).collect::<Vec<_>>()
    );
    let result = tsr_binder::bind(
        arena,
        parsed.source_file,
        &parsed.nodes,
        tsr_binder::FileInfo { name: "test.ts", text: source },
    );
    Bound { parsed, result }
}

impl Bound<'_> {
    fn nodes(&self) -> &NodeTable {
        &self.parsed.nodes
    }

    fn root(&self) -> tsr_ast::NodeId {
        Node::SourceFile(self.parsed.source_file).node_id().expect("registered")
    }

    /// The symbol a name resolves to from the file's own scope — which, for an
    /// exported declaration, is the **marker** and not the export symbol.
    fn local(&self, name: &str) -> SymbolId {
        self.result.lookup_local(self.root(), name).unwrap_or_else(|| panic!("no local `{name}`"))
    }
}

#[test]
fn a_namespace_member_marker_links_to_the_namespaces_export_and_not_the_files() {
    // The case the file-walking reconstruction cannot reach: the export symbol
    // is on `N`, and the file has no `exports` entry for `E` at all. 2,175 corpus
    // assertion lines are this shape.
    let arena = Arena::new();
    let source = "namespace N { export enum E { A } }\n";
    let bound = bind(&arena, source);

    let namespace = bound.local("N");
    let export = *bound.result.symbols().get(namespace).exports.get("E").expect("`N` exports `E`");

    // The marker lives in the namespace's own `locals`, so it is reached by
    // walking out of the declaration until a scope holds the name — never from
    // the file, which is the whole point of this test.
    let mut scope = bound.nodes().parent(bound.result.symbols().get(export).declarations[0]);
    let marker = loop {
        let Some(node) = scope else { panic!("no marker for `E` anywhere above the enum") };
        assert!(
            node != bound.root(),
            "the marker was found on the FILE, which is the bug this pins"
        );
        if let Some(found) = bound.result.lookup_local(node, "E") {
            break found;
        }
        scope = bound.nodes().parent(node);
    };

    assert_ne!(marker, export, "the marker and the export must be two symbols");
    assert_eq!(
        bound.result.symbols().get(marker).export_symbol,
        Some(export),
        "the marker must link to the namespace's export symbol"
    );
    assert_eq!(
        bound.result.symbols().get(namespace).exports.get("E").copied(),
        Some(export),
        "and that symbol must be the one in `N`'s exports, not the file's"
    );
}

#[test]
fn a_declaration_that_is_not_exported_has_no_export_symbol() {
    // The mirror. `export_symbol` is `Some` only where the binder actually made
    // two symbols; a link on every local would make the checker's marker arm
    // fire for declarations that were never exported.
    let arena = Arena::new();
    let source = "namespace N { enum E { A } }\nvar v = 1;\n";
    let bound = bind(&arena, source);

    assert_eq!(
        bound.result.symbols().get(bound.local("v")).export_symbol,
        None,
        "a plain `var` is not an export marker"
    );

    let namespace = bound.local("N");
    assert!(
        !bound.result.symbols().get(namespace).exports.contains_key("E"),
        "`enum E` without `export` must not be in `N`'s exports"
    );
    assert_eq!(
        bound.result.symbols().get(namespace).export_symbol,
        None,
        "and the namespace's own symbol is not a marker either"
    );
}
