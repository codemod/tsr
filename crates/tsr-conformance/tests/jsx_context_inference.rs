//! JSX contexts and fixing order compared with pinned tsgo 5b1047d probes.
use tsr_conformance::{TestCase, types_baseline::FileTypes, types_producer};

// Native TYPE accessibility controls on pinned 5b1047d. Canonical references
// below deliberately bypass written placeholder spelling, not type production.
const TYPE_NAMING_SOURCE: &str = r#"// @strict: true
// @target: es2015
// @module: commonjs
// @esModuleInterop: true
// @filename: naming.d.ts
declare module "same" {
    namespace React { interface Pair<A, B> { left: A; right: B } }
    export = React;
}
declare module "real" {
    namespace Actual { interface Pair<A, B> { left: B; right: A } }
    export default Actual;
}
declare module "outer" {
    import Nested from "real";
    export { Nested };
}
declare module "specifier" {
    namespace Actual { interface Pair<A, B> { left: B; right: A } }
    export { Actual as default };
    export interface Pair<A, B> { left: A; right: B }
}
declare module "marked" {
    namespace Marked { const __esModule: true; interface Pair<A, B> { left: A; right: B } }
    export = Marked;
}
declare module "callable" {
    function Callable(value: number): number;
    namespace Callable { interface Pair<A, B> { left: A; right: B } const token: 31; }
    export = Callable;
}
// @filename: naming.ts
/// <reference path="naming.d.ts" />
import React from "same";
import * as Safe from "same";
import Outer from "outer";
import Real from "real";
import Specifier from "specifier";
import Blocked from "marked";
import * as Clone from "callable";
declare const rootSlot: React.Pair<string, number>;
declare const realSlot: Real.Pair<boolean, 19>;
declare const shortestSlot: Real.Pair<31, true>;
declare const specifierSlot: Specifier.Pair<23, string>;
declare const markedSlot: import("marked").Pair<string, number>;
declare const cloneTypeSlot: Clone.Pair<29, string>;
const cloneValueSlot = Clone;
const sourceValueSlot = Clone;
rootSlot; realSlot; shortestSlot; specifierSlot; markedSlot; cloneTypeSlot; cloneValueSlot; sourceValueSlot;
function valueShadow() {
    const React = 47;
    let valueShadowSlot!: import("same").Pair<string, number>;
    valueShadowSlot;
}
namespace Other { export interface Pair<A, B> { left: B; right: A } }
namespace Inner {
    import React = Other;
    let namespaceShadowSlot!: import("same").Pair<string, number>;
    namespaceShadowSlot;
}
namespace Own {
    export interface Pair<A, B> { left: A; right: B }
    let directSlot!: Pair<string, number>;
    directSlot;
}
// @filename: default-shadows.ts
/// <reference path="naming.d.ts" />
/// <reference path="naming.ts" />
import DefaultOnly from "same";
import * as Safe from "same";
function defaultValueShadow() {
    const DefaultOnly = 53;
    let defaultValueShadowSlot!: import("same").Pair<string, number>;
    defaultValueShadowSlot;
}
namespace Other { export interface Pair<A, B> { left: B; right: A } }
namespace DefaultInner {
    import DefaultOnly = Other;
    let defaultNamespaceShadowSlot!: import("same").Pair<string, number>;
    defaultNamespaceShadowSlot;
}
"#;

