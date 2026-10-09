//! TS2492 — `checkCatchClause`'s last arm (`checker.go:4259`), pinned to
//! typescript-go 5b1047d1: a block-scoped redeclaration of an unannotated
//! catch variable inside the catch block. `docs/parity/notes/r5-jsdoc4.md` §3.
use tsr_ast::Node;
use tsr_binder::BindResult;
use tsr_checker::{Checker, check::FileContext};
use tsr_core::Arena;

fn reports_2492(source: &str) -> Vec<String> {
    let arena = Arena::new();
    let parsed =
        tsr_parser::parse_with_options(&arena, source, tsr_parser::ParseOptions::for_file("t.ts"));
    assert!(parsed.diagnostics.is_empty(), "fixture must parse");
    let root = Node::SourceFile(parsed.source_file).node_id().unwrap();
    let bound = tsr_binder::bind_into_with_jsdoc(
        BindResult::empty(),
        &arena,
        parsed.source_file,
        &parsed.nodes,
        tsr_binder::FileInfo { name: "t.ts", text: source },
        &[],
    );
    let mut checker = Checker::new(&bound, &parsed.nodes, &parsed.node_map);
    checker.check_source_file(root, FileContext { ambient: false, has_parse_errors: false });
    checker
        .diagnostics()
        .iter()
        .filter(|(_, d)| d.message.code() == 2492)
        .map(|(_, d)| source[d.span.start as usize..d.span.end as usize].to_string())
        .collect()
}

#[test]
fn a_let_redeclaring_the_catch_variable_is_ts2492_at_its_name() {
    assert_eq!(reports_2492("try {} catch (e) { let e; }"), ["e"]);
}

#[test]
fn every_bound_name_of_a_destructured_catch_variable_is_checked() {
    assert_eq!(reports_2492("try {} catch ({ a, b }) { const b = 1; }"), ["b"]);
}

#[test]
fn a_var_redeclaration_is_allowed() {
    assert!(reports_2492("try {} catch (e) { var e; }").is_empty());
}

#[test]
fn an_annotated_catch_variable_is_not_this_arm() {
    assert!(reports_2492("try {} catch (e: unknown) { let e; }").is_empty());
}
