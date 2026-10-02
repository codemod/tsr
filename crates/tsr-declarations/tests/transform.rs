//! The transform, one construct at a time.
//!
//! # Why these exist when the corpus already covers 96% of the transform
//!
//! `docs/conventions.md`: *tests that depend on the submodule skip, not fail*. So
//! a checkout without `vendor/typescript-go` runs the unit suite alone — and
//! measured with `cargo llvm-cov`, that suite reached **51.59%** of
//! `transform.rs`. The 96% is real, and it is not what a contributor sees before
//! pushing, nor what CI sees when the submodule is unavailable.
//!
//! These are therefore aimed at the arms the *corpus* reaches and the *unit suite*
//! did not: namespaces and the three-way scope-marker choice, accessors, parameter
//! properties, private members, and the import elision rules. Each asserts the
//! emitted text, because that is what ships.
//!
//! They are also the regression net for findings the corpus can only see
//! indirectly. Every case below marked with an upstream anchor corresponds to a
//! real defect found during slice 4 — the elided namespace bodies, the dropped
//! side-effect import, the spurious scope marker — and each of those printed
//! output that *parsed*, which is why a structural gate did not catch them.

use tsr_core::Arena;

/// Emit `source` and return the `.d.ts` text.
fn emit(source: &str) -> String {
    let arena = Arena::new();
    let parsed = tsr_parser::parse(&arena, source);
    assert!(
        parsed.diagnostics.is_empty(),
        "test source must parse cleanly: {source:?} — {:?}",
        parsed.diagnostics.first().map(tsr_diagnostics::Diagnostic::text)
    );
    let mut nodes = parsed.nodes;
    let result = tsr_declarations::emit(&arena, &mut nodes, parsed.source_file);
    assert!(result.unsupported.is_empty(), "printer gap {:?} in {source:?}", result.unsupported);
    result.text
}

fn emit_stripping_internal(source: &str) -> String {
    let arena = Arena::new();
    let parsed = tsr_parser::parse(&arena, source);
    assert!(parsed.diagnostics.is_empty(), "test source must parse cleanly: {source:?}");
    let mut nodes = parsed.nodes;
    tsr_declarations::emit_with_options(
        &arena,
        &mut nodes,
        parsed.source_file,
        tsr_declarations::DeclarationEmitOptions {
            source_text: Some(source),
            strip_internal: true,
            remove_comments: false,
            strict_null_checks: false,
            force_module: false,
            source_map_url: None,
        },
    )
    .text
}

#[test]
fn preserved_references_precede_declarations_in_source_order() {
    use tsr_declarations::{
        DeclarationReference, DeclarationReferenceKind, DeclarationResolutionMode,
    };

    let arena = Arena::new();
    let parsed = tsr_parser::parse(&arena, "export const x: number = 1;");
    let mut nodes = parsed.nodes;
    let references = [
        DeclarationReference {
            kind: DeclarationReferenceKind::Lib,
            file_name: "dom".into(),
            resolution_mode: DeclarationResolutionMode::None,
            position: 20,
        },
        DeclarationReference {
            kind: DeclarationReferenceKind::Path,
            file_name: "./a&b.ts".into(),
            resolution_mode: DeclarationResolutionMode::None,
            position: 10,
        },
        DeclarationReference {
            kind: DeclarationReferenceKind::Types,
            file_name: "node".into(),
            resolution_mode: DeclarationResolutionMode::Require,
            position: 30,
        },
    ];
    let result =
        tsr_declarations::emit_with_references(&arena, &mut nodes, parsed.source_file, &references);

    assert_eq!(
        result.text,
        "/// <reference path=\"a&amp;b.d.ts\" preserve=\"true\" />\n\
         /// <reference lib=\"dom\" preserve=\"true\" />\n\
         /// <reference types=\"node\" resolution-mode=\"require\" preserve=\"true\" />\n\
         export declare const x: number;"
    );
}

/// Emit with the original text available, as the conformance harness does.
fn emit_with_source(source: &str) -> String {
    let arena = Arena::new();
    let parsed = tsr_parser::parse(&arena, source);
    assert!(parsed.diagnostics.is_empty(), "test source must parse cleanly: {source:?}");
    let mut nodes = parsed.nodes;
    tsr_declarations::emit_with_options(
        &arena,
        &mut nodes,
        parsed.source_file,
        tsr_declarations::DeclarationEmitOptions {
            source_text: Some(source),
            strip_internal: false,
            remove_comments: false,
            strict_null_checks: false,
            force_module: false,
            source_map_url: None,
        },
    )
    .text
}

#[test]
fn a_forced_module_drops_unexported_declarations() {
    // moduleDetectionIsolatedModulesCjsFileScope: a `.cts`/`.mts` extension
    // marks the file a module with no import/export syntax, so its private
    // declarations drop and only the module marker remains.
    let arena = Arena::new();
    let source = "const a = 2;";
    let parsed = tsr_parser::parse(&arena, source);
    assert!(parsed.diagnostics.is_empty());
    let mut nodes = parsed.nodes;
    let result = tsr_declarations::emit_with_options(
        &arena,
        &mut nodes,
        parsed.source_file,
        tsr_declarations::DeclarationEmitOptions {
            source_text: Some(source),
            strip_internal: false,
            remove_comments: false,
            strict_null_checks: true,
            force_module: true,
            source_map_url: None,
        },
    );
    assert_eq!(result.text, "export {};");
}

#[test]
fn jsdoc_accessibility_tags_become_modifiers_in_javascript_files() {
    // `lateBoundAssignmentCandidateJS3`: `@protected`/`@private` JSDoc acts as
    // a modifier in a JavaScript file; a TypeScript file ignores it.
    let source = "export class C {\n    /** @protected @type {string} */\n    a = 'x';\n    /** @private */\n    b = 1;\n}";
    let emit_as = |javascript: bool| {
        let arena = Arena::new();
        let parsed = tsr_parser::parse(&arena, source);
        assert!(parsed.diagnostics.is_empty());
        let mut nodes = parsed.nodes;
        if javascript && let Some(root) = parsed.source_file.node_id {
            nodes.add_flags(root, tsr_ast::NodeFlags::JAVASCRIPT_FILE);
        }
        tsr_declarations::emit_with_options(
            &arena,
            &mut nodes,
            parsed.source_file,
            tsr_declarations::DeclarationEmitOptions {
                source_text: Some(source),
                strip_internal: false,
                remove_comments: false,
                strict_null_checks: true,
                force_module: false,
                source_map_url: None,
            },
        )
        .text
    };
    let javascript = emit_as(true);
    assert!(javascript.contains("protected a: string;"), "{javascript}");
    assert!(javascript.contains("private b;"), "{javascript}");
    let typescript = emit_as(false);
    // The tag stays comment text in TypeScript: no `protected` modifier.
    assert!(typescript.contains("\n    a: string;"), "{typescript}");
    assert!(typescript.contains("\n    b: number;"), "{typescript}");
}

