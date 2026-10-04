//! Native 5b1047d: missing required Function members in every file-module view.
use tsr_ast::NodeId;
use tsr_checker::Checker;
use tsr_checker::relater::{Relation, Ternary};
use tsr_checker::resolution::ModuleHost;
use tsr_conformance::{TestCase, diagnostics_suite, types_producer};
use tsr_core::Arena;

fn fixture(function: &str, object: &str, module: &str) -> String {
    format!(
        "// @noLib: true\n// @module: commonjs\n\
         // @filename: globals.d.ts\n\
         interface Array<T> {{}}\ninterface Boolean {{}}\n\
         interface CallableFunction {{}}\n{function}\n\
         interface IArguments {{}}\ninterface NewableFunction {{}}\n\
         interface Number {{}}\n{object}\n\
         interface RegExp {{}}\ninterface String {{}}\n\
         // @filename: mod.ts\n{module}\n\
         // @filename: use.ts\n/// <reference path=\"globals.d.ts\" />\n\
         import original = require(\"./mod\");\noriginal();\n"
    )
}

fn check(source: &str, file: &str, expected: Ternary) {
    for interop in [false, true] {
        for synthetic in [false, true] {
            for reversed in [false, true] {
                let text = format!(
                    "// @esModuleInterop: {interop}\n\
                     // @allowSyntheticDefaultImports: {synthetic}\n{source}"
                );
                let mut case =
                    TestCase::parse("probe/module-function-absence", "fixture.ts", &text);
                if reversed {
                    case.files.reverse();
                }
                let arena = Arena::new();
                let program = types_producer::program_for_case(&arena, &case);
                let mut checker = types_producer::configured_checker(&program);
                let owner = program
                    .binder()
                    .symbol_of(program.source_file(file).unwrap().source_file().node_id.unwrap())
                    .unwrap();
                let function = program.binder().global("Function").unwrap();
                for warm in [false, true] {
                    let (source, target) = if reversed {
                        let target = checker.get_declared_type_of_symbol(function);
                        (checker.get_type_of_symbol(owner), target)
                    } else {
                        let source = checker.get_type_of_symbol(owner);
                        (source, checker.get_declared_type_of_symbol(function))
                    };
                    for relation in
                        [Relation::Assignable, Relation::Subtype, Relation::StrictSubtype]
                    {
                        assert_eq!(
                            checker.relate_ternary(source, target, relation),
                            expected,
                            "{relation:?}, reversed={reversed}, warm={warm}, fixture={text}"
                        );
                    }
                    // Identity is not exposed by this port's relation API.
                    assert_eq!(
                        checker.relate_ternary(source, target, Relation::Comparable),
                        Ternary::Unknown
                    );
                }
            }
        }
    }
}

#[test]
fn required_named_property_missing_from_every_view_is_negative() {
    // Native both original and namespace copies: missing __esModule rejects,
    // even when a copy supplies the other required property, default.
    check(
        &fixture(
            "interface Function { default: any; __esModule: any; }",
            "interface Object {}",
            "export const token = 17;",
        ),
        "mod.ts",
        Ternary::NotRelated,
    );
    check(
        &fixture(
            "interface Function { requiredToken(): void; }",
            "interface Object {}",
            "export const other = 29;",
        ),
        "mod.ts",
        Ternary::NotRelated,
    );
}

#[test]
fn possible_default_own_optional_and_object_properties_are_not_witnesses() {
    for (function, object, module) in [
        ("default: any;", "", "export const token = 17;"),
        ("__esModule: any;", "", "export const __esModule = true;"),
        ("token?: any;", "", "export const other = 17;"),
        ("token: any;", "token: any;", "export const other = 17;"),
        ("token: any;", "token?: any;", "export const other = 17;"),
        ("", "", "export const other = 17;"),
    ] {
        check(
            &fixture(
                &format!("interface Function {{ {function} }}"),
                &format!("interface Object {{ {object} }}"),
                module,
            ),
            "mod.ts",
            Ternary::Unknown,
        );
    }
}

#[test]
fn numeric_function_names_cannot_supply_an_absence_witness() {
    // Native's shortest decimal key is 1000000000000000100. The binder
    // currently publishes 1000000000000000128 for this numeric declaration.
    // An absence of the latter key cannot disprove the former's presence.
    for name in ["1000000000000000100", "0x10"] {
        for module in [
            "const value = 17; export { value as '1000000000000000100', value as '16' };",
            "export const other = 29;",
        ] {
            for member in [format!("{name}: any;"), format!("{name}(): void;")] {
                check(
                    &fixture(
                        &format!("interface Function {{ {member} }}"),
                        "interface Object {}",
                        module,
                    ),
                    "mod.ts",
                    Ternary::Unknown,
                );
            }
        }
    }
    // Refuse only the numeric witness, not an independent supported name.
    check(
        &fixture(
            "interface Function { 1000000000000000100: any; requiredToken: any; }",
            "interface Object {}",
            "const value = 17; export { value as '1000000000000000100' };",
        ),
        "mod.ts",
        Ternary::NotRelated,
    );
}

