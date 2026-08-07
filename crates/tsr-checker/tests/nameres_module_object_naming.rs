//! Naming a module object at the reference site.
//!
//! A module symbol's name in this port is the stripped file path, and
//! `TypeData::Anonymous`'s `text` is baked at type creation — so until now
//! `import * as ns from "./m"` could only print `typeof /m` where upstream
//! prints `typeof ns`. Every module-object item in this workstream was refused
//! on that one sentence (`bd tsr-6ph` at 2.1 and 2.5 wrong per right,
//! `bd tsr-4jk`, and the ALIAS row at 1.0).
//!
//! `Checker::type_to_string_at` is the second entry point that fixes it, and
//! `Checker::resolve_alias` gained the three module forms that make it
//! reachable. Both are pinned here.
//!
//! **The gap arm is the point.** Upstream picks the name through
//! `getAccessibleSymbolChain` (`internal/checker/symbolaccessibility.go:373`),
//! and when two aliases in scope name one module the corpus contradicts any
//! single tie-break — see `a_module_named_by_two_aliases_is_a_gap`. So the
//! ambiguous case answers `None` and the line keeps gapping, rather than
//! becoming a confidently wrong name.

use tsr_ast::{NodeId, NodeMap, NodeTable, SyntaxKind};
use tsr_binder::BindResult;
use tsr_checker::{Checker, resolution::ModuleHost};
use tsr_core::Arena;

struct Fixtures {
    files: Vec<(&'static str, NodeId)>,
}

impl ModuleHost for Fixtures {
    fn resolved_module(&self, _importing_file: NodeId, specifier: &str) -> Option<NodeId> {
        let name = specifier.strip_prefix("./").unwrap_or(specifier);
        self.files.iter().find(|(fixture, _)| *fixture == name).map(|&(_, id)| id)
    }
}

struct Fixture<'a> {
    nodes: NodeTable,
    node_map: NodeMap<'a>,
    bound: BindResult<'a>,
    host: Fixtures,
}