/// Emit a JavaScript file: parse, stamp the root flag, emit with source text.
fn emit_javascript(source: &str) -> String {
    let arena = Arena::new();
    let parsed = tsr_parser::parse(&arena, source);
    assert!(parsed.diagnostics.is_empty(), "test source must parse cleanly: {source:?}");
    let mut nodes = parsed.nodes;
    if let Some(root) = parsed.source_file.node_id {
        nodes.add_flags(root, tsr_ast::NodeFlags::JAVASCRIPT_FILE);
    }
    tsr_declarations::emit_with_options(
        &arena,
        &mut nodes,
        parsed.source_file,
        tsr_declarations::DeclarationEmitOptions {
            source_text: Some(source),
            strip_internal: false,
            remove_comments: false,
            strict_null_checks: true,
            force_module: false,
            source_map_url: None,
        },
    )
    .text
}

#[test]
fn js_constructor_this_assignments_declare_typed_properties() {
    // `argumentsReferenceInConstructor1_Js`: a `@type` JSDoc types the member,
    // `@param {T} [name]` types the parameter, and the synthesized property
    // precedes the constructor carrying the assignment's JSDoc.
    let output = emit_javascript(
        "class A {\n    /**\n     * @param {object} [foo={}]\n     */\n    constructor(foo = {}) {\n        /**\n         * @type object\n         */\n        this.arguments = foo;\n        this.count = 1;\n    }\n}",
    );
    assert!(output.contains("arguments: object;"), "{output}");
    assert!(output.contains("count: number;"), "{output}");
    assert!(output.contains("constructor(foo?: object);"), "{output}");
}

#[test]
fn a_rich_jsdoc_type_is_parsed_into_the_declaration_tree() {
    // Union, generic, and function types come from a real parse grafted into
    // the transform's own arena and node table.
    let output = emit_javascript(
        "class A {\n    /** @param {Map<string, number[]>} m */\n    constructor(m) {\n        /** @type {string | number} */\n        this.x = m;\n        /** @type {(a: string) => void} */\n        this.f = m;\n    }\n}",
    );
    assert!(output.contains("x: string | number;"), "{output}");
    assert!(output.contains("f: (a: string) => void;"), "{output}");
    assert!(output.contains("constructor(m: Map<string, number[]>);"), "{output}");
}

#[test]
fn jsdoc_typedef_and_callback_synthesize_type_aliases() {
    // `typedefOnSemicolonClassElement` / `callbackOnConstructor`: aliases are
    // hoisted before the statement containing their comment, exported in a
    // module, with `@template` tags as type parameters.
    let output = emit_javascript(
        "export class P {\n    /** @typedef {string} A */\n    ;\n    /** @type {A} */\n    a = 'ok';\n    /**\n     * @callback Get\n     * @param {string} name\n     * @returns {boolean|number}\n     */\n    constructor() {}\n}",
    );
    assert!(output.starts_with("export type A = string;"), "{output}");
    assert!(output.contains("export type Get = (name: string) => boolean | number;"), "{output}");
    assert!(output.contains("a: A;"), "{output}");

    let generic = emit_javascript(
        "/**\n * @template T\n * @template {keyof T} K\n * @typedef {T[K]} Foo\n */\nconst x = 1;\nexport { x };",
    );
    assert!(generic.contains("export type Foo<T, K extends keyof T> = T[K];"), "{generic}");
}

#[test]
fn nested_assignments_on_an_empty_object_const_build_its_type() {
    // `typeFromPropertyAssignment39`: property and element assignments on an
    // empty-object const spell a nested type literal in JavaScript.
    let output =
        emit_javascript("const foo = {};\nfoo[\"baz\"] = {};\nfoo[\"baz\"][\"blah\"] = 3;");
    assert_eq!(
        output.trim_end(),
        "declare const foo: {\n    baz: {\n        blah: number;\n    };\n};"
    );
}

#[test]
fn a_dotted_typedef_name_declares_a_namespace_member() {
    // `jsDeclarationsImportNamespacedType`: `@typedef {number} Dotted.Name`
    // wraps in `export declare namespace Dotted { export type Name }`, and
    // the declaring comment stays on its host statement.
    let output = emit_javascript("/** @typedef {number} Dotted.Name */\nexport var dummy = 1;");
    assert!(
        output
            .starts_with("export declare namespace Dotted {\n    export type Name = number;\n}\n"),
        "{output}"
    );
    assert!(
        output.contains("/** @typedef {number} Dotted.Name */\nexport declare var dummy"),
        "{output}"
    );
}

#[test]
fn jsdoc_only_type_spellings_map_to_typescript() {
    // `jsDeclarationsReusesExistingNodesMappingJSDocTypes`.
    let output = emit_javascript(
        "/** @type {?} */\nexport const a = null;\n/** @type {string?} */\nexport const c = null;\n/** @type {string=} */\nexport const d = null;\n/** @type {string!} */\nexport const e = null;\n/** @type {function(string): object} */\nexport const f = null;\n/** @type {Object.<string, number>} */\nexport const h = null;",
    );
    assert!(output.contains("a: any | null;"), "{output}");
    assert!(output.contains("c: string | null;"), "{output}");
    assert!(output.contains("d: string | undefined;"), "{output}");
    assert!(output.contains("e: string;"), "{output}");
    assert!(output.contains("f: Function;"), "{output}");
    assert!(output.contains("h: Record<string, number>;"), "{output}");
}

#[test]
fn a_binding_pattern_destructuring_an_entity_keeps_its_shape() {
    // `declarationEmitExpressionInExtends6`: the pattern stays, typed by
    // `typeof` the destructured entity, and its bound names participate in
    // reachability.
    let output = emit_with_source(
        "import * as A from \"./a\";\nconst { Foo } = A;\nexport default class extends Foo {\n}",
    );
    assert!(output.contains("import * as A from \"./a\";"), "{output}");
    assert!(output.contains("declare const { Foo }: typeof A;"), "{output}");
}

#[test]
fn a_qualified_name_root_is_not_shadowed_by_a_type_parameter() {
    // `declarationEmitRetainedAnnotationRetainsImportInOutput`: `E.Whatever`
    // resolves `E` in namespace space, so `<E>` must not drop the import.
    let output = emit_with_source(
        "import * as E from 'whatever';\nexport const run = <E,>(i: () => E.Whatever<E>): E.Whatever<E> => i();",
    );
    assert!(output.contains("import * as E from 'whatever';"), "{output}");
}

#[test]
fn a_jsdoc_implements_tag_becomes_a_heritage_clause() {
    // `jsdocImplements_properties`: braced, bare, and comment-closing forms.
    let output = emit_javascript("class A {}\n/** @implements A*/\nclass B {}");
    assert!(output.contains("declare class B implements A {"), "{output}");
}

/// Assert the emitted text exactly, so spacing and ordering are covered too.
#[track_caller]
fn assert_emits(source: &str, expected: &str) {
    assert_eq!(emit(source).trim_end(), expected.trim_end(), "\nfrom: {source}");
}

// ----- `declare`, and where it must not go ---------------------------------

#[test]
fn a_top_level_declaration_gains_declare_and_an_interface_does_not() {
    // `ensureModifierFlags` + `isAlwaysType` (`transform.go:2333`, `util.go`). An
    // interface has no runtime existence to declare.
    assert_emits("export declare const a: number;", "export declare const a: number;");
    assert_emits("export const a: number = 1;", "export declare const a: number;");
    assert_emits(
        "export interface I {\n    a: number;\n}",
        "export interface I {\n    a: number;\n}",
    );
    assert_emits("export type T = number;", "export type T = number;");
}

