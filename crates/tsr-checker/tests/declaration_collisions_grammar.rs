//! TS18027 (`checkWeakMapSetCollision`, `checker.go:10548`), TS2398
//! (`checkParameter`, `checker.go:2674`) and TS2500
//! (`checkClassLikeDeclaration`'s implements loop, `checker.go:4369`).
//! Expectations were checked against a native `tsgo` built from the pinned
//! submodule (r7-grammar lane note §55).

use tsr_ast::NodeId;
use tsr_checker::Checker;
use tsr_checker::check::FileContext;
use tsr_core::{Arena, CompilerOptions, ScriptTarget};

fn diagnostics(source: &str, target: ScriptTarget) -> Vec<(String, u32)> {
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
    let root: NodeId = file.source_file.node_id().expect("a parsed file has an id");
    let bound = tsr_binder::bind_into(
        tsr_binder::BindResult::empty(),
        &arena,
        file.source_file,
        &nodes,
        tsr_binder::FileInfo { name: "a.ts", text: source },
    );
    let mut checker = Checker::new(&bound, &nodes, &node_map);
    checker.apply_compiler_options(&CompilerOptions { target, ..CompilerOptions::default() });
    checker.check_source_file(root, FileContext { ambient: false, has_parse_errors: false });
    let mut out: Vec<(String, u32)> = checker
        .diagnostics()
        .iter()
        .filter(|(_, d)| matches!(d.code().as_str(), "TS18027" | "TS2398" | "TS2500"))
        .map(|(_, d)| {
            let start = d.span.start as usize;
            let line = u32::try_from(source[..start].matches('\n').count() + 1).unwrap_or(0);
            (d.code().clone(), line)
        })
        .collect();
    out.sort();
    out
}

#[test]
fn weak_map_and_weak_set_collide_with_a_private_named_element_below_es2022() {
    let source = "function a() { let WeakMap; class C { #x() {} } }
function b() { let WeakSet = 1; { class D { get #y() { return 1; } } } }
function d() { class E { #z = 1; } { let WeakMap; } }
";
    let expected = vec![("TS18027".to_string(), 1), ("TS18027".to_string(), 2)];
    assert_eq!(diagnostics(source, ScriptTarget::ES2015), expected);
    assert!(diagnostics(source, ScriptTarget::ES2022).is_empty());
}

#[test]
fn constructor_parameter_property_name_and_optional_chain_implements() {
    let source = "class Foo {
  constructor(public constructor: string) {}
}
namespace A { export class B {} }
class C2 implements A?.B {}
class C3 implements A.B {}
";
    let expected = vec![("TS2398".to_string(), 2), ("TS2500".to_string(), 5)];
    assert_eq!(diagnostics(source, ScriptTarget::ES2015), expected);
}
