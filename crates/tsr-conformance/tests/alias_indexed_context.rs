//! Alias union/intersection bodies must participate in native inference dispatch
//! and non-fixing contextual instantiation without eagerly mapping object bodies.
use tsr_conformance::{TestCase, types_baseline::FileTypes, types_producer};

const SOURCE: &str = r#"// @strict: true
// @target: es2020
// @jsx: preserve
declare namespace JSX {
    interface Element { readonly brand: "element" }
    interface ElementChildrenAttribute { children: {} }
}
interface Choices {
    east: { consume?: (value: number) => void };
    west: { consume?: (value: string) => void };
    polar: { consume?: (value: boolean) => void };
}
type ChoiceProps<C extends keyof Choices> = { selection?: C } & Choices[C];
declare function Choice<C extends keyof Choices>(props: ChoiceProps<C>): JSX.Element;
const west = <Choice selection="west" consume={word => word.toUpperCase()} />;
const east = <Choice selection="east" consume={count => count.toFixed()} />;
const polar = <Choice selection="polar" consume={flag => !flag} />;
const reversed = <Choice consume={firstWord => firstWord.length} selection="west" />;
const explicit = <Choice<"east"> selection="east" consume={writtenCount => writtenCount.toFixed()} />;
declare function choose<Q extends keyof Choices>(props: ChoiceProps<Q>): Q;
const ordinary = choose({selection: "west", consume: ordinaryWord => ordinaryWord.length});
interface NumberChoices { 3: { consume: (value: Date) => void }; 8: { consume: (value: bigint) => void } }
type NumberProps<N extends keyof NumberChoices> = { selection: N } & NumberChoices[N];
declare function NumberChoice<N extends keyof NumberChoices>(props: NumberProps<N>): JSX.Element;
const numeric = <NumberChoice selection={8} consume={large => large.toString()} />;
type Data<T> = { value: T; consume: (value: T) => void };
declare function Identity<T>(props: Data<T>): JSX.Element;
const objectTemplate = <Identity value={17} consume={objectCount => objectCount.toFixed()} />;
type Either<T> = { value: T } | { alternate: T };
declare function either<T>(props: Either<T>): T;
const aliasUnion = either({alternate: { label: "held", rank: 9 }});
interface Base { root: string }
interface Derived extends Base { leaf: number }
type Handlers<T> = { accept: (value: T) => void } & { confirm: (value: T) => void };
declare function consume<T>(handlers: Handlers<T>): T;
const contravariant = consume({accept: (base: Base) => {}, confirm: (derived: Derived) => {}});
type BaseProps = { locale: string };
type ChildProps<T extends BaseProps> = { children: (props: T) => string } & T;
declare function Child<T extends BaseProps>(props: ChildProps<T>): JSX.Element;
declare const bp: BaseProps;
const child = <Child {...bp}>{childProps => childProps.locale}</Child>;
const attributeChild = <Child {...bp} children={attributeProps => attributeProps.locale} />;
type PairProps<T extends BaseProps> = { children: [(props: T) => string, (props: T) => number] } & T;
declare function Pair<T extends BaseProps>(props: PairProps<T>): JSX.Element;
const pairChildren = <Pair {...bp}>{pairWord => pairWord.locale}{pairLength => pairLength.locale.length}</Pair>;
type ArrayProps<T extends BaseProps> = { children: ((props: T) => string)[] } & T;
declare function Many<T extends BaseProps>(props: ArrayProps<T>): JSX.Element;
const arrayChildren = <Many {...bp}>{arrayWord => arrayWord.locale}{arrayUpper => arrayUpper.locale.toUpperCase()}</Many>;
type Bare<T> = T | { wrap: T };
declare function hold<T>(value: Bare<T>): [T];
const held = hold("asymmetric");
type NonNull<T> = T & {};
declare function holdIntersection<T>(value: NonNull<T>): [T];
const heldIntersection = holdIntersection("intersection");
declare const unsettled: "east" | "west";
const ambiguous = <Choice selection={unsettled} consume={unknownValue => {}} />;
"#;

#[test]
fn scalar_candidates_complete_indexed_callbacks_before_certification() {
    assert_types(&[
        "selection : \"west\"",
        "word => word.toUpperCase() : (word: string) => string",
        "count => count.toFixed() : (count: number) => string",
        "flag => !flag : (flag: boolean) => boolean",
        "firstWord => firstWord.length : (firstWord: string) => number",
        "writtenCount => writtenCount.toFixed() : (writtenCount: number) => string",
        "large => large.toString() : (large: bigint) => string",
    ]);
}

#[test]
fn structured_alias_dispatch_is_shared_with_calls_and_preserves_object_templates() {
    assert_types(&[
        "ordinary : \"west\"",
        "ordinaryWord => ordinaryWord.length : (ordinaryWord: string) => number",
        "objectCount => objectCount.toFixed() : (objectCount: number) => string",
        "aliasUnion : { label: string; rank: number; }",
        "contravariant : Derived",
        "held : [string]",
        "heldIntersection : [\"intersection\"]",
    ]);
}

#[test]
fn deferred_attribute_and_children_images_cannot_infer_a_whole_naked_variable() {
    assert_types(&[
        "childProps => childProps.locale : (childProps: BaseProps) => string",
        "attributeProps => attributeProps.locale : (attributeProps: BaseProps) => string",
        "pairWord => pairWord.locale : (pairWord: BaseProps) => string",
        "pairLength => pairLength.locale.length : (pairLength: BaseProps) => number",
        "arrayWord => arrayWord.locale : (arrayWord: BaseProps) => string",
        "arrayUpper => arrayUpper.locale.toUpperCase() : (arrayUpper: BaseProps) => string",
    ]);
}

#[test]
fn differing_union_inputs_still_decline_without_a_discriminated_context() {
    // Native produces any plus TS7006 for this ambiguous callback. The port's
    // explicit unresolved certification must remain a decline, not publish a
    // guessed callable input or complete the opening signature from it.
    assert_types(&["unknownValue => {} : error"]);
}

fn assert_types(wanted: &[&str]) {
    let case = TestCase::parse("probe/alias-indexed-context", "alias-indexed-context.tsx", SOURCE);
    let expected: Vec<_> = case
        .files
        .iter()
        .map(|unit| FileTypes { file: unit.name.clone(), assertions: Vec::new() })
        .collect();
    let lines: Vec<_> = types_producer::assertions_for_case(&case, &expected, false)
        .iter()
        .flatten()
        .map(types_producer::Assertion::line)
        .collect();
    for wanted in wanted {
        assert!(lines.iter().any(|line| line == wanted), "missing {wanted}: {lines:?}");
    }
}