#[test]
fn a_declaration_inside_a_namespace_does_not_gain_declare() {
    // `ensureModifierFlags`'s `parentIsFile` branch: inside a namespace body the
    // mask clears `AMBIENT` outright, because the enclosing `declare` covers it.
    // `declare namespace M { declare const x: number; }` does not parse.
    assert_emits(
        "export namespace M {\n    export const x: number = 1;\n}",
        "export declare namespace M {\n    const x: number;\n}",
    );
}

#[test]
fn export_default_keeps_its_export_and_never_gains_declare() {
    // `maskModifierFlags`'s two corrections (`util.go`): a non-exported `default`
    // is a nonsequitur, and `declare` beside `default` is an error rather than a
    // redundancy.
    let text = emit("export default class C {\n    x: number = 1;\n}");
    assert!(text.contains("export default class C"), "{text}");
    assert!(!text.contains("declare"), "declare must not appear beside default:\n{text}");
}

#[test]
fn default_export_expression_gets_a_collision_free_binding() {
    let plain = emit("export default 1 + 2;");
    assert!(plain.contains("declare const _default"), "{plain}");
    assert!(plain.contains("export default _default;"), "{plain}");

    let collision = emit("const _default: number = 1; export default 2 + 3;");
    assert!(collision.contains("declare const _default_1"), "{collision}");
    assert!(collision.contains("export default _default_1;"), "{collision}");

    // A default-exported *literal* keeps its value as the synthesized const's
    // initializer instead of widening (`modulePreserve4`).
    assert_emits("export default 0;", "declare const _default = 0;\nexport default _default;");
    assert_emits(
        "export default 'a';",
        "declare const _default = \"a\";\nexport default _default;",
    );

    let export_equals = emit("export = { answer: 42 };");
    assert!(export_equals.contains("declare const _default"), "{export_equals}");
    assert!(export_equals.contains("export = _default;"), "{export_equals}");
}

#[test]
fn strict_null_checks_controls_null_widening() {
    let source = "export default null;";
    let emit_with_strict_null_checks = |strict_null_checks| {
        let arena = Arena::new();
        let parsed = tsr_parser::parse(&arena, source);
        let mut nodes = parsed.nodes;
        tsr_declarations::emit_with_options(
            &arena,
            &mut nodes,
            parsed.source_file,
            tsr_declarations::DeclarationEmitOptions {
                source_text: Some(source),
                strip_internal: false,
                remove_comments: false,
                strict_null_checks,
                force_module: false,
                source_map_url: None,
            },
        )
        .text
    };

    assert!(emit_with_strict_null_checks(true).contains("declare const _default: null;"));
    assert!(emit_with_strict_null_checks(false).contains("declare const _default: any;"));
}

// ----- namespaces, and the three-way scope-marker choice -------------------

#[test]
fn a_namespace_body_survives() {
    // The defect this is a regression test for: `is_declaration_visible` had no
    // opinion about nested declarations and the transform read that as "not
    // visible", emptying **every** namespace body in the corpus. The output —
    // `declare namespace M {}` — parses, so only a byte comparison saw it.
    assert_emits(
        "namespace M {\n    export interface I {\n        a: number;\n    }\n}",
        "declare namespace M {\n    interface I {\n        a: number;\n    }\n}",
    );
}

#[test]
fn source_namespace_elides_private_aliases_after_member_transformation() {
    assert_emits(
        "namespace N {\n    import Internal = M.Internal;\n    export class C { private value: Internal.Value; }\n}",
        "declare namespace N {\n    class C {\n        private value;\n    }\n}",
    );
    assert_emits(
        "namespace N {\n    import Internal = M.Internal;\n    export interface I { value: Internal.Value; }\n}",
        "declare namespace N {\n    import Internal = M.Internal;\n    interface I {\n        value: Internal.Value;\n    }\n}",
    );
}

#[test]
fn an_ambient_namespace_needs_no_scope_marker() {
    // `transform.go:1846`. Everything in a `declare namespace` is exported already.
    // Upstream tests `NodeFlagsAmbient`, which this parser never sets
    // (`bd tsr-qc3`), so the transform tests the `declare` modifier too — this is
    // the test that pins that workaround.
    let text = emit("declare namespace M {\n    interface I {\n    }\n}");
    assert!(!text.contains("export {}"), "an ambient namespace gained a scope marker:\n{text}");
}

#[test]
fn ambient_context_is_inherited_by_nested_namespaces() {
    assert_emits(
        "declare module \"pkg\" {\n    namespace A {\n        class C {}\n    }\n}",
        "declare module \"pkg\" {\n    namespace A {\n        class C {\n        }\n    }\n}\n",
    );
}

#[test]
fn a_namespace_whose_members_are_all_exported_has_its_export_modifiers_stripped() {
    // `transformModuleDeclaration`'s second branch (`:1849`): everything in an
    // ambient namespace is exported, so restating `export` is noise upstream does
    // not emit.
    assert_emits(
        "namespace M {\n    export const a: number = 1;\n    export const b: string = \"\";\n}",
        "declare namespace M {\n    const a: number;\n    const b: string;\n}",
    );
}

#[test]
fn a_dotted_namespace_emits_as_one_header() {
    // `namespace A.B {}` is two nested `ModuleDeclaration`s in the tree, and the
    // printer restates only the inner *name* — writing both headers produced
    // `namespace A. export namespace B {}` (`docs/architecture/printer.md`). The
    // emitted text is one dotted header, which is what upstream writes.
    assert_emits(
        "namespace A.B {\n    export const x: number = 1;\n}",
        "declare namespace A.B {\n    const x: number;\n}",
    );
}

// ----- the file-level scope marker -----------------------------------------

#[test]
fn external_module_elides_private_dependencies_after_member_transformation() {
    assert_emits(
        "class Hidden {}\nexport class Public { private value: Hidden; }",
        "export declare class Public {\n    private value;\n}",
    );
    assert_emits(
        "class Hidden {}\nexport interface Public { value: Hidden; }",
        "declare class Hidden {\n}\nexport interface Public {\n    value: Hidden;\n}\nexport {};",
    );
}

#[test]
fn external_module_post_transform_visibility_preserves_side_effect_imports() {
    assert_emits(
        "import \"./polyfill\";\nclass Hidden {}\nexport interface Public {}",
        "import \"./polyfill\";\nexport interface Public {\n}",
    );
}

#[test]
fn a_module_that_would_lose_its_moduleness_gains_an_export_marker() {
    // `transformSourceFile`'s scope-marker block. Without the marker the emitted
    // declarations would leak into the global scope — a change of meaning, not of
    // spelling.
    let text = emit("import { x } from \"./m\";\nconst hidden: number = 1;\n");
    assert_eq!(text.trim_end(), "export {};", "\ngot:\n{text}");
}

#[test]
fn a_script_gains_no_marker() {
    // Not a module: there is no export list to be reachable from, and nothing to
    // preserve.
    let text = emit("const a: number = 1;\n");
    assert!(!text.contains("export {}"), "a script gained a scope marker:\n{text}");
}