#[test]
fn numeric_object_surface_is_not_a_complete_named_fallback() {
    for (function, object) in [
        ("'1000000000000000100': any;", "1000000000000000100: any;"),
        ("'1000000000000000100'(): void;", "1000000000000000100(): void;"),
        ("'16': any;", "0x10: any;"),
        ("requiredToken: any;", "0x10: any;"),
    ] {
        check(
            &fixture(
                &format!("interface Function {{ {function} }}"),
                &format!("interface Object {{ {object} }}"),
                "export const other = 29;",
            ),
            "mod.ts",
            Ternary::Unknown,
        );
    }
    // A written string key needs no numeric canonicalization. Own presence
    // must decline, while true absence from a closed string surface rejects.
    for (module, expected) in [
        ("const value = 17; export { value as '1000000000000000100' };", Ternary::Unknown),
        ("export const other = 29;", Ternary::NotRelated),
    ] {
        check(
            &fixture(
                "interface Function { '1000000000000000100': any; }",
                "interface Object {}",
                module,
            ),
            "mod.ts",
            expected,
        );
    }
}

#[test]
fn open_replaced_and_indirected_file_surfaces_decline() {
    for module in [
        "export * from './missing';",
        "declare const value: {}; export = value;",
        "export function F(): 17 { return 17; } export { F as 'module.exports' };",
        "import value = require('./cycle'); export = value;",
    ] {
        check(
            &fixture("interface Function { requiredToken: any; }", "interface Object {}", module),
            "mod.ts",
            Ternary::Unknown,
        );
    }
}

#[test]
fn declaration_and_js_files_decline() {
    let source = fixture(
        "interface Function { requiredToken: any; }",
        "interface Object {}",
        "export const other = 17;",
    );
    for (name, body) in
        [("mod.d.ts", "export declare const other: 17;"), ("mod.js", "export const other = 17;")]
    {
        let text = source
            .replace("// @filename: mod.ts", &format!("// @filename: {name}"))
            .replace("export const other = 17;", body);
        check(&format!("// @allowJs: true\n{text}"), name, Ternary::Unknown);
    }
}

#[test]
fn invalid_globals_and_open_object_surfaces_decline() {
    for (function, object) in [
        ("type Function = { requiredToken: any };", "interface Object {}"),
        ("interface Function<T> { requiredToken: any; }", "interface Object {}"),
        ("declare class Function { requiredToken: any; }", "interface Object {}"),
        ("interface Function { requiredToken: any; }", "type Object = {}"),
        ("interface Function { requiredToken: any; }", "interface Object<T> {}"),
        ("interface Function { requiredToken: any; }", "interface Object extends Missing {}"),
        ("interface Function { requiredToken: any; }", "interface Object { [key: string]: any; }"),
        (
            "interface Function { requiredToken: any; }",
            "declare const key: unique symbol; interface Object { [key]: any; }",
        ),
    ] {
        check(&fixture(function, object, "export const other = 17;"), "mod.ts", Ternary::Unknown);
    }
}

#[test]
fn default_copy_and_reporting_consumers_remain_unsupported() {
    let source = fixture(
        "interface Function { default: any; }",
        "interface Object {}",
        "export const token = 17;",
    )
    .replace("// @module: commonjs", "// @module: nodenext")
    .replace("// @filename: mod.ts", "// @filename: mod.cts")
    .replace("// @filename: use.ts", "// @filename: use.mts")
    .replace(
        "import original = require(\"./mod\");\noriginal();",
        "import original = require('./mod.cjs'); import * as copy from './mod.cjs'; original(); copy();",
    );
    // Native has exactly TS2349 on original(), but copy() is Function-related.
    // The relater must not use original default absence for either Rust view.
    check(&source, "mod.cts", Ternary::Unknown);
    let case = TestCase::parse("probe/module-copy", "fixture.ts", &source);
    assert!(diagnostics_suite::reported_for(&case).is_empty());
}

#[test]
fn a_same_named_foreign_function_is_not_the_global_target() {
    let source = fixture(
        "interface Function { requiredToken: any; }",
        "interface Object {}",
        "export interface Function { requiredToken: any; } export const other = 17;",
    );
    let case = TestCase::parse("probe/foreign-function", "fixture.ts", &source);
    let arena = Arena::new();
    let program = types_producer::program_for_case(&arena, &case);
    let mut checker = types_producer::configured_checker(&program);
    let owner = program
        .binder()
        .symbol_of(program.source_file("mod.ts").unwrap().source_file().node_id.unwrap())
        .unwrap();
    let foreign = *program.binder().symbols().get(owner).exports.get("Function").unwrap();
    let actual = program.binder().global("Function").unwrap();
    assert_ne!(foreign, actual);
    let module = checker.get_type_of_symbol(owner);
    let foreign = checker.get_declared_type_of_symbol(foreign);
    for relation in [Relation::Assignable, Relation::Subtype, Relation::StrictSubtype] {
        assert_eq!(checker.relate_ternary(module, foreign, relation), Ternary::Unknown);
    }
}

