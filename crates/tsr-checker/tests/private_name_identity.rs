//! A private name's per-class identity in the relation's property walk
//! (`binder.GetSymbolNameForPrivateIdentifier`, `binder/binder.go:369`;
//! `getUnmatchedPropertiesWorker`, `relater.go:984`). Expectations were
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
        .map(|(_, d)| d.code().to_string())
        .filter(|code| {
            matches!(code.as_str(), "TS2322" | "TS2415" | "TS2416" | "TS2739" | "TS2741")
        })
        .collect();
    codes.sort();
    codes
}

#[test]
fn a_redeclared_private_name_does_not_override_the_inherited_one() {
    let source = "class A { #foo: number = 1; }\nclass B extends A { #foo: string = \"\"; }";
    assert!(codes(source).is_empty());
}

#[test]
fn a_redeclared_private_method_does_not_override_the_inherited_one() {
    let source = "class A { #m(a: number) {} }\nclass B extends A { #m(a: string) {} }";
    assert!(codes(source).is_empty());
}

#[test]
fn an_unrelated_class_with_the_same_private_name_still_fails() {
    let source = "class A { #foo = 1; }\nclass B { #foo = 1; }\nconst a: A = new B();";
    assert_eq!(codes(source), ["TS2322"]);
}

#[test]
fn a_static_private_name_is_not_an_unmatched_property() {
    let source =
        "class A { static #foo: number; static #bar: number; }\nconst c: typeof A = class {};";
    assert!(codes(source).is_empty());
}