#[test]
fn canonical_type_names_preserve_default_scope_shadows_and_clone_identity() {
    use tsr_ast::{BindingName, Expression, Node, TypeNode};
    let case = TestCase::parse("probe/type-naming", "type-naming.ts", TYPE_NAMING_SOURCE);
    let arena = tsr_core::Arena::new();
    let program = types_producer::program_for_case(&arena, &case);
    let mut queries = Vec::new();
    let mut annotations = std::collections::HashMap::new();
    let mut stack: Vec<_> =
        program.source_files().iter().map(|file| Node::SourceFile(file.source_file())).collect();
    while let Some(node) = stack.pop() {
        if let Node::VariableDeclaration(variable) = node
            && let Some(BindingName::Identifier(name)) = variable.name
            && let Some(ty) = variable.r#type
        {
            annotations.insert(name.text, ty);
        }
        if let Node::Identifier(name) = node
            && name.text.ends_with("Slot")
            && program.nodes().parent(name.node_id.unwrap()).is_some_and(|parent| {
                program.nodes().kind(parent) == tsr_ast::SyntaxKind::ExpressionStatement
            })
        {
            queries.push(name);
        }
        tsr_ast::push_children(node, &mut stack);
    }
    queries.sort_by_key(|name| program.nodes().span(name.node_id.unwrap()).start);
    assert_eq!(queries.len(), 13);
    for reverse in [false, true] {
        let mut checker = types_producer::configured_checker(&program);
        let mut order = queries.clone();
        if reverse {
            order.reverse();
        }
        for _ in 0..2 {
            for name in &order {
                let module_name = match name.text {
                    "realSlot" | "shortestSlot" => "real",
                    "specifierSlot" => "specifier",
                    "markedSlot" => "marked",
                    "cloneTypeSlot" | "cloneValueSlot" | "sourceValueSlot" => "callable",
                    _ => "same",
                };
                let module = program.binder().ambient_module(module_name).unwrap();
                let exports = &program.binder().symbols().get(module).exports;
                let export =
                    exports.get("export=").or_else(|| exports.get("default")).copied().unwrap();
                let namespace = checker.resolve_alias(export).unwrap();
                let mut target = program.binder().symbols().get(namespace).exports["Pair"];
                if name.text == "directSlot" {
                    target = program
                        .binder()
                        .resolve_name(
                            program.nodes(),
                            program.node_map(),
                            name.node_id.unwrap(),
                            "Pair",
                            tsr_binder::SymbolFlags::TYPE,
                        )
                        .unwrap();
                }
                let written_arguments = match annotations.get(name.text) {
                    Some(TypeNode::TypeReferenceNode(node)) => node.type_arguments,
                    Some(TypeNode::ImportTypeNode(node)) => node.type_arguments,
                    _ => &[],
                };
                let arguments = written_arguments
                    .iter()
                    .map(|argument| checker.get_type_from_type_node(*argument))
                    .collect();
                let ordinary = checker.check_expression(Expression::Identifier(name));
                let ty = match name.text {
                    "cloneValueSlot" => ordinary,
                    "sourceValueSlot" => checker.get_type_of_symbol(namespace),
                    _ => checker.create_type_reference_public(target, arguments),
                };
                let actual = checker.type_to_string_at(ty, name.node_id.unwrap());
                let expected = match name.text {
                    "rootSlot" | "valueShadowSlot" => Some("React.Pair<string, number>"),
                    "defaultValueShadowSlot" => Some("DefaultOnly.Pair<string, number>"),
                    "namespaceShadowSlot" | "defaultNamespaceShadowSlot" => {
                        Some("Safe.Pair<string, number>")
                    }
                    "realSlot" => Some("Real.Pair<boolean, 19>"),
                    "shortestSlot" => Some("Real.Pair<31, true>"),
                    "specifierSlot" => Some("Specifier.Pair<23, string>"),
                    "cloneTypeSlot" => Some("Clone.Pair<29, string>"),
                    "cloneValueSlot" => Some("typeof Clone"),
                    // The TYPE reader must not open VALUE or unsupported default naming.
                    "sourceValueSlot" => None,
                    "markedSlot" => Some("import(\"marked\").Marked.Pair<string, number>"),
                    "directSlot" => Some("Pair<string, number>"),
                    _ => unreachable!(),
                };
                assert_eq!(actual.as_deref(), expected, "{} reverse={reverse}", name.text);
            }
        }
    }
}

fn assert_types(source: &str, wanted: &[&str]) {
    let case = TestCase::parse("probe/jsx-context", "jsx-context.tsx", source);
    let expected: Vec<_> = case
        .files
        .iter()
        .map(|unit| FileTypes { file: unit.name.clone(), assertions: Vec::new() })
        .collect();
    let assertions = types_producer::assertions_for_case(&case, &expected, false);
    let lines: Vec<_> = assertions.iter().flatten().map(types_producer::Assertion::line).collect();
    for wanted in wanted {
        assert!(lines.iter().any(|line| line == wanted), "missing {wanted}: {lines:?}");
    }
}

const PRELUDE: &str = r#"// @strict: true
// @target: es2015
// @jsx: preserve
declare namespace JSX {
    interface Element { readonly brand: "element" }
    interface ElementChildrenAttribute { content: {} }
    interface IntrinsicElements { input: { onValue: (value: string) => void } }
}
"#;