struct PartialHost;

impl ModuleHost for PartialHost {
    fn resolved_module(&self, _file: NodeId, _specifier: &str) -> Option<NodeId> {
        None
    }

    fn module_resolution_found(&self, _file: NodeId, _specifier: &str) -> bool {
        false
    }
}

struct FileMetadataHost<'a> {
    file: NodeId,
    path: &'a str,
    declaration: bool,
}

impl ModuleHost for FileMetadataHost<'_> {
    fn resolved_module(&self, _file: NodeId, _specifier: &str) -> Option<NodeId> {
        None
    }

    fn module_resolution_found(&self, _file: NodeId, _specifier: &str) -> bool {
        false
    }

    fn file_path(&self, file: NodeId) -> Option<String> {
        (file == self.file).then(|| self.path.to_owned())
    }

    fn is_declaration_file(&self, file: NodeId) -> bool {
        file == self.file && self.declaration
    }
}

#[test]
fn incomplete_hosts_and_unsupported_paths_cannot_prove_file_eligibility() {
    let text = fixture(
        "interface Function { requiredToken: any; }",
        "interface Object {}",
        "export const other = 17;",
    );
    let mut case = TestCase::parse("probe/file-metadata", "fixture.ts", &text);
    let mut failures = Vec::new();
    for reversed in [false, true] {
        if reversed {
            case.files.reverse();
        }
        let arena = Arena::new();
        let program = types_producer::program_for_case(&arena, &case);
        let file = program.source_file("mod.ts").unwrap().source_file().node_id.unwrap();
        let owner = program.binder().symbol_of(file).unwrap();
        let function = program.binder().global("Function").unwrap();
        let empty = TestCase::parse("probe/empty-host", "empty.ts", "// @noLib: true\n");
        let unknown = types_producer::program_for_case(&arena, &empty);
        // The default false does not distinguish unknown membership from TS.
        assert!(!PartialHost.is_declaration_file(file));
        assert!(PartialHost.file_path(file).is_none());
        assert!(!unknown.is_declaration_file(file));
        assert!(unknown.file_path(file).is_none());
        let mut run = |label, host: Option<&dyn ModuleHost>, expected| {
            let mut checker = Checker::with_module_host(
                program.binder(),
                program.nodes(),
                program.node_map(),
                host,
            );
            for warm in [false, true] {
                let (source, target) = if reversed {
                    let target = checker.get_declared_type_of_symbol(function);
                    (checker.get_type_of_symbol(owner), target)
                } else {
                    let source = checker.get_type_of_symbol(owner);
                    (source, checker.get_declared_type_of_symbol(function))
                };
                for relation in [Relation::Assignable, Relation::Subtype, Relation::StrictSubtype] {
                    let actual = checker.relate_ternary(source, target, relation);
                    eprintln!(
                        "HOST {label:?} reversed={reversed} warm={warm} file={file:?} \
                         path={:?} declaration={:?} source={source:?} target={target:?} \
                         relation={relation:?} actual={actual:?} expected={expected:?}",
                        host.and_then(|host| host.file_path(file)),
                        host.map(|host| host.is_declaration_file(file)),
                    );
                    if actual != expected {
                        failures.push(format!(
                            "{label:?} {relation:?} reversed={reversed} warm={warm}: \
                             {actual:?} != {expected:?}"
                        ));
                    }
                }
            }
        };
        run("partial", Some(&PartialHost), Ternary::Unknown);
        run("unknown-program", Some(&unknown), Ternary::Unknown);
        run("absent", None, Ternary::Unknown);
        run("real-program", Some(&program), Ternary::NotRelated);
        for path in
            ["", "mod", "mod.js", "mod.json", "mod.tsx", "mod.d.ts", "mod.d.mts", "mod.d.cts"]
        {
            let host = FileMetadataHost { file, path, declaration: false };
            run(path, Some(&host), Ternary::Unknown);
        }
        for path in ["mod.ts", "mod.mts", "mod.cts"] {
            let host = FileMetadataHost { file, path, declaration: true };
            run(path, Some(&host), Ternary::Unknown);
            let host = FileMetadataHost { file, path, declaration: false };
            run(path, Some(&host), Ternary::NotRelated);
        }
    }
    assert!(failures.is_empty(), "{failures:#?}");
}
