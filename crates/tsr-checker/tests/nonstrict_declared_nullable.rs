//! `getWidenedType` under `strictNullChecks: false`, pinned to 5b1047d1:
//! only `createWideningType`'s `null`/`undefined` twins widen to `any`
//! (`checker.go:25027`, `:16090`), so a declared `undefined` keeps its type
//! through a variable's initializer. `docs/parity/notes/r6-jsdoc.md` §9.
use tsr_ast::{Node, NodeId, SyntaxKind};
use tsr_binder::BindResult;
use tsr_checker::Checker;
use tsr_core::Arena;

fn variable_types(source: &str) -> Vec<String> {
    let arena = Arena::new();
    let parsed =
        tsr_parser::parse_with_options(&arena, source, tsr_parser::ParseOptions::for_file("t.ts"));
    assert!(parsed.diagnostics.is_empty(), "fixture must parse");
    let bound = tsr_binder::bind_into(
        BindResult::empty(),
        &arena,
        parsed.source_file,
        &parsed.nodes,
        tsr_binder::FileInfo { name: "t.ts", text: source },
    );
    let mut checker = Checker::new(&bound, &parsed.nodes, &parsed.node_map);
    checker.set_strict_null_checks(false);
    let mut out = Vec::new();
    for index in 0..parsed.nodes.len() {
        let id = NodeId::new(u32::try_from(index).unwrap());
        if parsed.nodes.kind(id) != SyntaxKind::VariableDeclaration {
            continue;
        }
        let Some(Node::VariableDeclaration(_)) = parsed.node_map.get(id) else { continue };
        let symbol = bound.symbol_of(id).expect("bound variable");
        let ty = checker.get_type_of_symbol(symbol);
        out.push(checker.type_to_string(ty));
    }
    out
}

/// `typeFromJSInitializer3`'s shape: the declared `undefined` and `null`
/// survive; the literals widen.
#[test]
fn only_widening_nullables_widen() {
    let source = "declare const u: undefined;\ndeclare const n: null;\nconst a = u;\nconst b = n;\nconst c = undefined;\nconst d = null;\n";
    assert_eq!(variable_types(source), ["undefined", "null", "undefined", "null", "any", "any"]);
}
