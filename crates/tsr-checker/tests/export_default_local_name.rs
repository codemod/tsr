//! `NameResolver.Resolve`'s export-default arm (`nameresolver.go:109`): the
//! written name of `export default class C` resolves inside an external module
//! or an *ambient* module declaration, and the internal `default` key is never
//! an answer from the exports arm.

use tsr_ast::NodeId;
use tsr_checker::Checker;
use tsr_checker::check::FileContext;
use tsr_core::Arena;

fn messages(source: &str) -> Vec<String> {
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
    assert!(file.diagnostics.is_empty(), "fixture must parse");
    let root: NodeId = file.source_file.node_id.expect("a parsed file has an id");
    let bound = tsr_binder::bind_into(
        tsr_binder::BindResult::empty(),
        &arena,
        file.source_file,
        &nodes,
        tsr_binder::FileInfo { name: "a.ts", text: source },
    );
    let mut checker = Checker::new(&bound, &nodes, &node_map);
    checker.check_source_file(root, FileContext { ambient: false, has_parse_errors: false });
    checker.diagnostics().iter().map(|(_, d)| d.text()).collect()
}

/// `declare module "foo"` carries `NodeFlagsAmbient`, so `C` resolves through
/// the default export (`compiler/es5ExportDefaultClassDeclaration4`).
#[test]
fn a_default_class_is_visible_by_name_in_its_ambient_module() {
    assert!(
        messages("declare module \"foo\" { export var before: C; export default class C {} }")
            .is_empty()
    );
}

/// `typeof default` names the internal key, which no arm admits
/// (`compiler/defaultIsNotVisibleInLocalScope`).
#[test]
fn the_default_key_is_not_a_name() {
    assert_eq!(
        messages("export default function () { return true; }\nexport type X = typeof default;"),
        ["Cannot find name 'default'."]
    );
}
