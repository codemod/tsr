//! JSX contexts and fixing order compared with pinned tsgo 5b1047d probes.
use tsr_conformance::{TestCase, types_baseline::FileTypes, types_producer};

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
fn jsx_declines_uncertified_union_callbacks_but_keeps_common_inputs() {
    // Decline pins, NOT native parity: jsx.go's attribute discriminator selects
    // string/number using bare, explicit, and absent optional discriminants.
    // Until that operation is ported, these callbacks must not publish an any
    // signature. The identical-input union is independently native-supported.
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
            "word => word.length : error",
            "bodyWord => bodyWord.length : error",
            "count => count.toFixed() : error",
            "bodyCount => bodyCount.toFixed() : error",
            "missing => missing.toFixed() : error",
            "bodyMissing => bodyMissing.toFixed() : error",
            "undefinedMode => undefinedMode.toFixed() : error",
            "same => same.length : (same: string) => number",
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