#[test]
fn the_empty_export_marker_has_no_space_in_it() {
    // `LFNamedImportsOrExportsElements` carries `LFNoSpaceIfEmpty`. Written by hand
    // at the call site instead of through `emit_list`, this printed `export {  };`
    // — the most common line in a `.d.ts`, wrong in every file that had one.
    let text = emit("import { x } from \"./m\";\nconst hidden: number = 1;\n");
    assert!(text.contains("export {};"), "expected `export {{}};`, got:\n{text}");
    assert!(!text.contains("export {  }"), "{text}");
}

// ----- imports --------------------------------------------------------------

#[test]
fn an_unreferenced_import_is_elided_and_a_referenced_one_is_not() {
    // `transformImportDeclaration` (`:2471`): upstream keeps an import only when
    // something in the output still refers to it.
    let unused = emit("import { T } from \"./m\";\nexport const a: number = 1;\n");
    assert!(!unused.contains("import"), "an unreferenced import survived:\n{unused}");

    let used = emit("import { T } from \"./m\";\nexport const a: T = 1 as T;\n");
    assert!(
        used.contains("import { T } from \"./m\";"),
        "a referenced import was dropped:\n{used}"
    );
}

#[test]
fn an_import_used_by_a_written_arrow_signature_is_retained() {
    assert_emits(
        "import { T } from \"./m\";\nexport const f = (value: T): void => {};\n",
        "import { T } from \"./m\";\nexport declare const f: (value: T) => void;\n",
    );
}

#[test]
fn mapped_types_in_synthesized_arrow_signatures_stay_single_line() {
    assert_emits(
        "export const map = <T>(value: T): { [K in keyof T]: T[K] } => ({} as any);",
        "export declare const map: <T>(value: T) => { [K in keyof T]: T[K]; };",
    );
}

#[test]
fn a_missing_mapped_value_type_becomes_any_in_declaration_output() {
    assert_emits(
        "type T<U> = ({ [K in keyof U] }) extends ({ [P in keyof U]: U[P] }) ? 1 : 0;",
        "type T<U> = ({\n    [K in keyof U]: any;\n}) extends ({\n    [P in keyof U]: U[P];\n}) ? 1 : 0;",
    );
}

#[test]
fn an_empty_namespace_body_keeps_its_source_brace_layout() {
    // moduleSymbolMerging: a body whose braces sat on one line prints `{ }`
    // even when its statements were filtered away; declareDottedModuleName: a
    // body whose source braces span lines stays multiline.
    let output = emit_with_source("namespace A { ; }\nnamespace B {\n}\nnamespace C.D { }");
    assert_eq!(
        output,
        "declare namespace A { }\ndeclare namespace B {\n}\ndeclare namespace C.D { }"
    );
}

#[test]
fn a_source_declare_modifier_is_dropped_inside_a_namespace_body() {
    // `ensureModifierFlags` (`transform.go:2333`): `declare` is masked out
    // whenever the parent is not the source file — the enclosing namespace's
    // own `declare` already covers its body.
    assert_emits(
        "export namespace M { export declare var v: number; export var w: string; }",
        "export declare namespace M {\n    var v: number;\n    var w: string;\n}",
    );
}

#[test]
fn an_invisible_declarator_is_pruned_from_a_retained_var_statement() {
    // `getBindingNameVisible` (`transform.go:2216`): `export = m2` reaches
    // `m2` and not `x`, so only `m2` survives from the shared statement.
    assert_emits("var x = 10, m2: number;\nexport = m2;", "declare var m2: number;\nexport = m2;");
}

#[test]
fn a_declaration_map_url_is_appended_after_the_output() {
    let arena = Arena::new();
    let source = "export const a: number = 1;";
    let parsed = tsr_parser::parse(&arena, source);
    assert!(parsed.diagnostics.is_empty());
    let mut nodes = parsed.nodes;
    let result = tsr_declarations::emit_with_options(
        &arena,
        &mut nodes,
        parsed.source_file,
        tsr_declarations::DeclarationEmitOptions {
            source_text: Some(source),
            strip_internal: false,
            remove_comments: false,
            strict_null_checks: true,
            force_module: false,
            source_map_url: Some("a.d.ts.map"),
        },
    );
    assert_eq!(result.text, "export declare const a: number;\n//# sourceMappingURL=a.d.ts.map");
}

#[test]
fn a_deferred_import_becomes_an_ordinary_declaration_import() {
    assert_emits(
        "import defer * as ns from \"./m.js\";\nexport type T = ns.Value;\n",
        "import * as ns from \"./m.js\";\nexport type T = ns.Value;\n",
    );
}

#[test]
fn an_arrow_type_parameter_does_not_retain_a_shadowed_import() {
    assert_emits(
        "import * as T from \"./m\";\nexport const identity = <T>(value: T): T => value;\n",
        "export declare const identity: <T>(value: T) => T;\n",
    );
}

#[test]
fn a_side_effect_import_is_never_elided() {
    // It binds no name, so reachability has nothing to say about it — and dropping
    // it changes what an importer of the `.d.ts` loads. `transform.go:2474`.
    let text = emit("import \"./polyfill\";\nexport const a: number = 1;\n");
    assert!(text.contains("import \"./polyfill\";"), "a side-effect import was dropped:\n{text}");
}

#[test]
fn a_module_augmentation_and_its_type_import_are_retained() {
    assert_emits(
        "import { T } from \"./m\";\ndeclare global { interface Window { value: T; } }\n",
        "import { T } from \"./m\";\ndeclare global {\n    interface Window {\n        value: T;\n    }\n}\n",
    );
}

#[test]
fn an_augmentation_declaration_does_not_retain_a_shadowed_import() {
    assert_emits(
        "import { Observable } from \"./observable\";\n\
         declare module \"./observable\" {\n\
             interface Observable<T> { map<U>(value: T): Observable<U>; }\n\
         }",
        "declare module \"./observable\" {\n    interface Observable<T> {\n        map<U>(value: T): Observable<U>;\n    }\n}\nexport {};",
    );
}

#[test]
fn an_augmentation_retains_only_genuine_external_type_imports() {
    assert_emits(
        "import { A } from \"./f1\";\n\
         import { B } from \"./f2\";\n\
         declare module \"./f1\" { interface A { foo(): B; } }",
        "import { B } from \"./f2\";\ndeclare module \"./f1\" {\n    interface A {\n        foo(): B;\n    }\n}",
    );
}

#[test]
fn every_declaration_in_a_merged_symbol_is_retained_in_source_order() {
    assert_emits(
        "function f(): void {}\nnamespace f { export const x: number = 1; }\nexport { f };\n",
        "declare function f(): void;\ndeclare namespace f {\n    const x: number;\n}\nexport { f };\n",
    );
}

#[test]
fn override_is_removed_from_interface_method_signatures() {
    assert_emits(
        "export interface I { override method(): void; }",
        "export interface I {\n    method(): void;\n}\n",
    );
}

// ----- class members --------------------------------------------------------

