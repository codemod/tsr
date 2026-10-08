//! `checkSwitchStatement`'s case arm (`checker.go:4188`): a fresh object
//! literal case that the switch type is not comparable to fails
//! `checkTypeComparableTo` in `hasExcessProperties` (`relater.go:2714`),
//! TS2353 at the member. Expected texts are native tsgo's (vendor `5b1047d`).

use tsr_checker::{Checker, check::FileContext};
use tsr_core::Arena;

fn diagnostics(source: &str) -> Vec<(u32, String)> {
    let arena = Arena::new();
    let parsed = tsr_parser::parse(&arena, source);
    assert!(parsed.diagnostics.is_empty());
    let root = parsed.source_file.node_id.unwrap();
    let bound = tsr_binder::bind(
        &arena,
        parsed.source_file,
        &parsed.nodes,
        tsr_binder::FileInfo { name: "a.ts", text: source },
    );
    let mut checker = Checker::new(&bound, &parsed.nodes, &parsed.node_map);
    checker.check_source_file(root, FileContext { ambient: false, has_parse_errors: false });
    checker.diagnostics().iter().map(|(_, d)| (d.message.code(), d.text())).collect()
}

#[test]
fn a_fresh_literal_case_reports_its_excess_member() {
    let actual = diagnostics(
        "class C { id!: number; }\n\
         switch (new C()) {\n\
             case { id: 12, name: '' }:\n\
             case { id: 12 }:\n\
         }\n",
    );
    assert_eq!(actual, [(
        2353,
        "Object literal may only specify known properties, and 'name' does not exist in type 'C'."
            .to_string()
    )]);
}
