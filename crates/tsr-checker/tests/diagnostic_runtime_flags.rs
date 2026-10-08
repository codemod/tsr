//! Native `NewDiagnostic` copies the catalogue's unnecessary/deprecated metadata.
use tsr_checker::{Checker, check::FileContext};
use tsr_core::Arena;

#[test]
fn comma_operator_publishes_native_unnecessary_flag() {
    let arena = Arena::new();
    let source = "const discarded = (1, 2);";
    let parsed = tsr_parser::parse(&arena, source);
    let bound = tsr_binder::bind(
        &arena,
        parsed.source_file,
        &parsed.nodes,
        tsr_binder::FileInfo { name: "control.ts", text: source },
    );
    let mut checker = Checker::new(&bound, &parsed.nodes, &parsed.node_map);
    checker.check_source_file(
        parsed.source_file.node_id.unwrap(),
        FileContext { ambient: false, has_parse_errors: false },
    );
    let flags: Vec<_> = checker
        .diagnostics()
        .iter()
        .filter(|(_, diagnostic)| diagnostic.message.code() == 2695)
        .map(|(_, diagnostic)| (diagnostic.reports_unnecessary(), diagnostic.reports_deprecated()))
        .collect();
    assert_eq!(flags, [(true, false)]);
}