#[test]
fn a_private_member_keeps_its_name_and_loses_its_type() {
    // `omitPrivateMethodType` (`:1090`) and `ensureType`'s private guard (`:1630`):
    // a private member is not part of the class's public shape, but its *presence*
    // is, because it makes the class nominally distinct.
    assert_emits(
        "export class C {\n    private a: number = 1;\n    private m(x: number): void {}\n}",
        "export declare class C {\n    private a;\n    private m;\n}",
    );
}

#[test]
fn overload_implementations_are_omitted_and_private_methods_have_one_marker() {
    assert_emits(
        "class C {\n    private method(x: number): void;\n    private method(x: string): void;\n    private method(x: unknown): void {}\n    constructor(x: number);\n    constructor(x: number) {}\n}",
        "declare class C {\n    private method;\n    constructor(x: number);\n}\n",
    );
}

#[test]
fn initialized_parameter_before_required_parameter_includes_undefined() {
    assert_emits(
        "export class C { constructor(public values: number[] = [], count: number) {} }",
        "export declare class C {\n    values: number[];\n    constructor(values: number[] | undefined, count: number);\n}\n",
    );
}

#[test]
fn untyped_type_member_parameters_emit_as_any() {
    assert_emits(
        "interface Callable { (value): void; method(value): void; new (value): object; }",
        "interface Callable {\n    (value: any): void;\n    method(value: any): void;\n    new (value: any): object;\n}\n",
    );
}

#[test]
fn apparent_arrow_and_empty_function_returns_are_reused() {
    assert_emits(
        "export const number = (value: string) => 1;\nexport const nothing = (value?: string) => {};\nexport const nullish = function(value: string) {};",
        "export declare const number: (value: string) => number;\nexport declare const nothing: (value?: string) => void;\nexport declare const nullish: (value: string) => void;",
    );
}

#[test]
fn empty_declaration_and_method_bodies_return_void() {
    assert_emits(
        "export function f() {}\nexport class C { method() {} }",
        "export declare function f(): void;\nexport declare class C {\n    method(): void;\n}\n",
    );
}

#[test]
fn expression_class_base_is_hoisted_to_a_named_declaration() {
    assert_emits(
        "const Derived_base = 1;\nexport declare function factory(): new () => object;\nexport class Derived extends factory() {}",
        "export declare function factory(): new () => object;\ndeclare const Derived_base_1: any;\nexport declare class Derived extends Derived_base_1 {\n}\nexport {};\n",
    );
}

#[test]
fn empty_binding_patterns_emit_no_declaration() {
    assert_emits(
        "var {} = { value: 1 };\nvar [, []] = [1, []];\nvar { value } = { value: 1 };",
        "declare var { value }: {\n    value: number;\n};\n",
    );
}

#[test]
fn binding_patterns_with_defaults_are_flattened_to_names() {
    assert_emits(
        "var [first = 0, nested = [1], { value: renamed = 2 }] = source;",
        "declare var first: any, nested: any, renamed: any;\n",
    );
}

#[test]
fn function_and_method_overload_implementations_are_omitted() {
    assert_emits(
        "export function f(value: string): string;\nexport function f(value: number): number;\nexport function f(value: string | number) { return value; }\nexport class C { method(value: string): string; method(value: number): number; method(value: string | number) { return value; } }",
        "export declare function f(value: string): string;\nexport declare function f(value: number): number;\nexport declare class C {\n    method(value: string): string;\n    method(value: number): number;\n}\n",
    );
}

#[test]
fn function_expando_assignments_become_a_function_namespace_merge() {
    assert_emits(
        "const key = \"X\";\nexport const fn = () => {};\nfn[key] = 0;\nfn.named = (): string => \"\";",
        "export declare function fn(): void;\nexport declare namespace fn {\n    var X: number;\n    var named: () => string;\n}\n",
    );
}

#[test]
fn declared_function_expandos_emit_a_namespace_and_skip_non_identifier_keys() {
    assert_emits(
        "const key = \"named\";\nexport function fn() {}\nfn.direct = 1;\nfn[key] = \"ok\";\nfn[\"not-nameable\"] = true;\nfn[42] = false;",
        "export declare function fn(): void;\nexport declare namespace fn {\n    var direct: number;\n    var named: string;\n}\n",
    );
}

#[test]
fn default_function_expandos_use_a_trailing_default_export() {
    assert_emits(
        "export default function fn(): string { return \"ok\"; }\nfn.value = 1;",
        "declare function fn(): string;\ndeclare namespace fn {\n    var value: number;\n}\nexport default fn;\n",
    );
}

#[test]
fn keyword_and_resolvable_expando_names_take_a_generated_local() {
    // `transformExpandoAssignment` (`transform.go:2782`): a reserved word cannot
    // name a `var`, so it is declared under a temp and re-exported; once a
    // specifier exists, earlier members gain `export` and later ones carry it.
    // The temp counter runs over the whole file and skips `_i`/`_n`
    // (`declarationEmitFunctionKeywordProp`, `nullPropertyName`).
    assert_emits(
        "function foo() {}\nfoo.null = true;\nfunction baz() {}\nbaz.x = 1;\nbaz.class = true;\nbaz.normal = false;\nbaz.undefined = 2;\nbaz.foo = 3;",
        "declare function foo(): void;\ndeclare namespace foo {\n    var _a: boolean;\n    export { _a as null };\n}\ndeclare function baz(): void;\ndeclare namespace baz {\n    export var x: number;\n    var _b: boolean;\n    export { _b as class };\n    export var normal: boolean;\n    export var _c: number;\n    export { _c as undefined };\n    export var _d: number;\n    export { _d as foo };\n}\n",
    );
}

#[test]
fn identifier_valued_expandos_are_export_specifiers() {
    // `transformBinaryExpressionToExportDeclaration` (`transform.go:1307`)
    // (`jsDeclarationsFunctionWithDefaultAssignedMember`).
    assert_emits(
        "function foo() {}\nfoo.foo = foo;\nfoo.default = foo;\nfoo.n = 1;",
        "declare function foo(): void;\ndeclare namespace foo {\n    export { foo };\n    export { foo as default };\n    export var n: number;\n}\n",
    );
}

#[test]
fn private_computed_member_names_keep_their_entity_visible() {
    // `declarationEmitPrivateSymbolCausesVarDeclarationToBeEmitted`: the private
    // member drops its type but restates `[_data]`, so `_data` is emitted.
    let text = emit(
        "const _data: unique symbol = Symbol();\nexport class User {\n    private [_data]: any;\n}",
    );
    assert!(text.starts_with("declare const _data: unique symbol;\n"), "{text}");
    assert!(text.contains("private [_data];"), "{text}");
}

#[test]
fn object_literal_method_signatures_keep_their_references_visible() {
    // `declarationEmitThisPredicatesWithPrivateName02`.
    assert_emits(
        "interface Foo { a: string; }\nexport const obj = {\n    m(): this is Foo { return true; }\n};",
        "interface Foo {\n    a: string;\n}\nexport declare const obj: {\n    m(): this is Foo;\n};\nexport {};\n",
    );
}

