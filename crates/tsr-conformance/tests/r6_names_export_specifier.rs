//! r6-names: a local `export { X }` that resolves nowhere runs
//! `onFailedToResolveSymbol` at `Value|Type|Namespace`
//! (`crates/tsr-checker/src/export_specifier_names.rs`). Expectations are
//! native tsgo output at the pinned 5b1047d (`--module commonjs`).

use tsr_conformance::{TestCase, diagnostics_suite};

fn diagnostics(file: &str, source: &str) -> Vec<(u32, u32, u32)> {
    let mut case = TestCase::parse("probe/r6-names", file, source);
    case.options.insert("target".into(), "es2015".into());
    case.options.insert("module".into(), "commonjs".into());
    let mut diagnostics: Vec<_> = diagnostics_suite::reported_for(&case)
        .into_iter()
        .map(|diagnostic| (diagnostic.line, diagnostic.column, diagnostic.code))
        .collect();
    diagnostics.sort_unstable();
    diagnostics
}

#[test]
fn an_unresolved_local_export_specifier_is_reported_with_a_suggestion() {
    let source = "type RoomInterfae = {};
export type { RoomInterface };
export { Missing };
const valuu = 1;
export { value as renamed };
export { string };
declare namespace D { export { amb }; }";
    assert_eq!(
        diagnostics("m.ts", source),
        [(2, 15, 2552), (3, 10, 2304), (5, 10, 2552), (6, 10, 2661), (7, 32, 2304)],
    );
}
