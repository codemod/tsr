//! r6-jsx2's JSX checks (`docs/parity/notes/r6-jsx2.md`). Each expectation is
//! what the native tsgo oracle (pinned `5b1047d`, `--strict --jsx preserve`)
//! reports for the same source, columns included.

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
fn context_sensitive_attributes_choose_among_overloads() {
    // `argCheckMode` `SkipContextSensitive`: the first candidate passes the
    // skip check (the callback is `anyFunctionType`, the attributes are not
    // fresh), then fails the `Normal` one on the excess `extra`; the second
    // fails on `onClick`, and TS2769 lands on the last candidate's excess
    // attribute. Without `extra` the first candidate is chosen silently.
    let found = codes(
        "declare function log(...a: any[]): void;\n\
         interface B { onClick: (k: \"left\" | \"right\") => void }\n\
         interface L { goTo: \"home\" | \"contact\" }\n\
         declare function M(p: B): JSX.Element;\n\
         declare function M(p: L): JSX.Element;\n\
         const a = <M onClick={(k) => { log(k) }} extra />;\n\
         const b = <M onClick={(k) => { log(k) }} />;\n\
         const c = <M goTo=\"home\" onClick={(k) => { log(k) }} />;\n\
         const d = <M onClick={(k) => k.length} />;\n",
    );
    assert_eq!(found, vec![(6, 14, "TS2769".to_string()), (8, 26, "TS2769".to_string())]);
}

#[test]
fn a_tag_needing_more_arguments_than_the_factory_provides_is_ts6229() {
    // `checkTagNameDoesNotExpectTooManyArguments`: the factory's first
    // parameter is an alias reference (`SFC<P>`) whose body's call signature
    // takes two parameters; `C4` needs four, `C2` fits and its attributes are
    // then related as usual.
    let found = codes(
        "declare namespace React {\n\
         \x20   interface StatelessComponent<P> { (props: P, context?: any): JSX.Element | null }\n\
         \x20   type SFC<P> = StatelessComponent<P>;\n\
         \x20   function createElement<P>(type: SFC<P> | { tag: string }, props?: P): JSX.Element;\n\
         \x20   function createElement<P>(type: SFC<P>, props?: P): JSX.Element;\n\
         }\n\
         function C4(props: { x: number }, a: any, b: any, c: any) { return <div></div>; }\n\
         function C2(props: { x: number }, a: any) { return <div></div>; }\n\
         const a = <C4 x={2} />;\n\
         const b = <C2 x={2} />;\n\
         const c = <C2 x=\"no\" />;\n",
    );
    assert_eq!(found, vec![(9, 12, "TS6229".to_string()), (11, 15, "TS2322".to_string())]);
}
