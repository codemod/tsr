//! JSX overload resolution (`resolveCall`'s `chooseOverload` for a JSX value
//! tag, `docs/parity/notes/r6-jsx.md` §1). Each expectation is what the
//! native tsgo oracle (pinned `5b1047d`, `--strict --jsx preserve`) reports
//! for the same source, columns included.

use tsr_ast::{NodeMap, NodeTable};
use tsr_binder::BindResult;
use tsr_checker::Checker;
use tsr_checker::check::FileContext;
use tsr_core::Arena;

const JSX: &str = "declare namespace JSX {\n    interface Element { e: any }\n    interface ElementClass { render(): any }\n    interface ElementAttributesProperty { props: {} }\n    interface IntrinsicAttributes { key?: string }\n    interface IntrinsicElements { div: {} }\n}\n";

/// `(line, column, code)` of every diagnostic for one `.tsx` file.
fn codes(source: &str) -> Vec<(u32, u32, String)> {
    let source = format!("{JSX}{source}");
    let arena = Arena::new();
    let mut nodes = NodeTable::new();
    let mut node_map = NodeMap::new();
    let source: &str = arena.alloc_str(&source);
    let file = tsr_parser::parse_into(
        &arena,
        source,
        tsr_parser::ParseOptions::for_file("/a.tsx"),
        &mut nodes,
        &mut node_map,
    );
    assert!(file.diagnostics.is_empty(), "fixture must parse");
    let root = file.source_file.node_id.expect("a parsed file has an id");
    let bound = tsr_binder::bind_into(
        BindResult::empty(),
        &arena,
        file.source_file,
        &nodes,
        tsr_binder::FileInfo { name: "/a.tsx", text: source },
    );
    let mut checker = Checker::new(&bound, &nodes, &node_map);
    checker.check_source_file(root, FileContext { ambient: false, has_parse_errors: false });
    let header = JSX.lines().count() as u32;
    let line_of = |offset: u32| {
        let before = &source[..offset as usize];
        let line = before.matches('\n').count() as u32 + 1;
        let column = offset - before.rfind('\n').map_or(0, |at| at as u32 + 1) + 1;
        (line - header, column)
    };
    checker
        .diagnostics()
        .iter()
        .map(|(_, diagnostic)| {
            let (line, column) = line_of(diagnostic.span.start);
            (line, column, diagnostic.code())
        })
        .collect()
}

#[test]
fn no_applicable_function_overload_reports_ts2769_on_the_tag() {
    let found = codes(
        "declare function F(p: { a: string }): JSX.Element;\ndeclare function F(p: { b: number }): JSX.Element;\nconst x = <F a={1} />;\n",
    );
    assert_eq!(found, vec![(3, 14, "TS2769".to_string())]);
}

#[test]
fn an_applicable_overload_is_chosen_silently() {
    let found = codes(
        "declare function F(p: { a: string }): JSX.Element;\ndeclare function F(p: { b: number }): JSX.Element;\nconst x = <F b={1} />;\n",
    );
    assert_eq!(found, vec![]);
}

#[test]
fn a_class_with_two_constructors_reports_its_excess_attribute_as_ts2769() {
    let found = codes(
        "declare class C {\n    constructor(p: { a?: string });\n    constructor(p: { a?: string }, context: any);\n    props: { a?: string };\n    render(): any;\n}\nconst x = <C b=\"\" />;\n",
    );
    assert_eq!(found, vec![(7, 14, "TS2769".to_string())]);
}

#[test]
fn a_parameterless_candidate_takes_the_attributes_argument() {
    // `hasCorrectArity`'s JSX arm: with an attributes argument every
    // candidate's parameter count is one, so `F()` is an argument-error
    // candidate too (its props are `IntrinsicAttributes & unknown`).
    let found = codes(
        "declare function F(): JSX.Element;\ndeclare function F(p: { a: string }): JSX.Element;\nconst x = <F a={1} />;\n",
    );
    assert_eq!(found, vec![(3, 14, "TS2769".to_string())]);
}

#[test]
fn an_intrinsic_tag_takes_no_type_arguments() {
    assert_eq!(codes("const x = <div<number> />;\n"), vec![(1, 16, "TS2558".to_string())]);
}

#[test]
fn a_value_tag_with_too_many_type_arguments_reports_the_range() {
    let found = codes(
        "declare function F<T, U = string>(p: { a: T }): JSX.Element;\nconst x = <F<number, string, boolean> a={1} />;\n",
    );
    assert_eq!(found, vec![(2, 14, "TS2558".to_string())]);
}

#[test]
fn a_written_type_argument_outside_its_constraint_reports_ts2344() {
    // `U` is filled from its default (`fillMissingTypeArguments`).
    let found = codes(
        "declare function F<T extends string, U = string>(p: { a: T }): JSX.Element;\nconst x = <F<number> a={1} />;\n",
    );
    assert_eq!(found, vec![(2, 14, "TS2344".to_string())]);
}
