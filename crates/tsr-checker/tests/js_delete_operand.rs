//! `checkDeleteExpression` (`checker/checker.go:10808`) in a checked JS
//! file, at the pinned 5b1047d. Each expectation was checked against native `tsgo`
//! (`docs/parity/notes/r6-jsdoc2.md` §2.2).
use tsr_ast::Node;
use tsr_binder::BindResult;
use tsr_checker::{Checker, check::FileContext};
use tsr_core::{Arena, CompilerOptions, Tristate};

/// Every diagnostic of a checked file, as `(code, text at its span)`.
fn diagnostics(
    source: &str,
    javascript: bool,
    allow_unused_labels: Tristate,
) -> Vec<(u32, String)> {
    let name = if javascript { "t.js" } else { "t.ts" };
    let arena = Arena::new();
    let mut parsed =
        tsr_parser::parse_with_options(&arena, source, tsr_parser::ParseOptions::for_file(name));
    let root = Node::SourceFile(parsed.source_file).node_id().unwrap();
    if javascript {
        parsed.nodes.add_flags(root, tsr_ast::NodeFlags::JAVASCRIPT_FILE);
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
    let options = CompilerOptions { allow_unused_labels, ..CompilerOptions::default() };
    let mut checker = Checker::new(&bound, &parsed.nodes, &parsed.node_map);
    checker.apply_compiler_options(&options);
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

/// `delete (o)` is TS2703 in a checked JS file as in TypeScript: the operand
/// skips parentheses only, and `o` is not an access.
#[test]
fn delete_of_a_non_access_is_ts2703_in_js() {
    let reported = diagnostics("var o = { p: 1 };\ndelete (o);\n", true, Tristate::Unknown);
    assert!(reported.iter().any(|(code, text)| *code == 2703 && text == "o"));
}
