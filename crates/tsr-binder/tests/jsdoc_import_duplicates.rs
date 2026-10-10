//! `@import` bindings are alias declarations in the file's locals, bound
//! with `SymbolFlagsAliasExcludes` (`declareModuleMember`, `binder.go:380`),
//! so a second `@import` of a name is TS2300 on both. Expectations were
//! checked against a native `tsgo` built from the pinned submodule.

use tsr_ast::NodeFlags;
use tsr_binder::{BindResult, FileInfo};
use tsr_core::Arena;

fn codes(source: &str) -> Vec<(String, u32)> {
    let arena = Arena::new();
    let mut parsed = tsr_parser::parse_with_options(
        &arena,
        source,
        tsr_parser::ParseOptions::for_file("foo.js"),
    );
    assert!(parsed.diagnostics.is_empty());
    let root = parsed.source_file.node_id.expect("registered source file");
    parsed.nodes.add_flags(root, NodeFlags::JAVASCRIPT_FILE);
    let docs: Vec<_> = parsed.jsdoc.iter().collect();
    let bound = tsr_binder::bind_into_with_jsdoc(
        BindResult::empty(),
        &arena,
        parsed.source_file,
        &parsed.nodes,
        FileInfo { name: "foo.js", text: source },
        &docs,
    );
    let mut codes: Vec<(String, u32)> =
        bound.diagnostics().iter().map(|d| (d.code().clone(), d.span.start)).collect();
    codes.sort();
    codes
}

#[test]
fn a_repeated_import_tag_binding_is_ts2300_on_both() {
    let source = "/**\n * @import { Foo } from \"./types\"\n */\n\n/**\n * @import { Foo } from \"./types\"\n */\nfunction f() {}\n";
    let first = u32::try_from(source.find("Foo").expect("first")).expect("small");
    let second = u32::try_from(source.rfind("Foo").expect("second")).expect("small");
    assert_eq!(codes(source), [("TS2300".to_string(), first), ("TS2300".to_string(), second)]);
}

#[test]
fn distinct_import_tag_bindings_do_not_report() {
    let source = "/**\n * @import { Foo } from \"./types\"\n * @import { Bar as Baz } from \"./types\"\n */\nfunction f() {}\n";
    assert!(codes(source).is_empty());
}
