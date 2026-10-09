//! The arity rule counts a `@typedef`'s `@template` parameters, pinned to
//! 5b1047d1: the reparsed `JSTypeAliasDeclaration` carries
//! `gatherTypeParameters(jsDoc, true)` (`parser/reparser.go:293`), so
//! `getTypeFromTypeAliasReference` reports TS2314 for too few arguments.
//! `docs/parity/notes/r5-jsdoc5.md` §4.5.
use tsr_ast::Node;
use tsr_binder::BindResult;
use tsr_checker::{Checker, check::FileContext};
use tsr_core::Arena;

/// Every diagnostic of a checked file, as `(code, text at its span)`.
fn diagnostics(source: &str, javascript: bool) -> Vec<(u32, String)> {
    let name = if javascript { "t.js" } else { "t.ts" };
    let arena = Arena::new();
    let mut parsed =
        tsr_parser::parse_with_options(&arena, source, tsr_parser::ParseOptions::for_file(name));
    assert!(parsed.diagnostics.is_empty(), "fixture must parse");
    let root = Node::SourceFile(parsed.source_file).node_id().unwrap();
    // As the loader stamps a JS file: the root and every comment's root
    // (`crates/tsr-compiler/src/loader.rs`, `r5-jsdoc3.md` §3).
    if javascript {
        parsed.nodes.add_flags(root, tsr_ast::NodeFlags::JAVASCRIPT_FILE);
        for (_, docs) in parsed.jsdoc.iter() {
            for doc in docs {
                if let Some(id) = doc.node_id {
                    parsed.nodes.add_flags(id, tsr_ast::NodeFlags::JAVASCRIPT_FILE);
                }
            }
        }
    }
    let jsdoc: Vec<_> = parsed.jsdoc.iter().collect();
    let bound = tsr_binder::bind_into_with_jsdoc(
        BindResult::empty(),
        &arena,
        parsed.source_file,
        &parsed.nodes,
        tsr_binder::FileInfo { name, text: source },
        &jsdoc,
    );
    let mut checker = Checker::new(&bound, &parsed.nodes, &parsed.node_map);
    checker.set_jsdoc(parsed.jsdoc.iter());
    checker.check_source_file(root, FileContext { ambient: false, has_parse_errors: false });
    checker
        .diagnostics()
        .iter()
        .map(|(_, d)| {
            let text = &source[d.span.start as usize..d.span.end as usize];
            (d.message.code(), text.to_string())
        })
        .collect()
}

const PAIR: &str = "/**\n * @template T\n * @template U\n * @typedef {{ t: T, u: U }} Pair\n */\n";

/// `typedefMultipleTypeParameters`: one argument for two parameters.
#[test]
fn too_few_arguments_to_a_typedef_is_ts2314() {
    let source = format!("{PAIR}/** @type {{Pair<number>}} */\nvar p;\n");
    assert!(
        diagnostics(&source, true)
            .iter()
            .any(|(code, text)| *code == 2314 && text == "Pair<number>")
    );
}

/// Both arguments: nothing to report.
#[test]
fn a_full_typedef_reference_is_allowed() {
    let source = format!("{PAIR}/** @type {{Pair<number, string>}} */\nvar p;\n");
    assert!(!diagnostics(&source, true).iter().any(|(code, _)| *code == 2314));
}
