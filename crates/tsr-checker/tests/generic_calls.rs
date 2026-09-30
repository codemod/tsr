//! That a generic call is **reached**, which the unit tests cannot prove.
//!
//! `crate::inference`'s own tests call `check_generic_call` directly, so they
//! measure inference and say nothing about whether anything routes to it.
//! `Checker::check_call_expression` (`calls.rs`) returned `errorType` for every
//! generic signature until the guard there was pointed at the module, and an
//! arm that is never entered has tests that pass without ever running it. This
//! file goes through `check_expression`, which is the only path a `.types`
//! baseline ever takes.
//!
//! The oracle is
//! `vendor/typescript-go/testdata/baselines/reference/submodule/conformance/callGenericFunctionWithZeroTypeArguments.types:10`:
//!
//! ```text
//! var r = f(1);
//! >r : number
//! >f(1) : 1
//! >f : <T>(x: T) => T
//! ```
//!
//! Note which line is asserted. `>r : number` is the *variable*, widened at the
//! declaration by machinery that already existed; `>f(1) : 1` is the **call**,
//! and it is unwidened. A port that widens the inference candidate gets the
//! variable right and the call wrong, so asserting on the variable would hide
//! the error this file exists to catch.

use tsr_ast::Statement;
use tsr_checker::Checker;
use tsr_core::Arena;

#[test]
fn an_annotation_can_infer_a_type_parameter_without_argument_candidates() {
    assert_eq!(
        type_of_last("declare function infer<T>(): T; const observed: number = infer();"),
        "number"
    );
}

#[test]
fn argument_inferences_replace_weaker_contextual_return_candidates() {
    assert_eq!(
        type_of_last(
            "declare function infer<T>(input: T): T; const observed: string = infer(123);"
        ),
        "123"
    );
}

#[test]
fn pure_contextual_return_inferences_combine_different_property_types() {
    let source = "declare function infer<T>(): { first: T; second: T };
        const observed: { first: number; second: string } = infer();";
    assert_eq!(type_of_last(source), "{ first: string | number; second: string | number; }");
}

#[test]
fn pure_return_inference_filters_union_members_outside_the_constraint() {
    let source = "declare function infer<T extends 'a' | 'b'>(): T;
        const observed: 'a' | 'c' = infer();";
    assert_eq!(type_of_last(source), "\"a\"");
}

#[test]
fn an_incompatible_argument_inference_falls_back_to_a_closed_constraint() {
    let source = "declare function infer<T extends string>(value: T): T;
        const observed = infer(123);";
    assert_eq!(type_of_last(source), "string");
}

#[test]
fn a_constraint_can_select_the_other_candidate_direction() {
    let source =
        "declare function infer<T extends string>(value: T, consume: (value: T) => void): T;
        declare const consume: (value: number) => void;
        const observed = infer('abc', consume);";
    assert_eq!(type_of_last(source), "\"abc\"");
}

#[test]
fn written_and_computed_homomorphic_maps_preserve_primitives() {
    let mapping = "type Immutable<T> = { readonly [K in keyof T]: T[K] };";
    assert_eq!(
        type_of_last(&format!("{mapping} const observed = null as Immutable<string>;")),
        "string"
    );
    assert_eq!(
        type_of_last(&format!(
            "{mapping} declare function infer<T>(value: T): Immutable<T>;
            const observed = infer('abc');"
        )),
        "string"
    );
}

#[test]
fn a_computed_homomorphic_map_rebuilds_array_elements() {
    let source = "interface Array<T> { length: number; [index: number]: T }
        interface ReadonlyArray<T> { readonly length: number; readonly [index: number]: T }
        type Immutable<T> = { readonly [K in keyof T]: T[K] };
        declare function infer<T>(value: T): Immutable<T>;
        const observed = infer([1, 2]);";
    assert_eq!(type_of_last(source), "readonly number[]");
}

#[test]
fn a_computed_homomorphic_map_preserves_its_object_members() {
    let source = "type Immutable<T> = { readonly [K in keyof T]: T[K] };
        declare function infer<T>(value: T): Immutable<T>;
        const observed = infer({ value: 123 }).value;";
    assert_eq!(type_of_last(source), "number");
}