// Native jsxMixedSignatureSupplierProbe on pinned 5b1047d. In particular,
// DifferentClass and DifferentMixed deliberately have different instance props
// and constructor parameters: the overall tag's reference kind owns selection.
const MIXED_SIGNATURE_SOURCE: &str = r#"// @strict: true
// @target: es2015
// @jsx: preserve
declare namespace JSX {
    interface Element { readonly brand: "element" }
    interface ElementAttributesProperty { props: {} }
    interface ElementChildrenAttribute { content: {} }
}
interface TextProps { mode: "west"; onValue: (value: string) => number; content?: (value: string) => number }
interface NumberProps { mode: "east"; onValue: (value: number) => string; content?: (value: number) => string }
interface TextInstance { props: TextProps }
interface NumberInstance { props: NumberProps }
type TextFunction = (props: TextProps) => JSX.Element;
type TextClass = new (props: TextProps) => TextInstance;
type NumberFunction = (props: NumberProps) => JSX.Element;
type NumberClass = new (props: NumberProps) => NumberInstance;
declare const Mixed: TextClass | TextFunction;
declare const Reverse: TextFunction | TextClass;
declare const Numeric: NumberClass | NumberFunction;
const attributes = <Mixed mode="west" onValue={word => word.length} content={attributeChild => attributeChild.length} />;
const body = <Mixed onValue={bodyWord => bodyWord.length} mode="west">{bodyChild => bodyChild.length}</Mixed>;
const reverse = <Reverse mode="west" onValue={reverseWord => reverseWord.length}>{reverseChild => reverseChild.length}</Reverse>;
const numeric = <Numeric onValue={count => count.toFixed()} mode="east">{countChild => countChild.toFixed()}</Numeric>;
interface BothKinds { (props: TextProps): JSX.Element; new (props: NumberProps): NumberInstance }
declare const Preferred: BothKinds;
const preferred = <Preferred mode="east" onValue={preferredCount => preferredCount.toFixed()} />;
interface DifferentInstance { props: NumberProps }
declare const DifferentClass: new (props: TextProps) => DifferentInstance;
declare const DifferentMixed: TextFunction | (new (props: TextProps) => DifferentInstance);
const differentClass = <DifferentClass mode="east" onValue={differentCount => differentCount.toFixed()} />;
const differentMixed = <DifferentMixed mode="west" onValue={differentWord => differentWord.length}>{differentChild => differentChild.length}</DifferentMixed>;
declare const UniformCallBeforeFallback: BothKinds | TextFunction;
const uniform = <UniformCallBeforeFallback mode="west" onValue={uniformWord => uniformWord.length}>{uniformChild => uniformChild.length}</UniformCallBeforeFallback>;
"#;

#[test]
fn jsx_mixed_signature_context_preserves_reference_kind_and_precedence() {
    assert_types(
        MIXED_SIGNATURE_SOURCE,
        &[
            "word => word.length : (word: string) => number",
            "attributeChild => attributeChild.length : (attributeChild: string) => number",
            "bodyWord => bodyWord.length : (bodyWord: string) => number",
            "bodyChild => bodyChild.length : (bodyChild: string) => number",
            "reverseWord => reverseWord.length : (reverseWord: string) => number",
            "reverseChild => reverseChild.length : (reverseChild: string) => number",
            "count => count.toFixed() : (count: number) => string",
            "countChild => countChild.toFixed() : (countChild: number) => string",
            "preferredCount => preferredCount.toFixed() : (preferredCount: number) => string",
            "differentCount => differentCount.toFixed() : (differentCount: number) => string",
            "differentWord => differentWord.length : (differentWord: string) => number",
            "differentChild => differentChild.length : (differentChild: string) => number",
            "uniformWord => uniformWord.length : (uniformWord: string) => number",
            "uniformChild => uniformChild.length : (uniformChild: string) => number",
        ],
    );
}

