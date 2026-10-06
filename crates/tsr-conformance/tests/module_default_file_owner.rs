//! Native 5b1047d: synthetic defaults retain the original file-module value.
use std::fmt::Write as _;

use tsr_ast::{Expression, Node, NodeId, SyntaxKind};
use tsr_binder::{SymbolFlags, SymbolId};
use tsr_conformance::{TestCase, diagnostics_suite, types_producer};
use tsr_core::{Arena, Idx};

const ORIGINAL: &str = r#"// @target: es2015
// @module: commonjs
// @filename: mod.ts
declare function fun(): void;
export default fun;
// @filename: a.ts
import mod = require("./mod");
export = mod;
// @filename: b.ts
import a from "./a";
import { default as b } from "./a";
import c, { default as d } from "./a";
import * as self from "./b";
export { default } from "./a";
export { default as def } from "./a";
a; b; c; d; self.default; self.def;
a(); b(); c(); d(); self.default(); self.def();
a.default(); b.default(); c.default(); d.default();
self.default.default(); self.def.default();
"#;

fn aliases(program: &tsr_compiler::Program<'_>, file: &str) -> Vec<(NodeId, SymbolId)> {
    let file = program.source_file(file).expect("fixture file").source_file().node_id.unwrap();
    (0..program.nodes().len())
        .filter_map(|index| {
            let id = NodeId::from_usize(index);
            if !matches!(
                program.nodes().kind(id),
                SyntaxKind::ImportClause
                    | SyntaxKind::ImportSpecifier
                    | SyntaxKind::ExportSpecifier
            ) {
                return None;
            }
            let symbol = program.binder().symbol_of(id)?;
            if !program.binder().symbols().get(symbol).flags.contains(SymbolFlags::ALIAS) {
                return None;
            }
            let mut root = id;
            while let Some(parent) = program.nodes().parent(root) {
                root = parent;
            }
            (root == file).then_some((id, symbol))
        })
        .collect()
}

#[test]
fn defaults_share_the_original_module_not_its_default_property() {
    for interop in [false, true] {
        for synthetic in [false, true] {
            for precheck in [false, true] {
                for reversed_queries in [false, true] {
                    let source = format!(
                        "// @esModuleInterop: {interop}\n// @allowSyntheticDefaultImports: {synthetic}\n{ORIGINAL}"
                    );
                    let case = TestCase::parse("probe/file-default", "fixture.ts", &source);
                    let arena = Arena::new();
                    let program = types_producer::program_for_case(&arena, &case);
                    let mut checker = types_producer::configured_checker(&program);
                    let file = program.source_file("b.ts").unwrap().source_file().node_id.unwrap();
                    if precheck {
                        checker.check_source_file(
                            file,
                            tsr_checker::check::FileContext {
                                ambient: false,
                                has_parse_errors: false,
                            },
                        );
                    }
                    let module = program
                        .binder()
                        .symbol_of(
                            program.source_file("mod.ts").unwrap().source_file().node_id.unwrap(),
                        )
                        .unwrap();
                    let wrapper = program
                        .binder()
                        .symbol_of(
                            program.source_file("a.ts").unwrap().source_file().node_id.unwrap(),
                        )
                        .unwrap();
                    let immediate =
                        *program.binder().symbols().get(wrapper).exports.get("export=").unwrap();
                    let mut sites = aliases(&program, "b.ts");
                    assert_eq!(sites.len(), 6);
                    if reversed_queries {
                        sites.reverse();
                    }
                    for _ in 0..2 {
                        for &(_, symbol) in &sites {
                            assert_eq!(checker.resolve_alias(symbol), Some(immediate));
                            let value = checker.get_type_of_symbol(symbol);
                            assert_eq!(value, checker.get_type_of_symbol(module));
                            let default =
                                checker.get_type_of_property_of_type(value, "default").unwrap();
                            assert_ne!(value, default);
                            assert_eq!(checker.type_to_string(default), "() => void");
                        }
                        let mut calls = (0..program.nodes().len())
                            .filter_map(|index| {
                                let id = NodeId::from_usize(index);
                                let Node::CallExpression(call) = program.node_map().get(id)? else {
                                    return None;
                                };
                                let mut root = id;
                                while let Some(parent) = program.nodes().parent(root) {
                                    root = parent;
                                }
                                (root == file).then_some(call)
                            })
                            .collect::<Vec<_>>();
                        assert_eq!(calls.len(), 12);
                        if reversed_queries {
                            calls.reverse();
                        }
                        let mut results = calls
                            .iter()
                            .map(|call| checker.check_expression(Expression::CallExpression(call)))
                            .collect::<Vec<_>>();
                        if reversed_queries {
                            results.reverse();
                        }
                        assert!(results[..6].iter().all(|&ty| ty == checker.intrinsics().error));
                        assert!(
                            results[6..].iter().all(|&ty| checker.type_to_string(ty) == "void")
                        );
                    }
                }
            }
        }
    }
}