#[test]
fn unannotated_binding_pattern_parameters_take_the_implied_type() {
    // `getTypeFromBindingPattern` (`paramterDestrcuturingDeclaration`): object
    // elements with defaults are optional, rests add a string index, array
    // elements after the last required one are optional, and an empty or
    // rest-only array pattern is `Iterable<any>`.
    assert_emits(
        "export interface C {\n    ({ p: name }): any;\n    ({ a: { b }, c = 1, [\"k\"]: k, ...r }, [d, , e = \"s\"], [], [...z]): void;\n}",
        "export interface C {\n    ({ p: name }: {\n        p: any;\n    }): any;\n    ({ a: { b }, c, [\"k\"]: k, ...r }: {\n        a: {\n            b: any;\n        };\n        c?: number;\n        k: any;\n        [x: string]: any;\n    }, [d, , e]: [any, any?, string?], []: Iterable<any>, [...z]: Iterable<any>): void;\n}\n",
    );
}

#[test]
fn unannotated_initialized_parameters_take_the_initializer_type() {
    // `ensureType` reads the parameter's declared type, which for `x = 1` is
    // the widened initializer type; the emitter used to drop the initializer
    // and write `any`.
    assert_emits(
        "export function f(x = 1, y = \"a\"): void {}",
        "export declare function f(x?: number, y?: string): void;\n",
    );
}

#[test]
fn untyped_property_signatures_emit_as_any() {
    assert_emits(
        "declare global { interface Box { value; } }",
        "declare global {\n    interface Box {\n        value: any;\n    }\n}\n",
    );
}

#[test]
fn non_nameable_computed_members_are_omitted() {
    assert_emits(
        "interface I { [\"\" + \"\"](): void; kept(): void; }\nclass C { [\"\" + \"\"]() {} kept() {} }\nvar value: { [\"\" + \"\"](): void; kept: number };",
        "interface I {\n    kept(): void;\n}\ndeclare class C {\n    kept(): void;\n}\ndeclare var value: {\n    kept: number;\n};\n",
    );
}

#[test]
fn a_hash_private_member_becomes_a_single_marker() {
    // `buildClassMembers` (`:1918`): a class with any `#name` carries one
    // `#private` marker, and the members themselves are not named — emitting them
    // would leak a private name.
    let text =
        emit("export class C {\n    #a: number = 1;\n    #b: number = 2;\n    c: number = 3;\n}");
    assert_eq!(text.matches("#private").count(), 1, "expected exactly one marker:\n{text}");
    assert!(!text.contains("#a"), "a private name leaked:\n{text}");
}

#[test]
fn a_parameter_property_becomes_a_property() {
    // `buildClassMembers`'s parameter-property pass: `constructor(public x: T)`
    // declares a property, and the `.d.ts` has to restate it as one.
    assert_emits(
        "export class C {\n    constructor(public a: number, private b: string) {}\n}",
        "export declare class C {\n    a: number;\n    private b;\n    constructor(a: number, b: string);\n}",
    );
}

#[test]
fn strip_internal_removes_annotated_members() {
    let source = "class C {\n  kept(): void {}\n  // @internal\n  removed(): void {}\n}";
    assert_eq!(emit_stripping_internal(source), "declare class C {\n    kept(): void;\n}");
}

#[test]
fn strip_internal_removes_parameter_properties_but_keeps_parameters() {
    let source = "export class C { constructor(\n/** @internal */ public removed: string,\n/** @internal */ // explanation\npublic kept: string\n) {} }";
    assert_eq!(
        emit_stripping_internal(source),
        "export declare class C {\n    kept: string;\n    constructor(removed: string, kept: string);\n}"
    );
}

#[test]
fn declaration_emit_preserves_leading_jsdoc_unless_comments_are_removed() {
    let source = "/** value docs */\nexport const value: number = 1;\nexport class Box {\n    /** member docs */\n    member: string = \"\";\n}";
    let arena = Arena::new();
    let parsed = tsr_parser::parse(&arena, source);
    let mut nodes = parsed.nodes;
    let result = tsr_declarations::emit_with_options(
        &arena,
        &mut nodes,
        parsed.source_file,
        tsr_declarations::DeclarationEmitOptions {
            source_text: Some(source),
            strip_internal: false,
            remove_comments: false,
            strict_null_checks: false,
            force_module: false,
            source_map_url: None,
        },
    );
    assert_eq!(
        result.text,
        "/** value docs */\nexport declare const value: number;\nexport declare class Box {\n    /** member docs */\n    member: string;\n}"
    );

    let arena = Arena::new();
    let parsed = tsr_parser::parse(&arena, source);
    let mut nodes = parsed.nodes;
    let result = tsr_declarations::emit_with_options(
        &arena,
        &mut nodes,
        parsed.source_file,
        tsr_declarations::DeclarationEmitOptions {
            source_text: Some(source),
            strip_internal: false,
            remove_comments: true,
            strict_null_checks: false,
            force_module: false,
            source_map_url: None,
        },
    );
    assert!(!result.text.contains("docs"), "removeComments leaked JSDoc: {}", result.text);
}

#[test]
fn literal_declarations_preserve_context_and_widen_negative_bigints() {
    let source = "export const value = {\n    /** one docs */\n    one: 1,\n    /** string docs */\n    string: 'one',\n    /** template docs */\n    template: `one`,\n    /** method docs */\n    method(): void {}\n} as const;\nexport const topLevelString = 'one';\nexport const topLevelTemplate = `one`;\nexport const mutable = { negativeBigInt: -1n };";
    let arena = Arena::new();
    let parsed = tsr_parser::parse(&arena, source);
    let mut nodes = parsed.nodes;
    let result = tsr_declarations::emit_with_options(
        &arena,
        &mut nodes,
        parsed.source_file,
        tsr_declarations::DeclarationEmitOptions {
            source_text: Some(source),
            strip_internal: false,
            remove_comments: false,
            strict_null_checks: false,
            force_module: false,
            source_map_url: None,
        },
    );
    assert_eq!(
        result.text,
        "export declare const value: {\n    /** one docs */\n    readonly one: 1;\n    /** string docs */\n    readonly string: 'one';\n    /** template docs */\n    readonly template: `one`;\n    /** method docs */\n    readonly method: () => void;\n};\nexport declare const topLevelString = \"one\";\nexport declare const topLevelTemplate = \"one\";\nexport declare const mutable: {\n    negativeBigInt: bigint;\n};"
    );
}

#[test]
fn destructured_parameter_properties_are_flattened_when_types_are_syntactic() {
    assert_emits(
        "export class C { constructor(public [[x], { value: [y] }, [...rest]]: any[]) {} }",
        "export declare class C {\n    x: any;\n    y: any;\n    rest: any;\n    constructor([[x], { value: [y] }, [...rest]]: any[]);\n}\n",
    );
    assert_emits(
        "export class C { constructor(public [x, y]: string[]) {} }",
        "export declare class C {\n    x: string;\n    y: string;\n    constructor([x, y]: string[]);\n}\n",
    );
    assert_emits(
        "type Tuple = [string, number]; type Object = { value: boolean }; export class C { constructor(public [text, count]: Tuple, public { value }: Object) {} }",
        "type Tuple = [string, number];\ntype Object = {\n    value: boolean;\n};\nexport declare class C {\n    text: string;\n    count: number;\n    value: boolean;\n    constructor([text, count]: Tuple, { value }: Object);\n}\nexport {};\n",
    );
}

