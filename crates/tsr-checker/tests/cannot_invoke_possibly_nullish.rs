//! TS2721–TS2723 — `reportCannotInvokePossiblyNullOrUndefinedError`
//! (`checker.go:9894`), the reporter `resolveCallExpression` hands to
//! `checkNonNullTypeWithReporter` (`checker.go:8511`). Expectations were
//! checked against a native `tsgo` built from the pinned submodule.

use tsr_ast::NodeId;
use tsr_checker::Checker;
use tsr_checker::check::FileContext;
use tsr_core::Arena;

fn codes(source: &str) -> Vec<String> {
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
    let bound = tsr_binder::bind_into(
        tsr_binder::BindResult::empty(),
        &arena,
        file.source_file,
        &nodes,
        tsr_binder::FileInfo { name: "a.ts", text: source },
    );
    let mut checker = Checker::new(&bound, &nodes, &node_map);
    checker.set_strict_null_checks(true);
    checker.check_source_file(root, FileContext { ambient: false, has_parse_errors: false });
    let mut codes: Vec<String> = checker
        .diagnostics()
        .iter()
        .map(|(_, d)| d.code().clone())
        .filter(|code| matches!(code.as_str(), "TS2721" | "TS2722" | "TS2723"))
        .collect();
    codes.sort();
    codes
}

#[test]
fn a_possibly_undefined_callee_is_ts2722() {
    assert_eq!(codes("declare const f: (() => void) | undefined;\nf();"), ["TS2722"]);
}

#[test]
fn a_possibly_null_callee_is_ts2721() {
    assert_eq!(codes("declare const f: (() => void) | null;\nf();"), ["TS2721"]);
}

#[test]
fn a_possibly_null_or_undefined_callee_is_ts2723() {
    assert_eq!(codes("declare const f: (() => void) | null | undefined;\nf();"), ["TS2723"]);
}

#[test]
fn a_narrowed_callee_does_not_report() {
    assert!(codes("declare const f: (() => void) | undefined;\nif (f) f();").is_empty());
}

#[test]
fn a_super_property_narrowed_by_its_guard_does_not_report() {
    // `isMatchingReference`'s `super` arm (`flow.go:1617`): the guard and
    // the call are the same reference (`controlFlowSuperPropertyAccess`).
    // Needs `r5-smallcodes3-flow-super-meta-references.diff`.
    assert!(
        codes("class B { m?(): void }\nclass C extends B { f() { super.m && super.m(); } }")
            .is_empty()
    );
}