#[test]
fn homomorphic_maps_preserve_tuple_labels_and_change_modifiers() {
    let mapping = "type Immutable<T> = { readonly [K in keyof T]: T[K] };";
    assert_eq!(
        type_of_last(&format!(
            "{mapping} const observed = null as Immutable<[first: number, second?: string]>;"
        )),
        "readonly [first: number, second?: string | undefined]"
    );
    assert_eq!(
        type_of_last(
            "type Mutable<T> = { -readonly [K in keyof T]: T[K] };
            declare function infer<T>(value: T): Mutable<T>;
            const observed = infer(null as readonly [number, string]);"
        ),
        "[number, string]"
    );
}

#[test]
fn removing_tuple_optionality_keeps_null_and_required_undefined() {
    let source = "type RequiredTuple<T> = { [K in keyof T]-?: T[K] };
        const observed = null as RequiredTuple<[first: number | undefined, second?: string | null]>;";
    assert_eq!(type_of_last(source), "[first: number | undefined, second: string | null]");
}

#[test]
fn removing_array_optionality_keeps_null() {
    let source = "interface Array<T> { length: number; [index: number]: T }
        type RequiredElements<T> = { [K in keyof T]-?: T[K] };
        const observed = null as RequiredElements<(number | null | undefined)[]>;";
    assert_eq!(type_of_last(source), "(number | null)[]");
}

#[test]
fn homomorphic_tuple_maps_rebuild_rest_arrays() {
    let source = "interface Array<T> { length: number; [index: number]: T }
        interface ReadonlyArray<T> { readonly length: number; readonly [index: number]: T }
        type Immutable<T> = { readonly [K in keyof T]: T[K] };
        const observed = null as Immutable<[first: number, ...tail: string[]]>;";
    assert_eq!(type_of_last(source), "readonly [first: number, ...tail: string[]]");
}

#[test]
fn adding_tuple_optionality_maps_fixed_and_rest_elements() {
    let source = "interface Array<T> { length: number; [index: number]: T }
        type Optional<T> = { [K in keyof T]?: T[K] };
        const observed = null as Optional<[number, string?, ...boolean[]]>;";
    assert_eq!(
        type_of_last(source),
        "[(number | undefined)?, (string | undefined)?, ...(boolean | undefined)[]]"
    );
}

#[test]
fn a_written_mapped_tuple_constraint_keeps_its_reference_spelling() {
    let source = "interface Array<T> { length: number; [index: number]: T }
        interface ReadonlyArray<T> { readonly length: number; readonly [index: number]: T }
        type Immutable<T> = { readonly [K in keyof T]: T[K] };
        declare function infer<U, T extends Immutable<[U, ...U[]]>>(value: T): T;
        const observed = infer;";
    assert_eq!(type_of_last(source), "<U, T extends Immutable<[U, ...U[]]>>(value: T) => T");
}

#[test]
fn exact_optional_mapped_tuples_omit_missing_but_rest_arrays_include_undefined() {
    let source = "interface Array<T> { length: number; [index: number]: T }
        type Optional<T> = { [K in keyof T]?: T[K] };
        const observed = null as Optional<[number, string?, ...boolean[]]>;";
    let options = tsr_core::CompilerOptions {
        strict: tsr_core::Tristate::True,
        exact_optional_property_types: tsr_core::Tristate::True,
        ..Default::default()
    };
    assert_eq!(
        type_of_last_with_options(source, Some(&options)),
        "[number?, string?, ...(boolean | undefined)[]]"
    );
}

#[test]
fn nonstrict_mapped_tuple_optionality_does_not_add_undefined() {
    let source = "type Optional<T> = { [K in keyof T]?: T[K] };
        const observed = null as Optional<[number, string]>;";
    let options = tsr_core::CompilerOptions {
        strict_null_checks: tsr_core::Tristate::False,
        ..Default::default()
    };
    assert_eq!(type_of_last_with_options(source, Some(&options)), "[number?, string?]");
}

#[test]
fn deferred_identity_maps_expand_after_a_variadic_operand_is_substituted() {
    let source = "interface Array<T> { length: number; [index: number]: T }
        interface ReadonlyArray<T> { readonly length: number; readonly [index: number]: T }
        type Immutable<T> = { readonly [K in keyof T]: T[K] };
        declare function infer<T extends unknown[]>(value: T): Immutable<[number, ...T]>;
        const observed = infer(null as [string, boolean]);";
    assert_eq!(type_of_last(source), "readonly [number, string, boolean]");
}

