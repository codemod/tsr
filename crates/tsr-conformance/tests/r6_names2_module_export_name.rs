//! r6-names2: `checkModuleExportName(name, allowStringLiteral=false)`
//! (`checker.go:5388`), TS1003 on a string property name in an export
//! specifier without a module specifier (`crates/tsr-checker/src/module_format.rs`).
//! Expectations are native tsgo output at the pinned 5b1047d.
//! `docs/parity/notes/r6-names2.md` §3.

use tsr_conformance::{TestCase, diagnostics_suite};

fn diagnostics(file: &str, source: &str, module: &str) -> Vec<(u32, u32, u32)> {
    let mut case = TestCase::parse("probe/r6-names2", file, source);
    case.options.insert("module".into(), module.into());
    case.options.insert("target".into(), "es2022".into());
    let mut diagnostics: Vec<_> = diagnostics_suite::reported_for(&case)
        .into_iter()
        .map(|diagnostic| (diagnostic.line, diagnostic.column, diagnostic.code))
        .collect();
    diagnostics.sort_unstable();
    diagnostics
}

const LOCAL: &str = "const s = 1;
export { \"s\" as x };
export { \"t\" };
export { s as \"y\" };";

/// Only the property name of a local specifier is disallowed; an exported
/// string name is fine (TS18057 only under es2015/es2020).
#[test]
fn a_local_string_property_name_is_identifier_expected() {
    assert_eq!(diagnostics("a.ts", LOCAL, "commonjs"), [(2, 10, 1003)]);
    assert_eq!(
        diagnostics("a.ts", LOCAL, "es2015"),
        [(2, 10, 1003), (3, 10, 18057), (4, 15, 18057)]
    );
}

/// Inside a namespace too, beside TS1194; and in a declaration file.
#[test]
fn a_namespace_and_a_declaration_file_report_it_too() {
    let namespace = "namespace N { export { \"u\" as v } }\nexport {};";
    assert_eq!(diagnostics("b.ts", namespace, "commonjs"), [(1, 15, 1194), (1, 24, 1003)]);
    let declaration = "declare const s: number;\nexport { \"s\" as x };";
    assert_eq!(diagnostics("c.d.ts", declaration, "commonjs"), [(2, 10, 1003)]);
}

/// `grammarErrorOnNode`: a file with a parse error reports only that.
#[test]
fn a_file_with_a_parse_error_reports_nothing_more() {
    let source = "const s = 1;\nexport { \"s\" as x };\nvar v = ;";
    assert_eq!(diagnostics("d.ts", source, "commonjs"), [(3, 9, 1109)]);
}