#[test]
fn jsx_mixed_signature_context_matches_cold_reverse_and_warm_program_queries() {
    use tsr_ast::{BindingName, Expression, Node};
    let case = TestCase::parse("probe/jsx-mixed", "jsx-mixed.tsx", MIXED_SIGNATURE_SOURCE);
    let arena = tsr_core::Arena::new();
    let program = types_producer::program_for_case(&arena, &case);
    let file = program
        .source_files()
        .iter()
        .find(|file| file.file_name().ends_with("jsx-mixed.tsx"))
        .expect("configured source");
    let mut callbacks = Vec::new();
    let mut stack = vec![Node::SourceFile(file.source_file())];
    while let Some(node) = stack.pop() {
        if let Node::ArrowFunction(arrow) = node {
            let Some(BindingName::Identifier(name)) = arrow.parameters[0].name else {
                unreachable!()
            };
            let parameter = match name.text {
                "count" | "countChild" | "preferredCount" | "differentCount" => "number",
                _ => "string",
            };
            let result = if parameter == "number" { "string" } else { "number" };
            callbacks.push((arrow, format!("({}: {parameter}) => {result}", name.text)));
        }
        tsr_ast::push_children(node, &mut stack);
    }
    assert_eq!(callbacks.len(), 14);
    callbacks.sort_by_key(|(arrow, _)| program.nodes().span(arrow.node_id.unwrap()).start);
    for reversed in [false, true] {
        let mut checker = types_producer::configured_checker(&program);
        let mut order: Vec<_> = callbacks.iter().collect();
        if reversed {
            order.reverse();
        }
        for _ in 0..2 {
            for (arrow, expected) in &order {
                let ty = checker.check_expression(Expression::ArrowFunction(arrow));
                assert_eq!(checker.type_to_string(ty), *expected, "reverse={reversed}");
            }
        }
    }
}

#[test]
fn jsx_composite_own_members_keep_optional_discriminant_metadata() {
    let source = format!(
        r#"{PRELUDE}
declare namespace JSX {{ interface IntrinsicAttributes {{ key?: string | number; readonly sharedTag?: "same" }} }}
interface TextProps {{ mode?: false; onValue: (value: string) => number; content?: (value: string) => number }}
interface NumberProps {{ mode: true; onValue: (value: number) => string; content?: (value: number) => string }}
type ChoiceProps = TextProps | NumberProps;
declare function Choice(props: ChoiceProps): JSX.Element;
const omitted = <Choice onValue={{absentWord => absentWord.length}} />;
const explicit = <Choice mode onValue={{explicitNumber => explicitNumber.toFixed()}} />;
const body = <Choice onValue={{parentWord => parentWord.length}}>{{childWord => childWord.length}}</Choice>;
const reversed = <Choice onValue={{laterNumber => laterNumber.toFixed()}} mode />;
interface RequiredTag {{ readonly sharedTag: "same"; onValue: (value: string) => void }}
interface OptionalTag {{ readonly sharedTag?: "same"; onValue: (value: number) => void }}
declare function BySharedTag(props: RequiredTag | OptionalTag): JSX.Element;
const absentTag = <BySharedTag onValue={{absentTagNumber => absentTagNumber.toFixed()}} />;
const undefinedTag = <BySharedTag sharedTag={{undefined}} onValue={{undefinedTagNumber => undefinedTagNumber.toFixed()}} />;
"#
    );
    assert_types(
        &source,
        &[
            "absentWord => absentWord.length : (absentWord: string) => number",
            "explicitNumber => explicitNumber.toFixed() : (explicitNumber: number) => string",
            "parentWord => parentWord.length : (parentWord: string) => number",
            "childWord => childWord.length : (childWord: string) => number",
            "laterNumber => laterNumber.toFixed() : (laterNumber: number) => string",
            "absentTagNumber => absentTagNumber.toFixed() : (absentTagNumber: number) => string",
            "undefinedTagNumber => undefinedTagNumber.toFixed() : (undefinedTagNumber: number) => string",
        ],
    );
}

#[test]
fn jsx_scalar_generic_context_is_not_a_callback_certification_query() {
    let source = format!(
        r#"{PRELUDE}
declare function Literal<T extends "north" | "south">(props: {{selection?: T; content?: T; consume?: (value: T) => void}}): JSX.Element;
const literal = <Literal selection="north" consume={{value => {{}}}}>{{"south"}}</Literal>;
const explicit = <Literal<"south"> selection={{"south"}} />;
"#
    );
    assert_types(
        &source,
        &[
            "selection : \"north\"",
            "selection : \"south\"",
            "value => {} : (value: \"north\" | \"south\") => void",
        ],
    );
}