#[test]
fn deferred_optional_maps_keep_the_variadic_element_modifiers() {
    let source = "interface Array<T> { length: number; [index: number]: T }
        type Optional<T> = { [K in keyof T]?: T[K] };
        declare function infer<T extends unknown[]>(value: T): Optional<[number, ...T]>;
        const observed = infer(null as [string, boolean]);";
    assert_eq!(
        type_of_last(source),
        "[(number | undefined)?, (string | undefined)?, (boolean | undefined)?]"
    );
}

#[test]
fn mapped_tuple_aliases_normalize_at_written_and_inferred_instantiations() {
    let declarations = "interface Array<T> { length: number; [index: number]: T }
        interface ReadonlyArray<T> { readonly length: number; readonly [index: number]: T }
        type Immutable<T> = { readonly [K in keyof T]: T[K] };
        type Prefix<T extends unknown[]> = Immutable<[number, ...T]>;";
    assert_eq!(
        type_of_last(&format!(
            "{declarations}
            declare function infer<T extends unknown[]>(value: T): Prefix<T>;
            const observed = infer(null as [string, boolean]);"
        )),
        "readonly [number, string, boolean]"
    );
    assert_eq!(
        type_of_last(&format!(
            "{declarations} const observed = null as Prefix<[string, boolean]>;"
        )),
        "readonly [number, string, boolean]"
    );
}

#[test]
fn an_uninferred_outer_parameter_does_not_replace_an_inner_default() {
    let source = "declare function wrap<T>(input: T): [T];
        declare function inner<T = string>(): T;
        const observed = wrap(inner());";
    assert_eq!(type_of_last(source), "[string]");
}

#[test]
fn return_context_and_callback_inputs_use_independent_outer_mappers() {
    let source = "type Op<I, O> = (input: I) => O;
        type Box<V> = { data: V };
        declare function consume<A>(first: Op<number, A>): A;
        declare function map<T, R>(project: (value: T) => R): Op<T, R>;
        declare function box<V>(data: V): Box<V>;
        const observed = consume(map(data => box(data)));";
    assert_eq!(type_of_last(source), "Box<number>");
}

#[test]
fn input_only_inference_chooses_the_common_subtype() {
    let source = "class Base { value: string } class Derived extends Base { extra: number }
        declare function infer<T>(first: (input: T) => void, second: (input: T) => void): T;
        declare const broad: (input: Base) => void;
        declare const narrow: (input: Derived) => void;
        const observed = infer(broad, narrow);";
    assert_eq!(type_of_last(source), "Derived");
}

#[test]
fn compatible_output_inference_preserves_a_literal_against_a_broad_input() {
    let source = "declare function infer<T>(value: T, consume: (input: T) => void): T;
        declare const consume: (input: string) => void;
        const observed = infer('abc', consume);";
    assert_eq!(type_of_last(source), "\"abc\"");
}

#[test]
fn never_output_inference_yields_to_an_input_candidate() {
    let source = "declare function infer<T>(value: T, consume: (input: T) => void): T;
        declare const empty: never;
        declare const consume: (input: string) => void;
        const observed = infer(empty, consume);";
    assert_eq!(type_of_last(source), "string");
}

#[test]
fn different_generic_reference_targets_infer_through_shared_members() {
    let source = "interface Source<V> { value: V }
        interface Target<T> { value: T }
        declare const input: Source<number>;
        declare function infer<T>(input: Target<T>): T;
        const observed = infer(input);";
    assert_eq!(type_of_last(source), "number");
}

#[test]
fn structural_never_inference_yields_to_a_contravariant_candidate() {
    let source = "interface Source<V> { value: V }
        interface Target<T> { value: T }
        declare const input: Source<never>;
        declare const consume: (value: number) => void;
        declare function infer<T>(input: Target<T>, consume: (value: T) => void): T;
        const observed = infer(input, consume);";
    assert_eq!(type_of_last(source), "number");
}

#[test]
fn overload_inference_matches_signatures_from_the_bottom_up() {
    let source = "declare const overloaded: {
            (value: string): string;
            (value: number): number;
        };
        declare function infer<T>(callback: (value: number) => T): T;
        const observed = infer(overloaded);";
    assert_eq!(type_of_last(source), "number");
}

