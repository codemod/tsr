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
            file_name: "a&b.ts".into(),
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

    let export_equals = emit("export = { answer: 42 };");
    assert!(export_equals.contains("declare const _default"), "{export_equals}");
    assert!(export_equals.contains("export = _default;"), "{export_equals}");
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