#[test]
fn jsx_completed_producers_fix_later_consumers_per_opening_node() {
    let source = format!(
        r#"{PRELUDE}
declare function Pair<A, B>(props: {{ value: A; produce: (value: A) => B; consume: (value: B) => void }}): JSX.Element;
const first = <Pair value={{17}} produce={{source => source.toFixed()}} consume={{result => result.toUpperCase()}} />;
const second = <Pair value="abc" produce={{source => source.length}} consume={{result => result.toFixed()}} />;
const spread = <Pair {{...{{value: true}}}} produce={{source => source ? 1 : 0}} consume={{result => result.toFixed()}} />;
const reversed = <Pair value={{17}} consume={{early => {{}}}} produce={{source => source.toFixed()}} />;
"#
    );
    assert_types(
        &source,
        &[
            "source => source.toFixed() : (source: number) => string",
            "result => result.toUpperCase() : (result: string) => string",
            "source => source.length : (source: string) => number",
            "result => result.toFixed() : (result: number) => string",
            "source => source ? 1 : 0 : (source: boolean) => 0 | 1",
            "result => result.toFixed() : (result: 0 | 1) => string",
            "early => {} : (early: unknown) => void",
        ],
    );
}

#[test]
fn jsx_failed_inference_defaults_follow_the_use_sites_script_kind() {
    let source = r#"// @strict: true
// @target: es2015
// @jsx: preserve
// @allowJs: true
// @checkJs: true
// @filename: jsxContextDefinitions.ts
declare namespace JSX {
    interface Element { readonly brand: "element" }
    interface ElementChildrenAttribute { children: {} }
}
declare function GenericView<T>(props: {consume?: (value: T) => void; children?: (value: T) => void}): JSX.Element;
// @filename: jsxGenericTypescript.tsx
const tsDefaults = <GenericView consume={missingTs => {}} />;
const tsChildren = <GenericView>{missingTsChild => {}}</GenericView>;
// @filename: jsxGenericJavascript.jsx
const jsDefaults = <GenericView consume={missingJs => {}} />;
const jsChildren = <GenericView>{missingJsChild => {}}</GenericView>;
"#;
    assert_types(
        source,
        &[
            "missingTs => {} : (missingTs: unknown) => void",
            "missingTsChild => {} : (missingTsChild: unknown) => void",
            "missingJs => {} : (missingJs: any) => void",
            "missingJsChild => {} : (missingJsChild: any) => void",
        ],
    );
}

#[test]
fn jsx_discriminates_attribute_and_body_callback_contexts() {
    // Pinned jsx.go:267: attributes discriminate the complete props union,
    // including absent optional fields but excluding semantic body children.
    let source = format!(
        r#"{PRELUDE}
declare function Mixed(props:
    | {{mode: true; onValue: (value: string) => void; content?: (value: string) => void}}
    | {{mode?: false; onValue: (value: number) => void; content?: (value: number) => void}}
): JSX.Element;
const one = <Mixed mode onValue={{word => word.length}}>{{bodyWord => bodyWord.length}}</Mixed>;
const two = <Mixed mode={{false}} onValue={{count => count.toFixed()}}>{{bodyCount => bodyCount.toFixed()}}</Mixed>;
const absent = <Mixed onValue={{missing => missing.toFixed()}}>{{bodyMissing => bodyMissing.toFixed()}}</Mixed>;
const undef = <Mixed mode={{undefined}} onValue={{undefinedMode => undefinedMode.toFixed()}} />;
declare function Shared(props:
    | {{kind: "left"; onValue: (value: string) => number}}
    | {{kind: "right"; onValue: (value: string) => string}}
): JSX.Element;
const common = <Shared kind="left" onValue={{same => same.length}} />;
"#
    );
    assert_types(
        &source,
        &[
            "word => word.length : (word: string) => number",
            "bodyWord => bodyWord.length : (bodyWord: string) => number",
            "count => count.toFixed() : (count: number) => string",
            "bodyCount => bodyCount.toFixed() : (bodyCount: number) => string",
            "missing => missing.toFixed() : (missing: number) => string",
            "bodyMissing => bodyMissing.toFixed() : (bodyMissing: number) => string",
            "undefinedMode => undefinedMode.toFixed() : (undefinedMode: number) => string",
            "same => same.length : (same: string) => number",
        ],
    );
}

