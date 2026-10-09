//! TS2503 / TS2833 — `onFailedToResolveSymbol` with `SymbolFlagsNamespace`
//! (`checker.go:1564`), from a qualified type name's left identifier and from
//! `import a = b` (`getSymbolOfPartOfRightHandSideOfImportEquals`'s case 1,
//! `checker.go:5020`). Expectations were checked against a native `tsgo`
//! built from the pinned submodule. Needs
//! `docs/parity/notes/r6-smallcodes4-namespace-not-found.diff`.

use tsr_ast::NodeId;
use tsr_checker::Checker;
use tsr_checker::check::FileContext;
use tsr_core::Arena;

fn reports(source: &str) -> Vec<String> {
    let arena = Arena::new();
    let mut nodes = tsr_ast::NodeTable::default();
    let mut node_map = tsr_ast::NodeMap::default();
    let file = tsr_parser::parse_into(
        &arena,
        source,
        tsr_parser::ParseOptions::for_file("a.ts"),
        &mut nodes,
        &mut node_map,
    );
    let root: NodeId = file.source_file.node_id.expect("a parsed file has an id");
    let has_parse_errors = !file.diagnostics.is_empty();
    let bound = tsr_binder::bind_into(
        tsr_binder::BindResult::empty(),
        &arena,
        file.source_file,
        &nodes,
        tsr_binder::FileInfo { name: "a.ts", text: source },
    );
    let mut checker = Checker::new(&bound, &nodes, &node_map);
    checker.check_source_file(root, FileContext { ambient: false, has_parse_errors });
    let mut reports: Vec<String> = checker
        .diagnostics()
        .iter()
        .filter(|(_, d)| matches!(d.code().to_string().as_str(), "TS2503" | "TS2833"))
        .map(|(_, d)| format!("{}: {}", d.code(), d.text()))
        .collect();
    reports.sort();
    reports
}

#[test]
fn an_unresolved_import_equals_identifier_is_ts2503() {
    assert_eq!(reports("import d = asdf;"), ["TS2503: Cannot find namespace 'asdf'."]);
}

#[test]
fn an_import_equals_of_a_namespace_or_globalthis_does_not_report() {
    assert!(
        reports("namespace N { export var x = 1; }\nimport a = N;\nimport g = globalThis;")
            .is_empty()
    );
}

#[test]
fn a_missing_import_equals_identifier_does_not_report() {
    // `import abstract class D {}`: the module reference is the parser's
    // missing identifier, which `resolveEntityName` never resolves.
    assert!(reports("import abstract class D {}").is_empty());
}

#[test]
fn a_value_used_as_a_namespace_is_ts2503() {
    assert_eq!(
        reports("function f1() { let intrinsic: intrinsic.intrinsic; }"),
        ["TS2503: Cannot find namespace 'intrinsic'."]
    );
}

#[test]
fn an_enum_or_globalthis_left_side_does_not_report() {
    assert!(reports("enum E { A }\nlet e: E.A;\ntype F = globalThis.Function;").is_empty());
}

#[test]
fn a_type_used_as_a_namespace_does_not_report_ts2503() {
    // `checkAndReportErrorForUsingTypeAsNamespace` answers instead (TS2702,
    // not ported here).
    assert!(reports("interface I { a: number }\nlet i: I.a;").is_empty());
}

#[test]
fn a_near_miss_namespace_is_ts2833() {
    assert_eq!(
        reports("namespace Outer { export interface T {} }\nlet o: Outr.T;"),
        ["TS2833: Cannot find namespace 'Outr'. Did you mean 'Outer'?"]
    );
}
