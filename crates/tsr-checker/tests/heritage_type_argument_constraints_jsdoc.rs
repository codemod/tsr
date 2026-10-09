//! TS2344 on a heritage clause's type arguments, pinned to typescript-go
//! 5b1047d1: `checkClassLikeDeclaration` checks a class base's arguments
//! against its constructors' type parameters (`checker.go:4327`), and
//! `checkTypeReferenceNode` checks `implements` and interface `extends`
//! elements (`:4371`, `:5022`). `docs/parity/notes/r5-jsdoc4.md` §2.
use tsr_ast::Node;
use tsr_binder::BindResult;
use tsr_checker::{Checker, check::FileContext};
use tsr_core::Arena;

fn codes(source: &str) -> Vec<(u32, String)> {
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
        .map(|(_, d)| (d.message.code(), source[d.span.start as usize..d.span.end as usize].into()))
        .collect()
}

const DECLARATIONS: &str = "interface Foo { a: number }\n\
    class A<T extends Foo> { t!: T }\n\
    interface I<T extends Foo> { t: T }\n";

#[test]
fn a_class_base_argument_must_satisfy_the_constraint() {
    let source = format!("{DECLARATIONS}class B extends A<{{ a: string }}> {{}}\n");
    assert!(codes(&source).contains(&(2344, "{ a: string }".to_string())));
}

#[test]
fn an_interface_base_argument_must_satisfy_the_constraint() {
    let source = format!("{DECLARATIONS}interface J extends I<{{ a: string }}> {{}}\n");
    assert!(codes(&source).contains(&(2344, "{ a: string }".to_string())));
}

#[test]
fn an_implemented_argument_must_satisfy_the_constraint() {
    let source =
        format!("{DECLARATIONS}class C implements I<{{ a: string }}> {{ t!: {{ a: string }} }}\n");
    assert!(codes(&source).contains(&(2344, "{ a: string }".to_string())));
}

#[test]
fn a_satisfying_base_argument_reports_nothing() {
    let source = format!("{DECLARATIONS}class D extends A<{{ a: number }}> {{}}\n");
    assert!(!codes(&source).iter().any(|(code, _)| *code == 2344));
}