#[test]
fn jsx_discriminants_resolve_symbols_and_preserve_unmatched_unions() {
    let source = format!(
        r#"{PRELUDE}
declare function Mixed(props:
    | {{mode: true; onValue: (value: string) => void}}
    | {{mode?: false; onValue: (value: number) => void}}
): JSX.Element;
const beforeTag = <Mixed onValue={{laterTag => laterTag.length}} mode />;
const shadowed = true;
{{
    const shadowed = false;
    const inside = <Mixed mode={{shadowed}} onValue={{lexical => lexical.toFixed()}} />;
}}
const outside = <Mixed mode={{shadowed}} onValue={{lexical => lexical.length}} />;
declare const dynamic: boolean;
const wide = <Mixed mode={{dynamic}} onValue={{wide => {{}}}} />;
const invalid = <Mixed mode={{"invalid"}} onValue={{unmatched => {{}}}} />;
enum Variant {{ Words = 7, Counts = 13 }}
declare function EnumView(props:
    | {{mode: Variant.Words; onValue: (value: string) => void}}
    | {{mode: Variant.Counts; onValue: (value: number) => void}}
): JSX.Element;
const words = <EnumView mode={{Variant.Words}} onValue={{word => word.length}} />;
const counts = <EnumView mode={{Variant.Counts}} onValue={{count => count.toFixed()}} />;
declare function ByChildren(props:
    | {{content: "required"; onValue: (value: string) => void}}
    | {{content?: "optional"; onValue: (value: number) => void}}
): JSX.Element;
const empty = <ByChildren onValue={{emptyBody => emptyBody.toFixed()}}>
    {{/* no semantic child */}}
</ByChildren>;
const body = <ByChildren onValue={{presentBody => {{}}}}>{{"required"}}</ByChildren>;
"#
    );
    assert_types(
        &source,
        &[
            "laterTag => laterTag.length : (laterTag: string) => number",
            "lexical => lexical.toFixed() : (lexical: number) => string",
            "lexical => lexical.length : (lexical: string) => number",
            "wide => {} : (wide: any) => void",
            "unmatched => {} : (unmatched: any) => void",
            "word => word.length : (word: string) => number",
            "count => count.toFixed() : (count: number) => string",
            "emptyBody => emptyBody.toFixed() : (emptyBody: number) => string",
            "presentBody => {} : (presentBody: any) => void",
        ],
    );
}

#[test]
fn jsx_unmatched_discriminants_preserve_prior_selection_in_source_order() {
    // Native ignores a value matching no remaining constituent. Swapping the
    // two inconsistent attributes therefore changes the selected context.
    let source = format!(
        r#"{PRELUDE}
declare function Multi(props:
    | {{mode: "north"; phase: 7; onValue: (value: string) => void}}
    | {{mode: "north"; phase: 13; onValue: (value: number) => void}}
    | {{mode: "south"; phase: 13; onValue: (value: boolean) => void}}
): JSX.Element;
const matching = <Multi mode="north" phase={{13}} onValue={{matched => matched.toFixed()}} />;
const afterNarrow = <Multi mode="south" phase={{7}} onValue={{after => !after}} />;
const beforeNarrow = <Multi phase={{7}} mode="south" onValue={{before => before.length}} />;
const noMatch = <Multi mode="north" phase={{99}} onValue={{uncertain => {{}}}} />;
declare function NullView(props:
    | {{tag: null; onValue: (value: string) => void}}
    | {{tag?: false; onValue: (value: number) => void}}
): JSX.Element;
const nullValue = <NullView tag={{null}} onValue={{nullTag => nullTag.length}} />;
const absentNullValue = <NullView onValue={{absentTag => absentTag.toFixed()}} />;
declare function Tuples(props:
    | {{mode: true; content: [(value: string) => void, (value: boolean) => void]}}
    | {{mode: false; content: [(value: number) => void, (value: number) => void]}}
): JSX.Element;
const wordAndFlag = <Tuples mode>{{left => left.length}}{{/* trivia */}}{{right => !right}}</Tuples>;
const counts = <Tuples mode={{false}}>{{left => left.toFixed()}}{{right => right.toFixed()}}</Tuples>;
"#
    );
    assert_types(
        &source,
        &[
            "matched => matched.toFixed() : (matched: number) => string",
            "after => !after : (after: boolean) => boolean",
            "before => before.length : (before: string) => number",
            "uncertain => {} : (uncertain: any) => void",
            "nullTag => nullTag.length : (nullTag: string) => number",
            "absentTag => absentTag.toFixed() : (absentTag: number) => string",
            "left => left.length : (left: string) => number",
            "right => !right : (right: boolean) => boolean",
            "left => left.toFixed() : (left: number) => string",
            "right => right.toFixed() : (right: number) => string",
        ],
    );
}