#[test]
fn an_optional_parameter_property_includes_undefined_in_its_property_type() {
    assert_emits(
        "export class C { constructor(public value?: string, public already?: number | undefined) {} }",
        "export declare class C {\n    value?: string | undefined;\n    already?: number | undefined;\n    constructor(value?: string | undefined, already?: number | undefined);\n}\n",
    );
}

#[test]
fn destructured_parameter_defaults_are_removed_recursively() {
    assert_emits(
        "export function f({ default: renamed = {}, nested: [value = 1] }: Options): void {}",
        "export declare function f({ default: renamed, nested: [value] }: Options): void;\n",
    );
    assert_emits(
        "export const f = ({ default: renamed = {}, nested: [value = 1], }: Options): void => {};",
        "export declare const f: ({ default: renamed, nested: [value], }: Options) => void;\n",
    );
}

#[test]
fn a_definite_assignment_assertion_is_dropped_but_an_optional_marker_is_not() {
    // `transformPropertyDeclaration` (`:979`): `!` is not legal in a `.d.ts`; `?`
    // is, and dropping it would change the type.
    assert_emits(
        "export class C {\n    a!: number;\n    b?: number;\n}",
        "export declare class C {\n    a: number;\n    b?: number;\n}",
    );
}

#[test]
fn a_setter_with_no_parameter_gets_a_synthesized_one() {
    // `updateAccessorParamList` (`:1031`): "emit `value: any` for non-private
    // accessors to match TypeScript's declaration emit behavior".
    let text =
        emit("export class C {\n    get a(): number { return 1; }\n    set a(v: number) {}\n}");
    assert!(text.contains("get a(): number;"), "{text}");
    assert!(text.contains("set a(v: number);"), "{text}");
}

#[test]
fn a_static_block_and_a_stray_semicolon_emit_nothing() {
    // `visitDeclarationSubtree`'s elision arms: a static block is runtime, and a
    // `SemicolonClassElement` is punctuation.
    assert_emits(
        "export class C {\n    static { }\n    ;\n    a: number = 1;\n}",
        "export declare class C {\n    a: number;\n}",
    );
}

// ----- parameters -----------------------------------------------------------

#[test]
fn a_parameter_with_an_initializer_becomes_optional() {
    // `ensureParameter` (`:2395`). The initializer cannot be emitted, so without
    // the `?` the signature would require an argument that used to be optional —
    // the same fact stated two ways.
    assert_emits(
        "export declare function f(a: number, b: number): void;",
        "export declare function f(a: number, b: number): void;",
    );
    assert_emits(
        "export function f(a: number = 1): void {}",
        "export declare function f(a?: number): void;",
    );
}

#[test]
fn a_rest_parameter_keeps_its_dots() {
    assert_emits(
        "export function f(...rest: number[]): void {}",
        "export declare function f(...rest: number[]): void;",
    );
}

// ----- enums ----------------------------------------------------------------

#[test]
fn enum_members_emit_their_folded_values() {
    // `transformEnumDeclaration` (`:2262`) rewrites every member to its constant.
    // `enum E { A }` and `enum E { A = 0 }` are the same type and not the same
    // text, and this gate compares text.
    assert_emits(
        "export enum E {\n    A,\n    B,\n    C = 10,\n    D,\n}",
        "export declare enum E {\n    A = 0,\n    B = 1,\n    C = 10,\n    D = 11\n}",
    );
}

#[test]
fn ambient_enum_members_without_initializers_stay_uninitialized() {
    assert_emits(
        "declare namespace N { enum E { A, B = 3, C } }",
        "declare namespace N {\n    enum E {\n        A,\n        B = 3,\n        C\n    }\n}\n",
    );
}

#[test]
fn an_enum_member_with_no_constant_value_emits_no_value() {
    // Upstream sets `newInitializer = nil` when the evaluator has nothing, which is
    // legal and is what a computed member emits.
    let text = emit("declare function f(): number;\nexport enum E {\n    A = f(),\n}");
    assert!(text.contains("A\n") || text.trim_end().ends_with('A'), "expected a bare `A`:\n{text}");
    assert!(!text.contains("f()"), "a call reached the .d.ts:\n{text}");
}

#[test]
fn enum_members_fold_through_their_siblings() {
    assert_emits(
        "export enum F {\n    A = 1,\n    B = 2,\n    C = A | B,\n}",
        "export declare enum F {\n    A = 1,\n    B = 2,\n    C = 3\n}",
    );
}

// ----- literal consts -------------------------------------------------------

#[test]
fn a_literal_const_emits_its_value_re_spelled() {
    // `CreateLiteralConstValue`. Upstream rebuilds the literal from the checker's
    // *value*, so `0x1` comes back as `1` and a template as a string.
    assert_emits("export const a = 0x1;", "export declare const a = 1;");
    assert_emits("export const b = 0o17;", "export declare const b = 15;");
    assert_emits("export const c = 1_000;", "export declare const c = 1000;");
    assert_emits("export const d = `s`;", "export declare const d = \"s\";");
    assert_emits("export const e = -1;", "export declare const e = -1;");
    assert_emits("export const f = +1;", "export declare const f = 1;");
}

#[test]
fn an_annotated_const_emits_its_annotation_not_its_value() {
    // `isLiteralConstDeclaration` is about a *fresh literal type*. An annotation
    // means the declared type is the annotation, so `= 42` would be a different
    // type — and would parse, which is why this needed a test rather than a gate.
    assert_emits("export const a: number = 42;", "export declare const a: number;");
}

#[test]
fn a_let_widens_and_a_const_object_widens_too() {
    // A `const` *declaration* is not a const *assertion*. Only `as const` produces
    // `readonly` members and literal types — `bd tsr-49v.2.6`.
    assert_emits("export let a = 1;", "export declare let a: number;");
    assert_emits("export const b = { c: 1 };", "export declare const b: {\n    c: number;\n};");
    assert_emits(
        "export const d = { e: 1 } as const;",
        "export declare const d: {\n    readonly e: 1;\n};",
    );
}

// ----- statements that are not declarations ---------------------------------

#[test]
fn runtime_statements_are_elided() {
    // `visit`'s elision list (`transform.go:247`). Nothing with a runtime effect
    // survives, and nothing that survives has a body.
    assert_emits(
        "export const a: number = 1;\nconsole.log(a);\nif (a) { }\nfor (;;) { }\nwhile (a) { }\ntry { } catch { }\n",
        "export declare const a: number;",
    );
}

// ----- object-literal accessors ---------------------------------------------

#[test]
fn annotated_object_accessors_keep_their_shape() {
    // `declarationEmitObjectLiteralAccessors1`: a get/set pair keeps both
    // signatures in source order; a lone getter is a `readonly` property of its
    // return type; a lone setter a mutable property of its parameter type.
    assert_emits(
        "export const a = {\n    get x(): string { return \"\"; },\n    set x(v: number) {},\n};",
        "export declare const a: {\n    get x(): string;\n    set x(v: number);\n};",
    );
    assert_emits(
        "export const b = { get x(): string { return \"\"; } };",
        "export declare const b: {\n    readonly x: string;\n};",
    );
    assert_emits(
        "export const c = { set x(v: number) {} };",
        "export declare const c: {\n    x: number;\n};",
    );
}

