//! `checkLabeledStatement`'s unused-label arm (`checker/checker.go:4219`),
//! at the pinned 5b1047d. Each expectation was checked against native `tsgo`
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

/// A label no `break` names is TS7028 under `allowUnusedLabels: false`, and
/// only then (unset is a suggestion).
#[test]
fn an_unused_label_is_ts7028_only_when_disallowed() {
    let source = "function f() {\n  label1: for (;;) {}\n  outer: for (;;) { break outer; }\n}\n";
    let unused = |allow| {
        diagnostics(source, false, allow)
            .into_iter()
            .filter(|(code, _)| *code == 7028)
            .map(|(_, text)| text)
            .collect::<Vec<_>>()
    };
    assert_eq!(unused(Tristate::False), ["label1"]);
    assert!(unused(Tristate::Unknown).is_empty());
    assert!(unused(Tristate::True).is_empty());
}