#[test]
fn jsx_body_children_share_the_attribute_mapper_and_ignore_trivia() {
    let source = format!(
        r"{PRELUDE}
declare function Child<A, B>(props: {{ value: A; select: (value: A) => B; content: (value: B) => void }}): JSX.Element;
const body = <Child value={{{{code: 17}}}} select={{source => source.code}}>
    {{/* ignored */}}
    {{selected => selected.toFixed()}}
</Child>;
declare function Multiple(props: {{ content: [(left: string) => void, (right: number) => void] }}): JSX.Element;
const multiple = <Multiple>
    {{left => left.toUpperCase()}}
    {{/* ignored */}}
    {{right => right.toFixed()}}
</Multiple>;
"
    );
    assert_types(
        &source,
        &[
            "source => source.code : (source: { code: number; }) => number",
            "selected => selected.toFixed() : (selected: number) => string",
            "left => left.toUpperCase() : (left: string) => string",
            "right => right.toFixed() : (right: number) => string",
        ],
    );
}

#[test]
fn jsx_written_arguments_intrinsics_and_return_only_skip_sources() {
    let source = format!(
        r#"{PRELUDE}
declare function Explicit<T>(props: {{value: T; consume: (value: T) => void}}): JSX.Element;
const written = <Explicit<boolean> value={{true}} consume={{flag => !flag}} />;
const intrinsic = <input onValue={{text => text.toUpperCase()}} />;
declare function Producer<T>(props: {{ produce: () => T; consume: (value: T) => void }}): JSX.Element;
const producerOnly = <Producer produce={{() => "s"}} consume={{word => word.toUpperCase()}} />;
const annotated = <Producer produce={{() => 17}} consume={{(value: number) => value.toFixed()}} />;
"#
    );
    assert_types(
        &source,
        &[
            "flag => !flag : (flag: boolean) => boolean",
            "text => text.toUpperCase() : (text: string) => string",
            "word => word.toUpperCase() : (word: string) => string",
            "(value: number) => value.toFixed() : (value: number) => string",
        ],
    );
}

#[test]
fn jsx_factory_namespace_owns_element_props_and_class_instance_context() {
    let source = r#"// @strict: true
// @target: es2015
// @jsx: react
// @jsxFactory: Local.h
declare namespace JSX { interface Element { globalBrand: "global" } }
namespace Local {
    export function h(tag: unknown, props: unknown, ...children: unknown[]): JSX.Element { return null as any; }
    export namespace JSX {
        export interface Element { localBrand: "local" }
        export interface ElementAttributesProperty { props: {} }
        export interface ElementChildrenAttribute { child: {} }
        export interface IntrinsicAttributes { key?: string }
        export interface IntrinsicClassAttributes<T> { ref?: (instance: T) => void }
        export interface IntrinsicElements { input: { onValue: (value: boolean) => void } }
        export type LibraryManagedAttributes<C, P> = P;
    }
}
declare function View(props: { onValue: (value: string) => void; child?: (value: number) => void }): Local.JSX.Element;
const local = <View onValue={word => word.toUpperCase()}>{count => count.toFixed()}</View>;
const localIntrinsic = <input onValue={enabled => !enabled} />;
declare class Component<T> { props: {value: T; onValue: (value: T) => void}; }
const genericClass = <Component value={29} onValue={count => count.toFixed()} ref={instance => instance.props.value.toFixed()} />;
declare function Tuple<T>(props: {child: [(value: T) => T, T]}): Local.JSX.Element;
const tuple = <Tuple>{value => value + "!"}{"word"}</Tuple>;
"#;
    assert_types(
        source,
        &[
            "local : Local.JSX.Element",
            "word => word.toUpperCase() : (word: string) => string",
            "count => count.toFixed() : (count: number) => string",
            "enabled => !enabled : (enabled: boolean) => boolean",
            "instance => instance.props.value.toFixed() : (instance: Component<number>) => string",
            "value => value + \"!\" : (value: string) => string",
        ],
    );
}