#[test]
fn construct_signatures_infer_return_types_separately_from_calls() {
    let source = "declare const overloaded: {
            new (value: string): string;
            new (value: number): number;
        };
        declare function infer<T>(constructor: new (value: number) => T): T;
        const observed = infer(overloaded);";
    assert_eq!(type_of_last(source), "number");
}

#[test]
fn nested_input_positions_restore_covariant_literal_inference() {
    let source = "declare function infer<T>(first: (consume: (input: T) => void) => void,
            second: (consume: (input: T) => void) => void): T;
        declare const first: (consume: (input: 'abc') => void) => void;
        declare const second: (consume: (input: 'def') => void) => void;
        const observed = infer(first, second);";
    assert_eq!(type_of_last(source), "\"abc\" | \"def\"");
}

#[test]
fn method_input_inference_remains_bivariant() {
    let source = "class Base { value: string } class Derived extends Base { extra: number }
        declare function infer<T>(first: { handle(input: T): void }, second: { handle(input: T): void }): T;
        const observed = infer({ handle(input: Base) {} }, { handle(input: Derived) {} });";
    assert_eq!(type_of_last(source), "Base");
}

#[test]
fn an_inapplicable_callback_does_not_fix_the_error_candidates_inferences() {
    assert_eq!(
        type_of_last(
            "declare function f<T, U>(value: T, cb: (input: T) => U, result: U): U;
            const result = f(1, input => '', 1);"
        ),
        "1"
    );
    assert_eq!(
        type_of_last(
            "declare function f<T>(first: T, second: T, cb: (input: T) => T): T;
            const result = f('', 3, input => input);"
        ),
        "\"\""
    );
}

#[test]
fn a_callback_only_fixes_the_parameters_its_context_reads() {
    let source = "declare function f<T, U>(value: T, result: U, callback: (input: T) => T): U;
        const result = f(1, 'keep', input => input);";
    assert_eq!(type_of_last(source), "\"keep\"");
}

#[test]
fn an_error_candidates_object_keeps_its_noncontextual_producer() {
    let source = "enum First { X } enum Second { X }
        declare function run<T, U>(options: { write: (input: T) => U; read: () => T }, other: T): U;
        const result = run({ write: observed => observed, read: () => First.X }, Second.X);";
    assert_eq!(type_of_last(source), "unknown");
}

#[test]
fn an_invalid_context_sensitive_callback_uses_the_error_candidates_constraint() {
    let source = "class Base { value: string }\n\
                  class Derived extends Base { extra: string }\n\
                  declare function apply<T extends Base>(cb: (a: T, b: T) => void): T;\n\
                  const result = apply((a: Derived, b, c) => {});";
    assert_eq!(type_of_last(source), "Base");
}

#[test]
fn identical_intersection_constituents_are_removed_before_inference() {
    assert_eq!(
        type_of_last(
            "declare function f<T>(value: string & T): T; declare const s: string; const x = f(s);"
        ),
        "unknown"
    );
    assert_eq!(
        type_of_last("declare function f<T>(value: string & T): T; const x = f(\"x\");"),
        "\"x\""
    );
    assert_eq!(
        type_of_last(
            "declare function f<T>(value: string & T): T; declare const s: string & { a: number }; const x = f(s);"
        ),
        "{ a: number; }"
    );
}

/// Type the initialiser of the last statement — the **call expression**, not
/// the variable it initialises.
fn type_of_last(source: &str) -> String {
    type_of_last_with_options(source, None)
}

fn type_of_last_with_options(source: &str, options: Option<&tsr_core::CompilerOptions>) -> String {
    let arena = Arena::new();
    let parsed = tsr_parser::parse(&arena, source);
    assert!(
        parsed.diagnostics.is_empty(),
        "fixture must parse: {:?}",
        parsed.diagnostics.iter().map(tsr_diagnostics::Diagnostic::text).collect::<Vec<_>>()
    );
    let bound = tsr_binder::bind(
        &arena,
        parsed.source_file,
        &parsed.nodes,
        tsr_binder::FileInfo { name: "test.ts", text: source },
    );
    let mut checker = Checker::new(&bound, &parsed.nodes, &parsed.node_map);
    if let Some(options) = options {
        checker.apply_compiler_options(options);
    }

    let index = parsed.source_file.statements.len() - 1;
    let Statement::VariableStatement(statement) = parsed.source_file.statements[index] else {
        panic!("the last statement must be a variable statement");
    };
    let initialiser = statement
        .declaration_list
        .and_then(|list| list.declarations.first().copied())
        .and_then(|declaration| declaration.initializer)
        .expect("an initialiser");
    let id = checker.check_expression(initialiser);
    checker.type_to_string(id)
}

