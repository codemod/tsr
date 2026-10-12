//! reportNonexistentProperty's union arm (pinned checker.go:11545-11552):
//! the first constituent lacking the name explains the message. Texts are
//! pinned tsgo 5b1047d10d32e7d5b446be4de56b126ff42f82bb on the same source.

use tsr_checker::{Checker, check::FileContext};
use tsr_core::Arena;
use tsr_diagnostics::format::write_flattened_diagnostic_message;

fn messages(source: &str) -> Vec<(u32, String)> {
    let arena = Arena::new();
    let parsed = tsr_parser::parse(&arena, source);
    assert!(parsed.diagnostics.is_empty());
    let root = parsed.source_file.node_id().unwrap();
    let bound = tsr_binder::bind(
        &arena,
        parsed.source_file,
        &parsed.nodes,
        tsr_binder::FileInfo { name: "a.ts", text: source },
    );
    let mut checker = Checker::new(&bound, &parsed.nodes, &parsed.node_map);
    checker.check_source_file(root, FileContext { ambient: false, has_parse_errors: false });
    checker
        .diagnostics()
        .iter()
        .map(|(_, diagnostic)| {
            let mut text = String::new();
            write_flattened_diagnostic_message(&mut text, diagnostic, "\n");
            (diagnostic.message.code(), text)
        })
        .collect()
}

#[test]
fn union_miss_chains_the_first_constituent_lacking_the_name() {
    let source = "interface A { a: number; notInB: string }\n\
                  interface B { a: number; notInC: string }\n\
                  declare const ab: A | B;\n\
                  ab.zzz;\n\
                  ab.notInB;\n";
    assert_eq!(
        messages(source),
        [
            (
                2339,
                "Property 'zzz' does not exist on type 'A | B'.\n  Property 'zzz' does not exist on type 'A'.".into()
            ),
            (
                2339,
                "Property 'notInB' does not exist on type 'A | B'.\n  Property 'notInB' does not exist on type 'B'.".into()
            ),
        ]
    );
}