#[test]
fn chains_past_the_naming_bound_still_resolve() {
    for depth in [4, 5] {
        let mut source =
            String::from("// @module: commonjs\n// @filename: mod.ts\nexport const token = 17;\n");
        let mut previous = "mod".to_string();
        for index in 0..depth {
            let name = format!("wrapper{index}");
            write!(
                source,
                "// @filename: {name}.ts\nimport canonical = require(\"./{previous}\");\nexport = canonical;\n"
            )
            .unwrap();
            previous = name;
        }
        write!(
            source,
            "// @filename: use.ts\nimport first from \"./{previous}\";\nimport {{ default as named }} from \"./{previous}\";\nfirst; named;\n"
        )
        .unwrap();
        let case = TestCase::parse("probe/default-depth", "fixture.ts", &source);
        let arena = Arena::new();
        let program = types_producer::program_for_case(&arena, &case);
        let mut checker = types_producer::configured_checker(&program);
        let module = program
            .binder()
            .symbol_of(program.source_file("mod.ts").unwrap().source_file().node_id.unwrap())
            .unwrap();
        for _ in 0..2 {
            for (_, symbol) in aliases(&program, "use.ts") {
                let value = checker.get_type_of_symbol(symbol);
                // getTargetOfModuleDefault has no chain bound: past the
                // naming walk's depth the alias still resolves and types.
                assert!(checker.resolve_alias(symbol).is_some());
                assert_eq!(value, checker.get_type_of_symbol(module));
            }
        }
    }
}

#[test]
fn immediate_links_preserve_intermediate_import_type_provenance() {
    let source = r#"// @module: commonjs
// @filename: mod.ts
export class A {}
// @filename: wrapper.ts
import type types = require("./mod");
export = types;
// @filename: bridge.ts
export { default as forwarded } from "./wrapper";
// @filename: use.ts
import first from "./wrapper";
import { default as named } from "./wrapper";
import { forwarded } from "./bridge";
first; named; forwarded;
new first.A(); new named.A(); new forwarded.A();
"#;
    let case = TestCase::parse("probe/file-default-type-only", "fixture.ts", source);
    let diagnostics = diagnostics_suite::reported_for(&case);
    assert_eq!(diagnostics.iter().filter(|diagnostic| diagnostic.code == 1361).count(), 6);
    assert!(diagnostics.iter().all(|diagnostic| diagnostic.code == 1361));
}

#[test]
fn direct_real_defaults_still_name_the_callable() {
    let source = ORIGINAL.replace("from \"./a\"", "from \"./mod\"");
    let case = TestCase::parse("probe/direct-default", "fixture.ts", &source);
    let arena = Arena::new();
    let program = types_producer::program_for_case(&arena, &case);
    let mut checker = types_producer::configured_checker(&program);
    let module = program
        .binder()
        .symbol_of(program.source_file("mod.ts").unwrap().source_file().node_id.unwrap())
        .unwrap();
    let value = checker.get_type_of_symbol(module);
    let default = checker.get_type_of_property_of_type(value, "default").unwrap();
    for (_, symbol) in aliases(&program, "b.ts") {
        assert_eq!(checker.get_type_of_symbol(symbol), default);
        assert_ne!(default, value);
    }
}

#[test]
fn every_alias_shape_resolves_to_the_immediate_export_equals() {
    for (wrapper, suffix) in [
        ("import * as canonical from \"./mod\"; export = canonical;", "ts"),
        ("import canonical from \"./plain\"; export = canonical;", "ts"),
        ("import { default as canonical } from \"./plain\"; export = canonical;", "ts"),
        ("import canonical = require(\"./mod\"); export = (canonical);", "ts"),
        (
            "import source = require(\"./mod\"); import canonical = source; export = canonical;",
            "ts",
        ),
        ("import canonical = require(\"./mod\"); export = canonical;", "d.ts"),
        ("import canonical from \"./wrapper\"; export = canonical;", "ts"),
        ("import { default as canonical } from \"./wrapper\"; export = canonical;", "ts"),
        ("import canonical = require(\"./wrapper\"); export = canonical;", "ts"),
        ("declare function canonical(): void; export = canonical;", "ts"),
        ("declare class canonical {} export = canonical;", "ts"),
        ("declare namespace canonical { const token: 17; } export = canonical;", "ts"),
        ("declare const canonical: { token: 17 }; export = canonical;", "ts"),
    ] {
        let source = format!(
            "// @module: commonjs\n// @filename: mod.ts\nexport class A {{}}\n// @filename: plain.ts\nimport canonical = require(\"./mod\"); export = canonical;\n// @filename: wrapper.{suffix}\n{wrapper}\n// @filename: use.ts\nimport declined from \"./wrapper\";\nimport {{ default as named }} from \"./wrapper\";\nexport {{ default as forwarded }} from \"./wrapper\";\ndeclined; named;"
        );
        let case = TestCase::parse("probe/default-refusal", "fixture.ts", &source);
        let arena = Arena::new();
        let program = types_producer::program_for_case(&arena, &case);
        let mut checker = types_producer::configured_checker(&program);
        let sites = aliases(&program, "use.ts");
        assert_eq!(sites.len(), 3);
        let wrapper_file = program
            .source_file(&format!("wrapper.{suffix}"))
            .unwrap()
            .source_file()
            .node_id
            .unwrap();
        let wrapper_module = program.binder().symbol_of(wrapper_file).unwrap();
        let immediate =
            program.binder().symbols().get(wrapper_module).exports.get("export=").copied();
        assert!(immediate.is_some());
        // getTargetOfModuleDefault resolves every shape to
        // `resolveExternalModuleSymbol(wrapper, dontResolveAlias = true)`;
        // a wrapper that imports itself is circular and reads errorType.
        let cycle = wrapper.contains("./wrapper");
        for _ in 0..2 {
            for &(_, symbol) in &sites {
                assert_eq!(checker.resolve_alias(symbol), immediate, "{wrapper}");
                if cycle {
                    assert_eq!(checker.get_type_of_symbol(symbol), checker.intrinsics().error);
                }
            }
        }
    }
}