/// Parse and bind several files into one identity space, as
/// `tests/cross_file_aliases.rs` does. Both orders are load-bearing.
fn program<'a>(arena: &'a Arena, files: &[(&'static str, &str)]) -> Fixture<'a> {
    let mut nodes = NodeTable::new();
    let mut node_map = NodeMap::new();
    let mut parsed = Vec::new();
    for (name, source) in files {
        let source: &'a str = arena.alloc_str(source);
        let file = tsr_parser::parse_into(
            arena,
            source,
            tsr_parser::ParseOptions::default(),
            &mut nodes,
            &mut node_map,
        );
        assert!(
            file.diagnostics.is_empty(),
            "fixture {name} must parse: {:?}",
            file.diagnostics.iter().map(tsr_diagnostics::Diagnostic::text).collect::<Vec<_>>()
        );
        parsed.push((*name, source, file.source_file));
    }
    let mut bound = BindResult::empty();
    let mut host = Fixtures { files: Vec::new() };
    for (name, source, source_file) in parsed {
        let file_name: &'a str = arena.alloc_str(&format!("/{name}.ts"));
        host.files.push((name, source_file.node_id.expect("a parsed file has an id")));
        bound = tsr_binder::bind_into(
            bound,
            source_file,
            &nodes,
            tsr_binder::FileInfo { name: file_name, text: source },
        );
    }
    Fixture { nodes, node_map, bound, host }
}

/// Render the declaration name of the alias declared by `kind` named `name`,
/// **through the rendering path** — `type_to_string_at`, given that name's own
/// node as the reference. That is the position the `.types` baseline records.
fn rendered_at(fixture: &Fixture<'_>, name: &str, kind: SyntaxKind) -> String {
    let host: Option<&dyn ModuleHost> = Some(&fixture.host);
    let mut checker =
        Checker::with_module_host(&fixture.bound, &fixture.nodes, &fixture.node_map, host);
    let declaration = (0..u32::try_from(fixture.nodes.len()).expect("fits"))
        .map(NodeId::new)
        .filter(|&id| fixture.nodes.kind(id) == kind)
        .find(|&id| {
            fixture.bound.symbol_of(id).is_some_and(|s| fixture.bound.symbols().get(s).name == name)
        })
        .unwrap_or_else(|| panic!("no {kind:?} named `{name}`"));
    let symbol = fixture.bound.symbol_of(declaration).expect("the declaration binds a symbol");
    let id = checker.get_type_of_symbol(symbol);
    // The reference is the declaration's own name node, which is what
    // `types_producer` passes for a declaration-name line.
    let reference =
        fixture.node_map.get(declaration).and_then(|node| node.name_id()).unwrap_or(declaration);
    checker.type_to_string_at(id, reference).unwrap_or_else(|| "error".to_string())
}

#[test]
fn a_namespace_import_prints_its_own_name_and_not_the_file_path() {
    // The line the whole workstream was blocked on. Upstream:
    // `>ns : typeof ns`. Before this, the only reachable answers were `error`
    // (the alias did not resolve) or `typeof /m` (the module symbol's name).
    let arena = Arena::new();
    let fixture = program(
        &arena,
        &[("m", "export const v: string = \"a\";\n"), ("b", "import * as ns from \"./m\";\n")],
    );
    assert_eq!(rendered_at(&fixture, "ns", SyntaxKind::NamespaceImport), "typeof ns");
}

#[test]
fn an_import_equals_require_prints_its_own_name() {
    // 2,014 lines of the ALIAS row are this form, the largest single one.
    let arena = Arena::new();
    let fixture = program(
        &arena,
        &[("m", "export const v: string = \"a\";\n"), ("b", "import mod = require(\"./m\");\n")],
    );
    assert_eq!(rendered_at(&fixture, "mod", SyntaxKind::ImportEqualsDeclaration), "typeof mod");
}

#[test]
fn a_module_named_by_two_aliases_is_a_gap() {
    // `compiler/es6ImportNameSpaceImport` prints the EARLIER alias's name for a
    // later one; `compiler/unusedImports_entireImportDeclaration` prints each of
    // `ns`, `ns2`, `ns3` under its OWN name. Both are two-alias scopes and they
    // disagree, so no tie-break reproduces both — upstream distinguishes them
    // with `cloneTypeAsModuleType`, which this port does not have.
    //
    // Answering `None` keeps those lines gapping. That is the property that
    // makes this arm safe, and it is why the entry point returns an `Option`
    // rather than falling back to the baked file-path text.
    let arena = Arena::new();
    let fixture = program(
        &arena,
        &[
            ("m", "export const v: string = \"a\";\n"),
            ("b", "import * as one from \"./m\";\nimport * as two from \"./m\";\n"),
        ],
    );
    assert_eq!(rendered_at(&fixture, "one", SyntaxKind::NamespaceImport), "error");
    assert_eq!(rendered_at(&fixture, "two", SyntaxKind::NamespaceImport), "error");
}

#[test]
fn a_module_writing_export_equals_resolves_through_the_assignment() {
    // This fixture pinned the export= REFUSAL for two builds ("shipping it
    // would be unmeasured surface") — the fourteenth unported-stand-in
    // fixture to come due. The surface is now measured
    // (`checker-notes-modobj.md` §10.8: 302 seed converts / 23 would-wrong)
    // and the chain is built: `resolveExternalModuleSymbol`
    // (`checker.go:15556`) hands back the export= alias, whose
    // `ExportAssignment` arm (`getTargetOfExportAssignment`,
    // `checker.go:14889`) resolves the written identifier. Baseline shape:
    // `conformance/exportAssignTypes.types` records `>iValue : number` for
    // exactly this pair of files.
    let arena = Arena::new();
    let fixture = program(
        &arena,
        &[
            ("m", "declare const v: string;\nexport = v;\n"),
            ("b", "import mod = require(\"./m\");\n"),
        ],
    );
    assert_eq!(rendered_at(&fixture, "mod", SyntaxKind::ImportEqualsDeclaration), "string");
}

#[test]
fn a_non_module_type_is_rendered_exactly_as_before() {
    // The additive property. `type_to_string_at` must be the identity on every
    // type that is not a module object, or the 110 call sites of
    // `type_to_string` and this one would disagree about the same type.
    let arena = Arena::new();
    let fixture = program(&arena, &[("m", "export const v: string = \"a\";\n")]);
    let mut checker = Checker::with_module_host(
        &fixture.bound,
        &fixture.nodes,
        &fixture.node_map,
        Some(&fixture.host),
    );
    let declaration = (0..u32::try_from(fixture.nodes.len()).expect("fits"))
        .map(NodeId::new)
        .find(|&id| fixture.nodes.kind(id) == SyntaxKind::VariableDeclaration)
        .expect("the fixture declares a variable");
    let symbol = fixture.bound.symbol_of(declaration).expect("bound");
    let id = checker.get_type_of_symbol(symbol);
    let plain = checker.type_to_string(id);
    assert_eq!(plain, "string");
    assert_eq!(checker.type_to_string_at(id, declaration), Some(plain));
}

#[test]
fn a_namespace_declaration_keeps_its_declared_name() {
    // `SymbolFlags::VALUE_MODULE` is carried by `namespace N {}` as well as by a
    // file's module symbol, so the module test is "one of its declarations is a
    // SourceFile" rather than a flag test. A flag test would send every
    // namespace through the alias lookup and gap all of them.
    let arena = Arena::new();
    let fixture = program(
        &arena,
        &[("m", "namespace N { export const v: string = \"a\"; }\nconst q = N;\n")],
    );
    let mut checker = Checker::with_module_host(
        &fixture.bound,
        &fixture.nodes,
        &fixture.node_map,
        Some(&fixture.host),
    );
    let declaration = (0..u32::try_from(fixture.nodes.len()).expect("fits"))
        .map(NodeId::new)
        .find(|&id| fixture.nodes.kind(id) == SyntaxKind::ModuleDeclaration)
        .expect("the fixture declares a namespace");
    let symbol = fixture.bound.symbol_of(declaration).expect("bound");
    let id = checker.get_type_of_symbol(symbol);
    assert_eq!(checker.type_to_string_at(id, declaration), Some("typeof N".to_string()));
}

#[test]
fn an_ambient_module_resolves_and_prints_the_alias_name() {
    // `tryFindAmbientModule` (`checker.go:15533`): `declare module 'm'` is a
    // resolution target consulted before the host, and the resolved module
    // object is named at the reference site like any other. Pinned to
    // `compiler/privacyTopLevelAmbientExternalModuleImportWithoutExport.types:7-8`:
    //
    // ```text
    // import im_private_mi_private = require("m");
    // >im_private_mi_private : typeof im_private_mi_private
    // ```
    //
    // Note the host below has no file for `"m"` at all — the ambient arm is
    // the only route.
    let arena = Arena::new();
    let fixture = program(
        &arena,
        &[
            ("decl", "declare module 'm' { export class c_private { baz: string } }\n"),
            ("core", "import a = require(\"m\");\n"),
        ],
    );
    assert_eq!(rendered_at(&fixture, "a", SyntaxKind::ImportEqualsDeclaration), "typeof a");
}

#[test]
fn an_ordinary_global_sharing_the_specifier_name_is_not_a_module() {
    // Upstream keys ambient modules in `globals` under the QUOTED name, so a
    // plain `namespace m {}` can never be found by `tryFindAmbientModule`.
    // This binder stores ambient module names unquoted
    // (`crates/tsr-binder/src/binder.rs:4091`), so the selection is recovered
    // from the declaration's shape instead — and this is the fixture that
    // reddens if that shape test is dropped: `m` here is a global namespace,
    // not a `declare module`, and the import must stay a gap.
    let arena = Arena::new();
    let fixture = program(
        &arena,
        &[
            ("decl", "namespace m { export class c { baz: string } }\n"),
            ("core", "import a = require(\"m\");\n"),
        ],
    );
    assert_eq!(rendered_at(&fixture, "a", SyntaxKind::ImportEqualsDeclaration), "error");
}

#[test]
fn an_ambient_module_named_by_two_aliases_is_still_a_gap() {
    // The ambiguity refusal from `c91314c` is form-independent: an ambient
    // module reached by the new arm flows through the same
    // `Checker::module_name_at`, so two in-scope aliases keep the line a gap
    // rather than a guessed name — same property
    // `a_module_named_by_two_aliases_is_a_gap` pins for file modules.
    let arena = Arena::new();
    let fixture = program(
        &arena,
        &[
            ("decl", "declare module 'm' { export const v: string }\n"),
            ("core", "import a = require(\"m\");\nimport b = require(\"m\");\n"),
        ],
    );
    assert_eq!(rendered_at(&fixture, "a", SyntaxKind::ImportEqualsDeclaration), "error");
    assert_eq!(rendered_at(&fixture, "b", SyntaxKind::ImportEqualsDeclaration), "error");
}

/// Render the type of the variable named `variable`, through the rendering
/// path, at the variable's own declaration — the position the `.types`
/// baseline records for a `var x = …` line.
fn variable_rendered_at(fixture: &Fixture<'_>, variable: &str) -> String {
    let host: Option<&dyn ModuleHost> = Some(&fixture.host);
    let mut checker =
        Checker::with_module_host(&fixture.bound, &fixture.nodes, &fixture.node_map, host);
    let declaration = (0..u32::try_from(fixture.nodes.len()).expect("fits"))
        .map(NodeId::new)
        .filter(|&id| fixture.nodes.kind(id) == SyntaxKind::VariableDeclaration)
        .find(|&id| {
            fixture
                .bound
                .symbol_of(id)
                .is_some_and(|s| fixture.bound.symbols().get(s).name == variable)
        })
        .unwrap_or_else(|| panic!("no variable named `{variable}`"));
    let symbol = fixture.bound.symbol_of(declaration).expect("bound");
    let id = checker.get_type_of_symbol(symbol);
    checker.type_to_string_at(id, declaration).unwrap_or_else(|| "error".to_string())
}

#[test]
fn a_member_of_an_ambient_module_qualifies_with_the_container_alias() {
    // The container-qualifier slice's alias arm
    // (`checker-notes-modobj.md` §10.6). Pinned to
    // `compiler/privacyTopLevelAmbientExternalModuleImportWithoutExport.types:24-25`:
    //
    // ```text
    // var privateUse_im_private_mi_private = new im_private_mi_private.c_private();
    // >privateUse_im_private_mi_private : im_private_mi_private.c_private
    // ```
    //
    // The class's bare name does not resolve at the site; its container is the
    // ambient module; the unique in-scope alias `a` names that container.
    let arena = Arena::new();
    let fixture = program(
        &arena,
        &[
            ("decl", "declare module 'm' { export class c_private { baz: string } }\n"),
            ("core", "import a = require(\"m\");\nvar use = new a.c_private();\n"),
        ],
    );
    assert_eq!(variable_rendered_at(&fixture, "use"), "a.c_private");
}

#[test]
fn a_member_of_an_ambient_module_with_no_alias_prints_the_import_form() {
    // The ambient-import arm — `getSpecifierForModuleSymbol`'s exact branch
    // (`nodebuilderimpl.go:1260`). Pinned to
    // `compiler/privacyCannotNameVarTypeDeclFile.types`, which records
    // `import("GlobalWidgets").Widget3` for a widget reached through a
    // *different* file's import than the reference site's own. Reduced here:
    // the value's type crosses files, and at the referencing site no alias of
    // `GlobalWidgets` is in scope.
    let arena = Arena::new();
    let fixture = program(
        &arena,
        &[
            ("decl", "declare module 'GlobalWidgets' { export class Widget3 { name: string } }\n"),
            (
                "exporter",
                "import Widgets = require(\"GlobalWidgets\");\nexport var w3 = new Widgets.Widget3();\n",
            ),
            ("core", "import exporter = require(\"./exporter\");\nvar w = exporter.w3;\n"),
        ],
    );
    assert_eq!(variable_rendered_at(&fixture, "w"), "import(\"GlobalWidgets\").Widget3");
}

#[test]
fn an_export_equals_namespace_prints_the_importing_alias_name() {
    // The RENAME (`checker-notes-modobj.md` §10.8). Pinned to the React shape
    // every tsx baseline records — `react.d.ts` writes
    // `declare namespace __React {…} declare module "react" { export = __React }`
    // and `conformance/tsxUnionElementType3.types:…` records
    // `>React : typeof React`, never `typeof __React`: the innermost table
    // (the importing file's locals) reaches the symbol through the alias
    // before any table holds `__React` directly.
    let arena = Arena::new();
    let fixture = program(
        &arena,
        &[
            (
                "decl",
                "declare namespace __X { export class C { x: string } }\ndeclare module 'm' { export = __X }\n",
            ),
            ("core", "import X = require(\"m\");\n"),
        ],
    );
    assert_eq!(rendered_at(&fixture, "X", SyntaxKind::ImportEqualsDeclaration), "typeof X");
}
