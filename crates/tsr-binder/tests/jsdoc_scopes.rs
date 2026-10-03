//! Locals created outside the ordinary container classification.

use tsr_ast::{NodeFlags, NodeId, SyntaxKind};
use tsr_binder::{BindResult, FileInfo, SymbolFlags};
use tsr_core::Arena;

#[test]
fn typedef_templates_resolve_in_their_own_comment_scope() {
    let source = "/** @template T\n * @typedef {Object} First\n * @property {T} value\n */\nlet first;\n\
                  /** @template T\n * @typedef {Object} Second\n * @property {T} value\n */\nlet second;\n\
                  /** @type {T} */\nlet outside;";
    let arena = Arena::new();
    let mut parsed = tsr_parser::parse_with_options(
        &arena,
        source,
        tsr_parser::ParseOptions::for_file("scopes.js"),
    );
    assert!(parsed.diagnostics.is_empty());
    let root = parsed.source_file.node_id.expect("registered source file");
    parsed.nodes.add_flags(root, NodeFlags::JAVASCRIPT_FILE);
    let docs: Vec<_> = parsed.jsdoc.iter().collect();
    let bound = tsr_binder::bind_into_with_jsdoc(
        BindResult::empty(),
        &arena,
        parsed.source_file,
        &parsed.nodes,
        FileInfo { name: "scopes.js", text: source },
        &docs,
    );
    let references: Vec<_> = source
        .match_indices("{T}")
        .map(|(offset, _)| {
            (0..parsed.nodes.len())
                .map(|index| NodeId::new(u32::try_from(index).expect("small fixture")))
                .find(|&node| {
                    parsed.nodes.kind(node) == SyntaxKind::Identifier
                        && usize::try_from(parsed.nodes.span(node).start).expect("fits")
                            == offset + 1
                })
                .expect("registered JSDoc reference")
        })
        .map(|node| {
            bound.resolve_name(&parsed.nodes, &parsed.node_map, node, "T", SymbolFlags::TYPE)
        })
        .collect();
    let first = references[0].expect("first template visible in its typedef");
    let second = references[1].expect("second template visible in its typedef");
    assert_ne!(first, second, "same spelling does not merge comment scopes");
    assert!(bound.symbols().get(first).flags.contains(SymbolFlags::TYPE_PARAMETER));
    assert!(bound.symbols().get(second).flags.contains(SymbolFlags::TYPE_PARAMETER));
    assert_eq!(references[2], None, "typedef templates do not escape their comments");
    assert_eq!(bound.lookup_local(root, "T"), None);
}