// ----- arrow returns that name a declaration --------------------------------

#[test]
fn an_arrow_returning_an_annotated_name_copies_the_annotation() {
    // `typeReferenceDirectives4`: upstream resolves the reference and prints its
    // declared type; the syntactic dual copies the written annotation when the
    // file scope is provably the only scope in reach.
    assert_emits(
        "export interface $ { x: number }\nexport declare let x: $;\nexport let y = () => x;",
        "export interface $ {\n    x: number;\n}\nexport declare let x: $;\nexport declare let y: () => $;",
    );
    // A parameter shadows the file scope and its annotation wins; the
    // unreferenced outer `x` is pruned by reachability as usual.
    assert_emits(
        "declare let x: string;\nexport let y = (x: number) => x;",
        "export declare let y: (x: number) => number;",
    );
}

#[test]
fn a_typeof_annotation_is_not_copied_into_a_return() {
    // `typeReferenceDirectives7`: `typeof $` resolves through to the value's
    // type in upstream's print, so copying it would restate something upstream
    // does not write. Declining leaves the honest `any`.
    assert_emits(
        "export let $ = 1;\nexport let x: typeof $;\nexport let y = () => x;",
        "export declare let $: number;\nexport declare let x: typeof $;\nexport declare let y: any;",
    );
}

#[test]
fn jsdoc_types_javascript_accessors() {
    // `declarationEmitClassAccessorsJs1`: `@returns {T}` is a getter's written
    // return type and `@param {T} name` the setter's value parameter type.
    let text = emit_javascript(
        "export class V {\n    /** @returns {string} */\n    get path() { return ''; }\n    /** @param {URL | string} path */\n    set path(path) { }\n}",
    );
    assert!(text.contains("get path(): string;"), "{text}");
    assert!(text.contains("set path(path: URL | string);"), "{text}");
}

#[test]
fn file_level_values_restate_as_typeof_and_new_as_the_class() {
    // `declarationEmitLocalClassHasRequiredDeclare`, `declarationEmitDefaultExport7`,
    // `declarationEmitMappedTypeDistributivityPreservesConstraints`: a reference
    // that certainly resolves to a file-level class, function or enum is that
    // symbol's own type, and `new C()` of a non-generic class is `C`. The
    // referenced declarations become visible.
    assert_emits(
        "class X {}\nfunction fn() {}\nclass A {}\nexport class B { static X = X; }\nexport const o = { fn };\nexport default new A();",
        "declare class X {\n}\ndeclare function fn(): void;\ndeclare class A {\n}\nexport declare class B {\n    static X: typeof X;\n}\nexport declare const o: {\n    fn: typeof fn;\n};\ndeclare const _default: A;\nexport default _default;\n",
    );
    // A variable of the same name, a generic class, and a reference inside a
    // namespace body all decline.
    let text = emit(
        "class G<T> {}\nexport const g = new G();\nexport namespace N { class X {} export const x = X; }",
    );
    assert!(text.contains("export declare const g: any;"), "{text}");
    assert!(text.contains("const x: any;"), "{text}");
}

#[test]
fn expando_namespaces_attach_to_the_first_overload_and_need_a_named_member() {
    // `declarationEmitFunctionDuplicateNamespace`,
    // `declarationEmitLateBoundAssignments2`.
    assert_emits(
        "export function f(a: 0): 0;\nexport function f(a: 1): 1;\nexport function f(a: 0 | 1) { return a; }\nf.x = 2;\nexport function g() {}\ng[77] = 0;",
        "export declare function f(a: 0): 0;\nexport declare namespace f {\n    var x: number;\n}\nexport declare function f(a: 1): 1;\nexport declare function g(): void;\n",
    );
}

#[test]
fn exported_function_expressions_become_function_declarations() {
    // `transformExportAssignment` (`transform.go:1235`): `modulePreserve4`,
    // `declarationEmitExportAliasVisibiilityMarking`. The export comes first and
    // the promoted declaration's parameter types keep their imports visible.
    assert_emits(
        "export = function() {};",
        "export = _default;\ndeclare function _default(): void;\n",
    );
    assert_emits(
        "import { Suit } from './Types';\nexport default (suit: Suit): string => '';",
        "import { Suit } from './Types';\nexport default _default;\ndeclare function _default(suit: Suit): string;\n",
    );
}

#[test]
fn type_references_reach_their_symbol_not_every_same_named_declaration() {
    // A declaration's own name and a property signature's name declare rather
    // than reference (`neverReturningFunctions1`, `nonPrimitiveAndEmptyObject`).
    assert_emits(
        "export interface Component { fooProps?: string }\nexport type D = Partial<Component>;\nconst Component = f();\nconst fooProps = g();",
        "export interface Component {\n    fooProps?: string;\n}\nexport type D = Partial<Component>;\n",
    );
    // A type reference makes the whole merged local symbol visible
    // (`declarationEmitNamespaceMergedWithInterfaceNestedFunction`).
    assert_emits(
        "export interface Foo { item: Bar }\ninterface Bar { baz(): void }\nnamespace Bar { export function biz(): number { return 0; } }",
        "export interface Foo {\n    item: Bar;\n}\ninterface Bar {\n    baz(): void;\n}\ndeclare namespace Bar {\n    function biz(): number;\n}\nexport {};\n",
    );
}

#[test]
fn commonjs_files_are_modules_whose_exports_are_their_assignments() {
    // `module.exports = foo` leads the file as `export = foo`
    // (`jsDeclarationsTypeReassignmentFromDeclaration`); a `require` call alone
    // makes the file a module, so unexported declarations drop.
    assert_eq!(
        emit_javascript_trimmed("function foo() {}\nfunction unused() {}\nmodule.exports = foo;"),
        "export = foo;\ndeclare function foo(): void;"
    );
    assert_eq!(emit_javascript_trimmed("const a = require(\"x\");\nvar b = 1;"), "export {};");
    // `exports.x = …` members (`transformCommonJSExportWorker`): literal types
    // survive, `default` goes through `_default`, a top-level single alias is
    // a specifier, and a name declared elsewhere takes an `_exported` binding.
    assert_eq!(
        emit_javascript_trimmed(
            "function f() {}\nconst g = 1;\nexports.y = 2;\nexports.default = { x: \"x\" };\nexports.h = f;\nexports.g = \"s\";"
        ),
        "export declare var y: 2;\ndeclare const _default: {\n    x: string;\n};\nexport default _default;\nexport { f as h };\ndeclare const _exported: \"s\";\nexport { _exported as g };\ndeclare function f(): void;"
    );
    // A synthesized `export =` name wraps the members in its namespace
    // (`wrapInCJSExportNamespace`).
    assert_eq!(
        emit_javascript_trimmed("module.exports = function () {};\nmodule.exports.x = 1;"),
        "export = _exports;\ndeclare function _exports(): void;\ndeclare namespace _exports {\n    export var x: 1;\n}"
    );
}

fn emit_javascript_trimmed(source: &str) -> String {
    emit_javascript(source).trim_end().to_string()
}