#[test]
fn jsx_intrinsic_context_uses_literal_tag_index_keys() {
    // Native jsx.go:1190 uses a tag's literal key. Broad string fallback is
    // lower precedence; overlapping patterns intersect; named members win.
    let source = r#"// @strict: true
// @target: es2015
// @jsx: preserve
declare namespace JSX {
    interface Element { readonly brand: "element" }
    interface IntrinsicElements {
        [name: string]: { consume?: (value: any) => void };
        [name: `widget-${string}`]: { consume?: (value: number) => void; selection?: string };
        [name: `widget-special-${string}`]: { consume?: (value: number) => void; selection?: "west"; only?: (value: string) => void };
        explicit: { consume?: (value: string) => void; selection?: "east" };
        fallback: { consume?: (value: boolean) => void };
    }
}
const broad = <fallback consume={flag => !flag} />;
const ordinary = <widget-normal consume={count => count.toFixed()} selection="free" />;
const overlapping = <widget-special-normal consume={overlapCount => overlapCount.toFixed()} only={overlapWord => overlapWord.length} selection="west" />;
const wrongLiteral = <widget-special-invalid selection="east" />;
const named = <explicit consume={namedWord => namedWord.length} />;
const nearMiss = <widget consume={flag => !flag} />;
"#;
    assert_types(
        source,
        &[
            "flag => !flag : (flag: boolean) => boolean",
            "count => count.toFixed() : (count: number) => string",
            "overlapCount => overlapCount.toFixed() : (overlapCount: number) => string",
            "overlapWord => overlapWord.length : (overlapWord: string) => number",
            "namedWord => namedWord.length : (namedWord: string) => number",
            "selection : \"west\"",
            "selection : \"east\"",
            "selection : string",
            "flag => !flag : (flag: any) => boolean",
        ],
    );
}

#[test]
fn jsx_multi_child_context_indexes_only_native_array_like_types() {
    let source = r#"// @strict: true
// @target: es2015
// @jsx: preserve
declare namespace JSX {
    interface Element { readonly brand: "element" }
    interface ElementChildrenAttribute { content: {} }
}
interface Words extends ReadonlyArray<(value: string) => void> {}
interface Counts extends ReadonlyArray<(value: any) => void> {
    readonly 0: (value: 7) => void;
    readonly 1: (value: 13) => void;
}
declare function WordList(props: {content: Words}): JSX.Element;
const words = <WordList>{leftWord => leftWord.length}{/* semantic index ignores comments */}{rightWord => rightWord.toUpperCase()}</WordList>;
declare function CountList(props: {content: Counts}): JSX.Element;
const counts = <CountList>{leftCount => { leftCount.toFixed(); }}{rightCount => { rightCount.toFixed(); }}</CountList>;
declare function Compound(props: {content: ReadonlyArray<(value: boolean) => void> & {readonly marker?: "compound"}}): JSX.Element;
const compound = <Compound>{leftFlag => !leftFlag}{rightFlag => !rightFlag}</Compound>;
declare function Structural(props: {content: {readonly length: number; readonly [index: number]: (value: boolean) => void}}): JSX.Element;
const merelyIndexed = <Structural>{notAnArray => {}}{stillNotAnArray => {}}</Structural>;
declare function SetList(props: {content: ReadonlySet<(value: string) => void>}): JSX.Element;
const merelyIterable = <SetList>{iterableOnly => {}}{alsoIterableOnly => {}}</SetList>;
const singleArrayChild = <WordList>{oneChild => {}}</WordList>;
"#;
    assert_types(
        source,
        &[
            "leftWord => leftWord.length : (leftWord: string) => number",
            "rightWord => rightWord.toUpperCase() : (rightWord: string) => string",
            "leftCount => { leftCount.toFixed(); } : (leftCount: 7) => void",
            "rightCount => { rightCount.toFixed(); } : (rightCount: 13) => void",
            "leftFlag => !leftFlag : (leftFlag: boolean) => boolean",
            "rightFlag => !rightFlag : (rightFlag: boolean) => boolean",
            "notAnArray => {} : (notAnArray: any) => void",
            "stillNotAnArray => {} : (stillNotAnArray: any) => void",
            "iterableOnly => {} : (iterableOnly: any) => void",
            "alsoIterableOnly => {} : (alsoIterableOnly: any) => void",
            "oneChild => {} : (oneChild: any) => void",
        ],
    );
}