#[test]
fn a_generic_call_reaches_inference_through_check_expression() {
    // The baseline line, verbatim: `>f(1) : 1`. Before the guard in
    // `check_call_expression` was pointed at `crate::inference` this printed
    // `error`, which is what makes this an end-to-end assertion rather than a
    // restatement of the unit tests.
    assert_eq!(type_of_last("function f<T>(x: T): T { return x; }\nconst a = f(1);"), "1");
    // Widening would print `number` here and would still be right about `a`.
    assert_ne!(type_of_last("function f<T>(x: T): T { return x; }\nconst a = f(1);"), "number");
}

#[test]
fn a_generic_call_this_slice_cannot_answer_is_still_a_gap_at_the_call_site() {
    // Digging `T` out of `T[]` is `inferTypes` (`inference.go:53`) and is not
    // ported, so the call cannot answer `number`. The guard has to keep gapping
    // through the *same* path it now answers through, which is the half of the
    // wiring the positive test above cannot check.
    //
    // **`unknown` since §929, and that is this harness rather than the
    // checker.** This file binds one LIB-FREE source, so `T[]` cannot resolve
    // `Array` and is itself a gap; §929's road therefore fires on the parameter,
    // keeping the written `T[]` with an `any` type, and the call lands on
    // `unknown` where it used to land on `error`. With a lib mounted — which is
    // what the corpus scores through — `T[]` resolves and §929 never fires here;
    // the corpus measured **zero `RIGHT->WRONG`** across the whole change.
    //
    // Either way the assertion holds what the test is for: **the call does not
    // answer**, and it does not answer the argument type either.
    assert_eq!(
        type_of_last("function f<T>(x: T[]): T { return x[0]; }\nconst a = f([1]);"),
        "unknown"
    );
    assert_ne!(
        type_of_last("function f<T>(x: T[]): T { return x[0]; }\nconst a = f([1]);"),
        "number"
    );
}

#[test]
fn a_written_type_argument_is_used_instead_of_inference() {
    // `f<string>("s")` is `string`, NOT `"s"`. This is the fixture that
    // separates the two paths: inference from the argument would answer the
    // literal `"s"`, which is what the same call without type arguments gives
    // one line down. An implementation that ignores the written arguments and
    // infers anyway passes every other test in this file.
    assert_eq!(
        type_of_last("function f<T>(x: T): T { return x; }\nconst a = f<string>(\"s\");"),
        "string"
    );
    assert_eq!(type_of_last("function f<T>(x: T): T { return x; }\nconst a = f(\"s\");"), "\"s\"");
}

#[test]
fn type_arguments_that_do_not_check_are_a_gap() {
    // SURPLUS arity FLIPPED at §690, on the same derivation §375 corrected for
    // the assertion below — and for the same reason. The old line asserted
    // `error` because *"`checkTypeArguments` fails the call, so answering the
    // one written argument would be a wrong answer"*. Upstream reports the
    // arity error and **still instantiates**: `getSignatureInstantiation`
    // proceeds past the diagnostic, dropping the surplus. The corpus witness is
    // `callGenericFunctionWithIncorrectNumberOfTypeArguments`, whose
    // `f<number, string, number>(1, '')` on `f<T, U>` records `number` — three
    // arguments, two parameters, resolved. Measured at 9 WRONG→RIGHT, zero
    // adverse.
    //
    // §690 deliberately covers the SURPLUS half only. The MISSING half is left
    // to ordinary inference, which already answers it — `f<number>(1, 2)` on
    // `f<T, U>` infers `T = number` from the argument and records `number`,
    // upstream's answer too. Routing it through the written-argument path
    // instead would need `fillMissingTypeArguments`' defaults, which this
    // port's `TypeParameter` does not carry; filling with `any` measured **62
    // RIGHT→WRONG in `genericDefaults`**. Pinned so that the next attempt sees
    // the missing half is not broken and needs no arm.
    assert_eq!(
        type_of_last("function f<T>(x: T): T { return x; }\nconst a = f<string, number>(\"s\");"),
        "string"
    );
    assert_eq!(
        type_of_last("function f<T, U>(x: T, y: U): T { return x; }\nconst a = f<number>(1, 2);"),
        "number"
    );
    // Type arguments on a signature that takes none — `checkNoTypeArguments`.
    // FLIPPED at §375: the old line asserted `error` on the derivation that
    // answering would "quietly drop what the caller wrote" — but upstream
    // REPORTS TS2558 and still resolves the error call through the
    // candidate; the type is the return. The corpus witness is
    // `typeAssertions`' `fn2<string>(4) : void`. What the old pin got
    // right: the construct IS an error — that half lives in the
    // diagnostics lane, not in this type.
    assert_eq!(
        type_of_last("function g(x: number): number { return x; }\nconst a = g<string>(1);"),
        "number"
    );
    // The stand-in came due (the twenty-seventh): §31 made a truly
    // unresolved NAME answer upstream's TS2304 `errorType`, printed `any` —
    // so `typeof missing` is `any` and the instantiated call answers it,
    // exactly as upstream's baseline would. An unresolved TYPE reference
    // (`f<Unported>()`) still gaps; only value-name resolution changed.
    assert_eq!(
        type_of_last(
            "function f<T>(x: T): T { return x; }\nconst a = f<typeof missing>(1 as any);"
        ),
        "any"
    );
}

/// §951: the NON-ARRAY rest parameter — `getNonArrayRestType`
/// (`relater.go:1858`) names the shape this port used to decline whole, and
/// `inferTypeArguments` (`checker.go:9489`) infers a TUPLE of the arguments at
/// the rest position against the rest parameter's own type.
#[test]
fn a_rest_parameter_typed_by_a_type_parameter_infers_a_tuple() {
    // The primary leg. Widened, not literal: `getSpreadArgumentType` applies
    // `getWidenedLiteralType` per element (`checker.go:29500`), so `["a", 1]`
    // would be a wrong answer that prints plausibly.
    assert_eq!(
        type_of_last(
            "declare function f<T extends unknown[]>(...args: T): T;\nconst a = f(\"a\", 1);"
        ),
        "[string, number]"
    );
    // The falsifier leg: no arguments is the EMPTY tuple, which is a distinct
    // print from `never[]` and from a gap. This is also the corpus's
    // `[]`-versus-`never[]` row.
    assert_eq!(
        type_of_last("declare function h<T extends unknown[]>(...args: T): T;\nconst a = h();"),
        "[]"
    );
    // The regression leg — §939's ARRAY-shaped rest keeping its own road — is
    // **not expressible in this harness and is asserted by the corpus instead**
    // (`scorepair` reports zero `RIGHT->WRONG` for §951). This fixture loads no
    // libs, so `T[]` is not a resolvable `Array` reference, `rest_element`
    // reports non-array, and the arm above fires for a shape that takes the
    // other road under the corpus. Use `examples/probefile` for that leg.
    //
    // Recorded because the first draft of this test asserted `number` here and
    // that was **my expectation, not upstream's or this port's**: under the
    // corpus the line reads `r(1, 2) : 1 | 2`, and upstream's own baseline for
    // the same shape is `makeArrayG(1, "") : number[]` (`genericRestArgs`) —
    // inference candidates are WIDENED (`getWidenedLiteralType` in
    // `getInferredType`) and this port does not widen them. That is a real
    // pre-existing defect, outside §951's road, and it now has a witness.
    // A SPREAD argument at the rest position still declines. Upstream gives the
    // element `ElementFlagsVariadic`/`ElementFlagsRest` and this port's tuple
    // side table carries no per-element flags at all (§950), so a tuple built
    // here would claim a required element where upstream records a variadic
    // one. Pinned so the decline is a recorded choice rather than an omission
    // the next reader silently lifts.
    assert_eq!(
        type_of_last(
            "declare function f<T extends unknown[]>(...args: T): T;\nconst xs: string[] = [];\nconst a = f(...xs);"
        ),
        "error"
    );
}
