//! What an unannotated parameter's type comes from when its function is a call
//! argument, the initialiser of an annotated variable, or the value of an
//! object-literal member.
//!
//! Every assertion here is on a parameter that has **no annotation**, because
//! that is the only case contextual typing can move: an annotated parameter
//! answers from its annotation and would pass identically against a checker with
//! no contextual typing at all. For the same reason no fixture uses a contextual
//! type of `any` — the pre-port answer *was* `any`, so such a fixture would be
//! decoration.

use tsr_checker::Checker;
use tsr_core::Arena;

#[test]
fn a_type_predicate_contributes_its_asserted_type_to_return_inference() {
    let source = "declare const guard: (input: unknown) => input is number;
        declare function infer<T>(callback: (value: unknown) => value is T): T;
        const observed = infer(guard);";
    assert_eq!(type_of_with_options(source, "observed", false, true), "number");
}

#[test]
fn predicate_inference_matches_parameter_positions_with_different_names() {
    let source = "declare const guard: (unused: unknown, input: unknown) => input is string;
        declare function infer<T>(callback: (first: unknown, value: unknown) => value is T): T;
        const observed = infer(guard);";
    assert_eq!(type_of_with_options(source, "observed", false, true), "string");
}

#[test]
fn a_rejected_overload_does_not_supply_the_next_callbacks_parameter_type() {
    let source = "declare function choose<T>(mode: 'first', callback: (input: string) => T): T;
        declare function choose<T>(mode: 'second', callback: (input: number) => T): T;
        const result = choose('second', observed => observed);";
    assert_eq!(type_of_with_options(source, "observed", false, true), "number");
    assert_eq!(type_of_with_options(source, "result", false, true), "number");
}

#[test]
fn contextual_overload_selection_accepts_a_callback_with_fewer_parameters() {
    let source = "declare function choose<T>(mode: 'first', callback: (input: string, index: number) => T): T;
        declare function choose<T>(mode: 'second', callback: (input: number, index: number) => T): T;
        const result = choose('second', observed => observed);";
    assert_eq!(type_of_with_options(source, "observed", false, true), "number");
    assert_eq!(type_of_with_options(source, "result", false, true), "number");
}

#[test]
fn a_generic_arrow_does_not_inherit_contextual_parameter_types() {
    let source = "const callback: (input: number) => number = <T>(observed) => 0;";
    assert_eq!(type_of(source, "observed"), "any");
}

#[test]
fn a_parenthesized_callback_consumes_the_generic_inference() {
    let source = "declare function invoke<T>(value: T, callback: (input: T) => T): T;
        invoke(1, (observed => observed));";
    assert_eq!(type_of_with_options(source, "observed", false, true), "number");
}

#[test]
fn conditional_callbacks_consume_the_generic_inference() {
    let source = "declare const choose: boolean;
        declare function invoke<T>(value: T, callback: (input: T) => T): T;
        invoke(1, choose ? observed => observed : other => other);";
    assert_eq!(type_of_with_options(source, "observed", false, true), "number");
    assert_eq!(type_of_with_options(source, "other", false, true), "number");
}

#[test]
fn logical_callback_operands_inherit_the_generic_call_context() {
    let source = "declare const fallback: (input: number) => number;
        declare function invoke<T>(value: T, callback: (input: T) => T): T;
        invoke(1, fallback || (observed => observed));";
    assert_eq!(type_of_with_options(source, "observed", false, true), "number");
}

#[test]
fn a_nullish_callback_operand_inherits_the_variables_annotation() {
    let source = "declare const fallback: ((input: string) => number) | undefined;
        const callback: (input: string) => number = fallback ?? (observed => 0);";
    assert_eq!(type_of(source, "observed"), "string");
}

#[test]
fn an_uncontextualized_or_uses_the_left_operands_type_for_the_right() {
    let source = "declare const fallback: ((input: boolean) => void) | undefined;
        const callback = fallback || (observed => {});";
    assert_eq!(type_of(source, "observed"), "boolean");
}

#[test]
fn logical_and_types_only_the_right_callback_from_the_outer_context() {
    let source = "declare const choose: boolean;
        const callback: (input: string) => number = choose && (observed => 0);";
    assert_eq!(type_of(source, "observed"), "string");
}

#[test]
fn a_logical_assignment_types_its_right_callback_from_the_target() {
    let source = "let callback: (input: number) => number;
        callback ||= observed => observed;";
    assert_eq!(type_of(source, "observed"), "number");
}

#[test]
fn returning_a_context_sensitive_function_defers_its_producer() {
    let source = "declare function invoke<T>(value: T, producer: () => (input: T) => T): T;
        invoke(1, () => observed => observed);";
    assert_eq!(type_of_with_options(source, "observed", false, true), "number");
}

#[test]
fn a_block_returning_a_context_sensitive_function_defers_its_producer() {
    let source = "declare function invoke<T>(value: T, producer: () => (input: T) => T): T;
        invoke(1, function () { return observed => observed; });";
    assert_eq!(type_of_with_options(source, "observed", false, true), "number");
}

#[test]
fn return_only_inference_keeps_noncontextual_returned_functions() {
    let source = "declare function useMemo<T>(produce: () => T): T;
        function make(choose: boolean) {
            const callback: (input: string) => boolean = useMemo(() => {
                if (choose) return () => true;
                return observed => observed.length > 0;
            });
            return callback;
        }";
    assert_eq!(type_of_with_options(source, "observed", false, true), "any");
}

#[test]
fn a_fixing_read_with_no_candidates_keeps_its_fallback() {
    let source = "declare function define<T>(options: {
            consume(value: T): void; produce(this: { value: any }): T;
        }): T;
        define({
            consume(observed) {},
            produce() { return this.value; }
        });";
    assert_eq!(type_of_with_options(source, "observed", false, true), "unknown");
}

#[test]
fn a_written_parameter_type_contributes_before_an_untyped_sibling_fixes() {
    let source = "interface Params { one: number; two: string }
        declare function define<T, R>(options: {
            fetch(params: T, extra: number): R; consume(value: R): void;
        }): T;
        const observed = define({
            fetch(params: Params, extra) { return 123; }, consume(value) {}
        });";
    assert_eq!(type_of_with_options(source, "observed", false, true), "Params");
}

#[test]
fn yielded_callbacks_receive_the_generators_contextual_yield_type() {
    let source = "interface Iterator<T, R = any, N = any> { next(): { value: T } }
        interface Generator<T, R = any, N = any> extends Iterator<T, R, N> {}
        function* produce(): Iterator<(input: string) => number, any, any> {
            yield observed => 0;
        }";
    assert_eq!(type_of_with_options(source, "observed", false, true), "string");
}

#[test]
fn rest_tuple_callbacks_align_the_fixed_suffix_from_the_end() {
    let source = "interface Array<T> { length: number; [index: number]: T }
        type Callbacks = [...((input: number) => void)[], (output: string) => void];
        declare function invoke(...callbacks: Callbacks): void;
        invoke(first => first, middle => middle, last => last);";
    assert_eq!(type_of(source, "first"), "number");
    assert_eq!(type_of(source, "middle"), "number");
    assert_eq!(type_of(source, "last"), "string");
}

#[test]
fn contextual_rest_parameters_include_the_remaining_fixed_prefix() {
    let source = "interface Array<T> { [index: number]: T }
        const callback: (...args: [number, boolean, ...string[]]) => void =
            (first, ...remaining) => {};";
    assert_eq!(type_of(source, "first"), "number");
    assert_eq!(type_of(source, "remaining"), "[boolean, ...string[]]");
}

#[test]
fn contextual_positions_follow_a_leading_parameter_before_a_tuple_rest() {
    let source = "interface Array<T> { [index: number]: T }
        const callback: (head: number, ...args: [boolean, ...string[]]) => void =
            (first, second, ...remaining) => {};";
    assert_eq!(type_of(source, "first"), "number");
    assert_eq!(type_of(source, "second"), "boolean");
    assert_eq!(type_of(source, "remaining"), "string[]");
}

#[test]
fn a_contextual_rest_parameter_can_capture_ordinary_signature_parameters() {
    let source = "const callback: (head: number, tail: string) => void = (...remaining) => {};";
    assert_eq!(type_of(source, "remaining"), "[head: number, tail: string]");
}

#[test]
fn ordinary_contextual_parameters_preserve_deferred_template_literal_types() {
    let source = "const callback: (input: `${number}`) => void = observed => {};";
    assert_eq!(type_of(source, "observed"), "`${number}`");
}

#[test]
fn immediately_invoked_parameters_expand_mixed_variadic_tuple_arguments() {
    let source = "interface Array<T> { [index: number]: T }
        declare const supplied: [boolean, ...string[]];
        (function(first, second, third, ...remaining) {})(1, ...supplied);";
    assert_eq!(type_of(source, "first"), "number");
    assert_eq!(type_of(source, "second"), "boolean");
    assert_eq!(type_of(source, "third"), "string");
    assert_eq!(type_of(source, "remaining"), "string[]");
}

#[test]
fn immediately_invoked_rest_parameters_preserve_generic_spread_tails() {
    let source = "interface Array<T> { [index: number]: T }
        function invoke<T extends any[]>(supplied: T) {
            (function(first, ...remaining) {})(1, 2, ...supplied);
        }";
    assert_eq!(type_of(source, "first"), "number");
    assert_eq!(type_of(source, "remaining"), "[number, ...T]");
}

#[test]
fn immediately_invoked_tuple_spreads_retain_optional_element_types() {
    let source = "declare const supplied: [number, string?];
        (function(first, second, ...remaining) {})(...supplied);";
    assert_eq!(type_of(source, "first"), "number");
    assert_eq!(type_of(source, "second"), "string | undefined");
    assert_eq!(type_of(source, "remaining"), "[]");
}

#[test]
fn array_tuple_context_uses_both_fixed_ends() {
    let source = "interface Array<T> { length: number; [index: number]: T }
        type Callbacks = [(start: boolean) => void, ...((input: number) => void)[], (end: string) => void];
        const callbacks: Callbacks = [first => first, middle => middle, last => last];";
    assert_eq!(type_of(source, "first"), "boolean");
    assert_eq!(type_of(source, "middle"), "number");
    assert_eq!(type_of(source, "last"), "string");
}

#[test]
fn a_spread_before_a_fixed_tuple_element_receives_all_possible_types() {
    let source = "interface Array<T> { length: number; [index: number]: T }
        declare const supplied: ((input: string) => void)[];
        const callbacks: [(head: string) => void, number] = [...supplied, final => final];";
    assert_eq!(type_of(source, "final"), "string");
}

#[test]
fn a_union_of_tuple_contexts_maps_each_constituent() {
    let source = "const callbacks: [number] | [(input: string) => void] = [item => item];";
    assert_eq!(type_of(source, "item"), "string");
}

#[test]
fn object_literal_method_parameters_take_the_matching_property_context() {
    let source = "interface Handlers { process(input: string): number }
        const handlers: Handlers = { process(value): number { return value.length; } };";
    assert_eq!(type_of(source, "value"), "string");
}

#[test]
fn object_literal_method_context_instantiates_a_generic_receiver() {
    let source = "interface Handlers<T> { process(input: T): void }
        const handlers: Handlers<number> = { process(value) { value; } };";
    assert_eq!(type_of(source, "value"), "number");
}

#[test]
fn a_producer_method_contributes_to_its_generic_sibling_context() {
    let source = "interface Options<T> { produce(): T; consume(input: T): void }
        declare function define<T>(options: Options<T>): T;
        define({ consume(value) {}, produce() { return 42; } });";
    assert_eq!(type_of_with_options(source, "value", false, true), "number");
}

#[test]
fn a_class_method_does_not_receive_object_literal_method_context() {
    let source = "interface Handlers { process(input: string): void }
        class Handler implements Handlers { process(value) { value; } }";
    assert_eq!(type_of(source, "value"), "any");
}

#[test]
fn template_substitution_context_skips_the_strings_argument() {
    let source = "declare function tag(strings: any, a: (head: number) => void, b: (tail: string) => void): void;
        tag`first ${first => first} second ${second => second}`;";
    assert_eq!(type_of(source, "first"), "number");
    assert_eq!(type_of(source, "second"), "string");
}

#[test]
fn template_substitution_context_uses_written_generic_arguments() {
    let source = "declare function tag<T>(strings: any, callback: (input: T) => void): void;
        tag<boolean>`value ${item => item}`;";
    assert_eq!(type_of(source, "item"), "boolean");
}

#[test]
fn consuming_a_callback_context_resolves_all_candidates_before_widening() {
    let source = "declare function invoke<T>(first: T, second: T, callback: (input: T) => T): T;
        invoke(1, 2, observed => observed);";
    assert_eq!(type_of_with_options(source, "observed", false, true), "number");
}

#[test]
fn a_primitive_constraint_keeps_a_combined_literal_callback_context() {
    let source = "declare function invoke<T extends number>(first: T, second: T, callback: (input: T) => T): T;
        invoke(1, 2, observed => observed);";
    assert_eq!(type_of_with_options(source, "observed", false, true), "1 | 2");
}

#[test]
fn a_const_parameter_keeps_its_literal_callback_context() {
    let source = "declare function invoke<const T>(value: T, callback: (input: T) => T): T;
        invoke(1, observed => observed);";
    assert_eq!(type_of_with_options(source, "observed", false, true), "1");
}

#[test]
fn nullable_candidates_preserve_the_nonnullable_callback_context() {
    let source = "interface A { a: A } interface B extends A { b: number }
        declare const b: B;
        declare function f<T, U>(first: T, callback: (input: T) => U, last: T): [T, U];
        f(b, observed => observed.a, null);";
    assert_eq!(type_of_with_null_checks(source, "observed", false), "B");
}

#[test]
fn fixing_an_object_callback_preserves_its_producer_inference() {
    let source =
        "interface Opts<P, D, M> { fetch: (params: P, other: number) => D; map: (data: D) => M }
        declare function example<P, D, M>(options: Opts<P, D, M>): (params: P) => M;
        interface Params { one: number; two: string }
        example({ fetch: (params: Params) => 123, map: observed => observed });";
    assert_eq!(type_of_with_options(source, "observed", false, true), "number");
}

#[test]
fn fixing_an_object_callback_uses_its_context_sensitive_producer_inference() {
    let source =
        "interface Opts<P, D, M> { fetch: (params: P, other: number) => D; map: (data: D) => M }
        declare function example<P, D, M>(options: Opts<P, D, M>): (params: P) => M;
        interface Params { one: number; two: string }
        example({ fetch: (params: Params, other) => 123, map: observed => observed });";
    assert_eq!(type_of_with_options(source, "observed", false, true), "number");
}

#[test]
fn a_chain_of_contextual_producers_infers_each_successive_callback() {
    let source = "type Chain<A, B> = { a(): A; b(input: A): B; c(input: B): void };
        declare function run<A, B>(options: Chain<A, B>): void;
        run({ a: () => 0, b: previous => 'a', c: observed => {} });";
    assert_eq!(type_of_with_options(source, "observed", false, true), "string");
}

#[test]
fn a_chain_of_contextual_methods_infers_each_successive_callback() {
    let source = "type Chain<A, B> = { a(): A; b(input: A): B; c(input: B): void };
        declare function run<A, B>(options: Chain<A, B>): void;
        run({ a() { return 0; }, b(previous) { return 'a'; }, c(observed) {} });";
    assert_eq!(type_of_with_options(source, "observed", false, true), "string");
}

#[test]
fn a_context_sensitive_method_produces_its_siblings_inference() {
    let source = "interface Opts<P, D, M> { fetch(params: P, other: number): D; map(data: D): M }
        declare function example<P, D, M>(options: Opts<P, D, M>): (params: P) => M;
        interface Params { one: number; two: string }
        example({ fetch(params: Params, other) { return 123; }, map(observed) { return observed; } });";
    assert_eq!(type_of_with_options(source, "observed", false, true), "number");
}

#[test]
fn a_later_method_cannot_replace_an_inference_fixed_by_an_earlier_method() {
    let source = "interface Int<T, U> { method(input: T): U }
        declare function run<T, U>(first: T, second: Int<T, U>, third: Int<U, T>): T;
        run('', { method(observed) { return observed.length; } }, { method(other) { return undefined; } });";
    assert_eq!(type_of_with_null_checks(source, "observed", false), "string");
}

#[test]
fn a_callback_reads_a_later_noncontextual_producer_before_fixing() {
    let source = "declare function run<T = unknown>(options: { consume: (input: T) => void; produce: () => T }): void;
        run({ consume: observed => {}, produce: () => 123 });";
    assert_eq!(type_of_with_options(source, "observed", false, true), "number");
}

#[test]
fn error_inference_keeps_noncontextual_object_members_as_sources() {
    let source = "enum First { X } enum Second { X }
        declare function run<T, U>(options: { write: (input: T) => U; read: () => T }, other: T): U;
        run({ write: observed => observed, read: () => First.X }, Second.X);";
    assert_eq!(type_of_with_options(source, "observed", false, true), "First");
}

#[test]
fn an_untagged_template_does_not_contextually_type_a_substitution() {
    let source = "const text = `value ${item => item}`;";
    assert_eq!(type_of(source, "item"), "any");
}

/// The printed type of the local `name`, wherever in the file it is declared.
///
/// The same walk `tests/members.rs` uses. It finds the parameter's symbol
/// through `lookup_local`, so the assertion is on `getTypeOfSymbol` of the
/// parameter itself rather than on a line that could be right for a reason
/// elsewhere.
///
/// # It returns the *first* match, and that is a trap these fixtures avoid
///
/// A contextual-typing fixture names the same thing twice by nature — once on
/// the callee's function-type annotation, once on the arrow. Reusing the name
/// makes this walk answer from the **annotation**, which prints the expected
/// string whether or not contextual typing exists. These tests were written that
/// way first, and one of them passed against a checker that could not have known
/// the answer. Every arrow parameter below therefore has a name that appears
/// nowhere else in its fixture.
fn type_of(source: &str, name: &str) -> String {
    type_of_with_implicit_any_check(source, name, false)
}

fn type_of_with_implicit_any_check(source: &str, name: &str, no_implicit_any: bool) -> String {
    type_of_with_options(source, name, no_implicit_any, false)
}

fn type_of_with_options(
    source: &str,
    name: &str,
    no_implicit_any: bool,
    resolve_calls: bool,
) -> String {
    type_of_with_compiler_options(source, name, no_implicit_any, resolve_calls, true)
}

fn type_of_with_null_checks(source: &str, name: &str, strict_null_checks: bool) -> String {
    type_of_with_compiler_options(source, name, false, true, strict_null_checks)
}

fn type_of_with_compiler_options(
    source: &str,
    name: &str,
    no_implicit_any: bool,
    resolve_calls: bool,
    strict_null_checks: bool,
) -> String {
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
    if no_implicit_any || !strict_null_checks {
        checker.apply_compiler_options(&tsr_core::CompilerOptions {
            no_implicit_any: if no_implicit_any {
                tsr_core::Tristate::True
            } else {
                tsr_core::Tristate::False
            },
            strict_null_checks: if strict_null_checks {
                tsr_core::Tristate::True
            } else {
                tsr_core::Tristate::False
            },
            ..Default::default()
        });
    }
    if resolve_calls {
        for index in 0..parsed.nodes.len() {
            let id = tsr_ast::NodeId::new(u32::try_from(index).unwrap());
            if let Some(tsr_ast::Node::CallExpression(call)) = parsed.node_map.get(id) {
                checker.check_expression(tsr_ast::Expression::CallExpression(call));
            }
        }
    }
    for index in 0..parsed.nodes.len() {
        #[allow(clippy::cast_possible_truncation)]
        let id = tsr_ast::NodeId::new(index as u32);
        if let Some(symbol) = bound.lookup_local(id, name) {
            let ty = checker.get_type_of_symbol(symbol);
            return checker.type_to_string(ty);
        }
    }
    panic!("`{name}` is declared nowhere");
}

/// The whole point: without contextual typing this is the implicit `any`.
///
/// `string` is not reachable from anything local to the arrow — the arrow is
/// `item => item`, so an implementation that guessed from the body, from the
/// arrow's own arity, or from the callee's *return* type would answer something
/// else. It has to come from `callback`'s parameter.
#[test]
fn a_callback_parameter_is_typed_from_the_callee_s_parameter() {
    let source = "declare function each(callback: (declared: string) => void): void;\n\
                  each(item => item);";
    assert_eq!(type_of(source, "item"), "string");
}

/// The index has to be the *argument's* index, not the parameter's and not zero.
///
/// Two callbacks with different parameter types, in a callee that takes both.
/// An implementation that always read the callee's first parameter answers
/// `number` for `second`; one that read the arrow's own position within its own
/// parameter list answers `number` too. Only threading argument index → callee
/// parameter → that parameter's signature answers `boolean`.
#[test]
fn the_second_callback_argument_is_typed_from_the_second_parameter() {
    let source = "declare function pair(a: (p: number) => void, b: (q: boolean) => void): void;\n\
                  pair(first => first, second => second);";
    assert_eq!(type_of(source, "first"), "number");
    assert_eq!(type_of(source, "second"), "boolean");
}

/// A generic callee stays the implicit `any`, and does not become `T`.
///
/// This is the guard on the reduction the module documents. `run`'s callback
/// parameter type is a type parameter, so reading it would print `T` — a
/// confident wrong answer where `any` is the honest pre-inference one. The
/// fixture discriminates because `T` and `any` are different strings.
#[test]
fn a_generic_callee_supplies_uninstantiated_context() {
    // The summit ladder, COMPLETE — each rung once pinned by this test:
    // "any" (the pre-summit standalone road), then "T" (the §75 adoption
    // through the callres2 memo), and now "unknown" — arm (a)'s
    // FIXING-FILL (`getInferredType`'s final leg, `inference.go:1317`):
    // a context consumed with no inference candidates fixes its type
    // parameters to `unknown`, which is upstream's final answer.
    let source = "declare function run<T>(callback: (declared: T) => void): void;\n\
                  run(value => value);";
    assert_eq!(type_of(source, "value"), "unknown");
}

/// An annotated parameter is unaffected, and the annotation wins.
///
/// Upstream reads the annotation first (`checker.go:16693`) and never consults
/// the context. The contextual type here is `string` and the annotation is
/// `number`, so the two answers differ and the ordering is observable rather
/// than assumed.
#[test]
fn an_annotation_wins_over_the_contextual_type() {
    let source = "declare function each(callback: (declared: string) => void): void;\n\
                  each((item: number) => item);";
    assert_eq!(type_of(source, "item"), "number");
}

/// The defect this module was written to close, stated as the member access it
/// produces.
///
/// `crate::members` answers `any` for a property access on an `any` receiver,
/// which is upstream-faithful — the wrongness was that the receiver reached it
/// as `any` in the first place. With the parameter typed from context the
/// receiver is a real type and the member is looked up for real, so `length` is
/// `number`. A checker without contextual typing prints `any` for both.
#[test]
fn a_member_of_a_contextually_typed_parameter_resolves_for_real() {
    let source = "interface Box { length: number; }\n\
                  declare function each(callback: (declared: Box) => void): void;\n\
                  each(item => { const size = item.length; });";
    assert_eq!(type_of(source, "size"), "number");
}

/// The second arm: an annotated variable declaration, not a call.
///
/// No call machinery is involved at all — the contextual type *is* the
/// annotation (`checker.go:29440`). The arrow is `item => item`, so `string` is
/// reachable from nothing local to it.
#[test]
fn an_annotated_variable_types_its_initialiser_s_parameter() {
    let source = "const f: (declared: string) => void = item => item;";
    assert_eq!(type_of(source, "item"), "string");
}

/// The third arm: a member of an object literal that is itself contextually
/// typed, here by a variable annotation.
///
/// The members are written in the **opposite order** to the interface, so an
/// implementation that took the contextual type's first property — or the object
/// literal's own member index — answers `string` for `solo` and `boolean` for
/// `duo`. Only looking the property up **by name** answers the other way round.
///
/// `Pair`'s own parameter names are `declaredA`/`declaredB` and appear nowhere
/// else, so the harness's first-local walk cannot answer from the annotation.
#[test]
fn an_object_literal_member_is_typed_from_the_property_of_the_same_name() {
    let source = "interface Pair { alpha: (declaredA: string) => void; beta: (declaredB: boolean) => void; }\n\
                  const pair: Pair = { beta: solo => solo, alpha: duo => duo };";
    assert_eq!(type_of(source, "solo"), "boolean");
    assert_eq!(type_of(source, "duo"), "string");
}

/// The same arm reached through a **call**, which is what the
/// `getContextualType` dispatch buys over the two-arm original.
///
/// The object literal has no annotation of its own; its contextual type is the
/// callee's parameter, and only then is `onTick` looked up in it. Two levels of
/// recursion, and `string` is reachable from nothing local to the arrow.
#[test]
fn an_object_literal_argument_types_its_member_through_the_callee() {
    let source = "interface Handlers { onTick: (declaredT: string) => void; }\n\
                  declare function attach(config: Handlers): void;\n\
                  attach({ onTick: value => value });";
    assert_eq!(type_of(source, "value"), "string");
}

/// A member the contextual type does not have invents nothing.
///
/// Upstream falls back to an index signature here (`checker.go:29946`); this
/// port does not port that fallback, so the member has no contextual type and
/// the parameter stays the implicit `any` — the same answer as before the arm
/// existed. The control that the fixture is otherwise sound is the sibling
/// `known` member, which does resolve.
#[test]
fn a_member_absent_from_the_contextual_type_stays_the_implicit_any() {
    let source = "interface Known { known: (declaredK: string) => void; }\n\
                  const obj: Known = { known: yes => yes, absent: nope => nope };";
    assert_eq!(type_of(source, "yes"), "string");
    assert_eq!(type_of(source, "nope"), "any");
}

/// A variable with no annotation supplies no contextual type.
///
/// `const h = item => item` is the probe `members.rs` records as producing
/// `x.foo : any`; it stays the implicit `any`, because there is no annotation to
/// read and inferring one is not this arm's job.
#[test]
fn an_unannotated_variable_supplies_no_contextual_type() {
    let source = "const h = solo => solo;";
    assert_eq!(type_of(source, "solo"), "any");
}

/// A string-literal property name is keyed the same as an identifier one.
///
/// Kept as a test rather than assumed, because `get_property_of_type` is keyed
/// by source text and a quoted name could have carried its quotes. It does not:
/// `StringLiteral::text` is already unquoted. Without this the
/// `PropertyName::StringLiteral` arm would be an unfalsifiable branch, and this
/// module deletes those rather than leaving them as evidence.
#[test]
fn a_string_literal_property_name_resolves_the_same_as_an_identifier() {
    let source = "interface Known { known: (declaredK: string) => void; }\n\
                  const obj: Known = { \"known\": yes => yes };";
    assert_eq!(type_of(source, "yes"), "string");
}

/// §768: `getContextuallyTypedParameterType`'s IIFE arm
/// (`checker.go:29463`-`:29484`). An immediately-invoked function's
/// parameters take their types from the CALL's arguments, widened — a road
/// entirely separate from the contextual-signature one the tests above walk.
///
/// The "same name twice" trap this file warns about cannot fire here: an IIFE
/// fixture has no annotation anywhere, so there is nothing for the walk to
/// read the expected answer off.
///
/// Found by §767: after `depend.rs` grew step arms, an un-annotated parameter
/// became the gap board's second-largest root, with `contextuallyTypedIife`
/// at its head.
///
/// Reddened by: removing the IIFE arm.
#[test]
fn an_iife_parameter_takes_its_type_from_the_argument() {
    assert_eq!(type_of("((jake) => jake)(\"build\");", "jake"), "string");
    // Widened: the argument's literal type does not survive.
    assert_eq!(type_of("((cats) => cats)(101);", "cats"), "number");
    // Positional across several arguments.
    let three = "((alpha, beta, gamma) => gamma)(\"foo\", 101, false);";
    assert_eq!(type_of(three, "alpha"), "string");
    assert_eq!(type_of(three, "beta"), "number");
    assert_eq!(type_of(three, "gamma"), "boolean");
    // Parentheses between the function and the call are looked through
    // (`contextuallyTypedIife`'s "Lots of Irritating Superfluous Parentheses").
    assert_eq!(type_of("((((zeta) => zeta))(\"!\"));", "zeta"), "string");
    // Past the arguments, with no initializer: `undefined`.
    assert_eq!(type_of("((kappa?) => kappa)();", "kappa"), "undefined");
    // §771: a REST parameter takes the TUPLE of the remaining arguments'
    // widened types (`getSpreadArgumentType` with `anyType` as the rest type).
    // §768 asserted `any` here and said the arm declined; that decline ended
    // when §770 unblocked it.
    assert_eq!(type_of("((...omega) => omega)(5, 6);", "omega"), "[number, number]");
    // §887 ends that decline. A SPREAD argument reaches the same answer as the
    // positional one above, because `isSpreadIntoCallOrNew` (`checker.go:8117`)
    // puts the spread literal in TUPLE context, so `[5, 6]` arrives as
    // `[number, number]` rather than widened to `number[]`. This line asserted
    // `any` as a pinned decline until then.
    assert_eq!(type_of("((...sigma) => sigma)(...[5, 6]);", "sigma"), "[number, number]");
}

/// §927 — a UNION contextual type supplies an object literal's members.
///
/// `getTypeOfPropertyOfContextualTypeEx` (`checker.go:30555`) maps over the
/// union's constituents. The port had that walk as
/// `Checker::contextual_property_type` and the object-literal caller reached for
/// `get_property_of_type` instead, which finds nothing on a union — so every
/// member of a literal under `I1<T> | I2<T>` answered `error` while the
/// identical literal under one constituent typed correctly. Measured on the
/// corpus: **123 `WRONG->RIGHT`, zero `RIGHT->WRONG`**,
/// `conformance/contextualTypeWithUnionTypeMembers` 86 of them.
#[test]
fn a_union_contextual_type_types_an_object_literal_method() {
    let source = "interface I1 { f: (a: string) => string; }\n\
                  interface I2 { f: (a: string) => string; }\n\
                  var v: I1 | I2 = { f: spruce => spruce };";
    assert_eq!(type_of(source, "spruce"), "string");
}

/// The single-constituent road, unchanged. `get_property_of_type` still runs
/// first and the union walk is consulted only on a miss, so this fixture must
/// answer exactly as it did before §927 — without it the test above could pass
/// on a rewrite that broke the common case.
#[test]
fn a_single_constituent_contextual_type_is_unchanged() {
    let source = "interface I1 { f: (a: string) => string; }\n\
                  var v: I1 = { f: larch => larch };";
    assert_eq!(type_of(source, "larch"), "string");
}

/// **§938 REMOVED the guard this test was written for**, so it now asserts the
/// answer the discriminating road gives. `discriminateTypeByDiscriminableItems`
/// (`checker.go:30779`) reaches the shape the guard protected, and removing the
/// decline measured **+6 `WRONG->RIGHT`, zero adverse** — §927's own prediction
/// that *"removing both guards is how you would know discrimination had landed"*,
/// confirmed for this half.
///
/// Historical note kept: `compiler/contextualOverloadListFromUnionWithPrimitive`
/// `NoImplicitAny` is a regression test for exactly this shape: a union with a
/// PRIMITIVE constituent supplies **no** contextual type upstream, and the
/// parameters are implicit `any` — which is what the case is named for. The
/// undiscriminated walk found the object constituent's member and typed them, 4
/// rows `RIGHT->WRONG`.
#[test]
fn a_union_with_a_primitive_constituent_now_supplies_the_object_constituent() {
    let source = "interface I1 { f: (a: string) => string; }\n\
                  var v: string | I1 = { f: cedar => cedar };";
    assert_eq!(type_of(source, "cedar"), "string");
}

/// §927's second guard, and §98's generalised. Which constituent governs a
/// literal-valued member is precisely what `discriminateTypeByDiscriminableItems`
/// (`checker.go:30779`) decides, and that is unported: `missingDiscriminants`
/// writes `const item1: Item = { subkind: 1, kind: "b" }` where the constituents
/// declare `subkind: 0` and `subkind: 1`, upstream discriminates to the one with
/// no `subkind` at all, and the literal widens to `number`. The walk unioned
/// `0 | 1` and kept `1` fresh — 15 rows `RIGHT->WRONG`. Declining costs 11 wins
/// and is the honest answer until discrimination lands.
#[test]
fn a_unit_member_out_of_a_multi_constituent_union_declines() {
    let source = "interface I1 { k: 0; f: (a: string) => string; }\n\
                  interface I2 { k: 1; f: (a: string) => string; }\n\
                  var v: I1 | I2 = { k: 1, f: alder => alder };";
    // `f` still types — the guard is per member, not per literal.
    assert_eq!(type_of(source, "alder"), "string");
}

/// The other half of §927: `compareSignaturesIdentical` (`relater.go:3103`) in
/// `contextual_signature`'s union branch. Two constituents offering the SAME
/// signature now combine; two offering DIFFERENT ones still decline, because
/// `createUnionSignature` is not ported and picking one would be a guess.
#[test]
fn union_constituents_with_different_signatures_still_decline() {
    let source = "interface I1 { f: (a: string) => string; }\n\
                  interface I2 { f: (a: number) => number; }\n\
                  var v: I1 | I2 = { f: fir => fir };";
    assert_eq!(type_of(source, "fir"), "any");
}

/// §928 — an object-literal method's `this` comes from the contextual
/// signature's `this` parameter.
///
/// `getContextualThisParameterType` (`checker.go:29104`) asks for the method's
/// own contextual signature before it falls to either object-literal branch.
/// §912's comment named this branch as the one that wins ahead of the literal
/// one and did not build it, so `this` answered `any` while the same annotation
/// written directly on a function typed correctly.
/// The assertion is on a LOCAL inside the method's body, because that is the
/// only thing this harness can see that the fix moves: `type_of(… "impl")`
/// answers `I` whether or not `this` resolves, and the first draft of this test
/// asserted exactly that — a test no mutation could redden.
#[test]
fn an_object_literal_method_adopts_the_contextual_this_parameter() {
    let source = "interface I { hemlock: number; em(this: { hemlock: number }): number; }\n\
                  let impl: I = { hemlock: 12, em() { let sequoia = this.hemlock; return sequoia; } };";
    assert_eq!(type_of(source, "sequoia"), "number");
}

/// The pair: with no contextual `this` parameter to inherit, the method keeps
/// the answer it had. Without this the test above would pass equally well if the
/// new branch simply returned the containing literal's type for every method.
///
/// Corrected during the class-this continuation: this lib-free harness reaches
/// globalThis for the method's unbound this. checkPropertyAccess recovers with
/// any for a missing global property, just as the corpus already did. The pair
/// still contrasts the inherited structural parameter above with an unbound
/// receiver here.
#[test]
fn a_method_with_no_contextual_this_parameter_is_unchanged() {
    let source = "interface J { hemlock: number; em(): number; }\n\
                  let impl: J = { hemlock: 12, em() { let redwood = this.hemlock; return redwood; } };";
    assert_eq!(type_of(source, "redwood"), "any");
}

// §928.1's printing half has **no seam this harness can reach**, and the two
// tests written for it are deleted rather than left passing for the wrong
// reason. They asserted the type of `impl.em` where `impl: I` — which reads
// `I`'s declared member, carrying the written `this` whatever the object
// literal's own method does, so the "thisless" fixture passed by inheriting
// from the annotation. What the change moves is the *inferred type of the
// object literal*, which `type_of` cannot name.
//
// Its evidence is the corpus measurement, recorded in
// `docs/architecture/checker-notes-deferred.md` §928.1: ungated **10
// `WRONG->RIGHT` against 12 `RIGHT->WRONG`**, gated on `NodeFacts::CONTAINS_THIS`
// **+3 with zero adverse**. The 12 that disappear are the falsifier.

/// §932.1 — a contextual signature read off a type literal, not just an arrow
/// form.
///
/// §932 found that §10.15's single-signature collapse records its signature in
/// `signature_types` without registering the type as *minted*, so every reader
/// gated on `is_instantiated_signature_type` never looked. The contextual side
/// was the second such reader:
///
/// ```ts
/// declare function f(cb: { (a: number): void }): void;   // was: any
/// declare function g(cb: (a: number) => void): void;     // was: number
/// ```
///
/// Two spellings of one type, two answers. `getSignaturesOfType`
/// (`checker.go:18959`) reads the type's signatures whatever minted it.
#[test]
fn a_contextual_signature_is_read_off_a_type_literal() {
    let source = "declare function f(cb: { (a: number): void }): void;\nf(oak => { oak; });";
    assert_eq!(type_of(source, "oak"), "number");
}

/// The same through an annotated variable rather than an argument, which is a
/// different arm of `get_contextual_type` reaching the same reader.
#[test]
fn an_annotated_variable_reads_a_type_literals_signature() {
    let source = "var h: { (a: number): void } = birch => { birch; };";
    assert_eq!(type_of(source, "birch"), "number");
}

/// The arrow-form spelling, which already worked and must keep working — without
/// it the two tests above could pass on a change that routed everything through
/// the new read.
#[test]
fn the_arrow_form_spelling_is_unchanged() {
    let source = "declare function g(cb: (a: number) => void): void;\ng(elm => { elm; });";
    assert_eq!(type_of(source, "elm"), "number");
}

/// §938 — `discriminateTypeByDiscriminableItems` (`checker.go:30779`) selects
/// the constituent the literal's own context-free members identify, and the
/// member lookup happens on that ONE type.
///
/// Its answer is authoritative **including a miss**: `missingDiscriminants`
/// writes `{ subkind: 1, kind: "b" }` against a union whose `kind: "b"`
/// constituent has no `subkind` at all, so upstream supplies no contextual type
/// and the literal widens.
#[test]
fn a_discriminated_union_supplies_the_selected_constituents_member() {
    let source = "interface A { kind: \"a\"; f: (a: string) => string; }\n\
                  interface B { kind: \"b\"; f: (a: number) => number; }\n\
                  var v: A | B = { kind: \"b\", f: maple => maple };";
    assert_eq!(type_of(source, "maple"), "number");
}

/// The other branch of the same fixture, so the test above cannot pass on an arm
/// that always picks one constituent.
#[test]
fn the_other_discriminant_selects_the_other_constituent() {
    let source = "interface A { kind: \"a\"; f: (a: string) => string; }\n\
                  interface B { kind: \"b\"; f: (a: number) => number; }\n\
                  var v: A | B = { kind: \"a\", f: willow => willow };";
    assert_eq!(type_of(source, "willow"), "string");
}

/// §940 — an OPTIONAL parameter contextually types its argument.
///
/// `contextual_type_for_argument`'s memo road declined `parameter.rest ||
/// parameter.optional`. The `rest` half is a real shape question; the
/// `optional` half was not — an optional parameter has a perfectly good declared
/// type and upstream contextually types its argument with it. Nothing about `?`
/// makes the position unreadable.
///
/// Measured: **77 `WRONG->RIGHT` + 34 `GAP->RIGHT` against 1 `GAP->WRONG`, zero
/// `RIGHT->WRONG`.**
#[test]
fn an_optional_parameter_contextually_types_its_argument() {
    let source = "declare function f(cb?: (a: number) => void): void;\nf(hazel => { hazel; });";
    assert_eq!(type_of(source, "hazel"), "number");
}

/// The pair: a REQUIRED parameter in the same position already worked, so the
/// test above cannot pass on a change that types every argument position.
#[test]
fn a_required_parameter_in_the_same_position_is_unchanged() {
    let source = "declare function f(cb: (a: number) => void): void;\nf(rowan => { rowan; });";
    assert_eq!(type_of(source, "rowan"), "number");
}

/// §945 — a function expression ASSIGNED to a property takes that property's
/// declared `this` parameter.
///
/// §928 ported `getContextualThisParameterType` for an object-literal method.
/// Upstream reaches both forms through the same function; the difference is only
/// which road supplies the contextual signature, and the assignment road was
/// never wired — so `impl.em = function () { return this.a; }` answered
/// `this : any` while `{ em() { return this.a } }` answered `{ a: number; }`.
#[test]
fn a_function_expression_assigned_to_a_property_adopts_its_this() {
    let source = "interface I { hazelnut: number; em(this: { hazelnut: number }): number; }\n\
                  declare const impl: I;\n\
                  impl.em = function () { let cedarv = this.hazelnut; return cedarv; };";
    assert_eq!(type_of(source, "cedarv"), "number");
}

/// The pair: the same assignment where the property's type carries **no** `this`
/// parameter is untouched, so the arm cannot be typing every assigned function.
#[test]
fn an_assigned_function_without_a_contextual_this_is_unchanged() {
    let source = "interface J { hazelnut: number; em(): number; }\n\
                  declare const impl: J;\n\
                  impl.em = function () { let rowanv = this.hazelnut; return rowanv; };";
    assert_eq!(type_of(source, "rowanv"), "any");
}

/// §945.1 — the contextual `this` comes from `get_contextual_type`, so every arm
/// that dispatch has works at once.
///
/// §945 wired the assignment road by hand. The sweep that followed found four
/// more entry points failing identically — an annotated variable's initialiser,
/// a call argument, an `as` assertion, and an array-literal element — and every
/// one of them already had an arm in `get_contextual_type`. Wiring them
/// individually would have been four more §945s.
#[test]
fn an_annotated_variables_function_initialiser_adopts_this() {
    let source = "type W = (this: { juniper: number }) => number;\n\
                  const f: W = function () { let larchx = this.juniper; return larchx; };";
    assert_eq!(type_of(source, "larchx"), "number");
}

/// A call argument, which is a different arm of the same dispatch.
#[test]
fn a_function_argument_adopts_the_parameters_this() {
    let source = "type W = (this: { juniper: number }) => number;\n\
                  declare function take(cb: W): void;\n\
                  take(function () { let cedarx = this.juniper; return cedarx; });";
    assert_eq!(type_of(source, "cedarx"), "number");
}

/// The pair: a contextual type carrying **no** `this` parameter leaves the
/// function alone, so the general road cannot be typing every function
/// expression that has a context.
#[test]
fn a_contextual_type_without_this_leaves_the_function_alone() {
    let source = "type N = () => number;\n\
                  const f: N = function () { let alderx = this; return 1; };";
    assert_eq!(type_of(source, "alderx"), "any");
}

/// §946 — a type variable constrained to a primitive is a literal context.
///
/// `isLiteralOfContextualType`'s `InstantiableNonPrimitive` arm
/// (`checker.go:25522`) consults the **base constraint**: upstream's own comment
/// is *"if the contextual type is a type variable constrained to a primitive
/// type, consider this a literal context for literals of that primitive type"*.
/// This port declined the arm outright.
#[test]
fn a_primitive_constrained_type_variable_keeps_a_literal() {
    let source = "declare function nested<A extends string>(a: { fields: A }): A;\n\
                  const beechv = nested({ fields: \"z\" });";
    assert_eq!(type_of(source, "beechv"), "\"z\"");
}

/// The pair: an UNCONSTRAINED type variable is `unknown` upstream, which matches
/// no primitive, so the literal widens. Without this the arm could be answering
/// "literal context" for every type variable.
#[test]
fn an_unconstrained_type_variable_does_not_keep_a_literal() {
    let source = "declare function nested<A>(a: { fields: A }): A;\n\
                  const mapleq = nested({ fields: \"z\" });";
    assert_eq!(type_of(source, "mapleq"), "string");
}

#[test]
fn a_generic_receivers_method_supplies_instantiated_callback_types() {
    let source = "interface Box<T> { apply(cb: (value: T) => void): void }\n\
                  declare const box: Box<string>; box.apply(item => item);";
    assert_eq!(type_of(source, "item"), "string");
}

#[test]
fn overloaded_generic_receiver_methods_keep_the_receivers_type_arguments() {
    let source = "interface Box<T> {\n\
                    apply(cb: (value: T) => boolean): boolean;\n\
                    apply(cb: (value: T) => unknown, context?: any): boolean;\n\
                  }\n\
                  declare const box: Box<string>; box.apply(item => true);";
    assert_eq!(type_of(source, "item"), "string");
}

#[test]
fn a_named_callable_interface_supplies_contextual_parameter_types() {
    let source = "interface Callback { (value: number): void }\n\
                  const callback: Callback = item => item;";
    assert_eq!(type_of(source, "item"), "number");
}

#[test]
fn a_generic_callable_interface_instantiates_its_contextual_signature() {
    let source = "interface Callback<T> { (value: T): void }\n\
                  const callback: Callback<string> = item => item;";
    assert_eq!(type_of(source, "item"), "string");
}

#[test]
fn a_callable_signatures_own_type_parameter_shadows_the_interfaces_parameter() {
    let source = "interface Callback<T> { <T>(value: T): void }\n\
                  const callback: Callback<string> = item => item;";
    assert_eq!(type_of(source, "item"), "T");
}

#[test]
fn contextual_union_signatures_ignore_return_types_when_comparing_parameters() {
    let source = "const callback: ((value: number) => string) | ((value: number) => number) =\n\
                  item => item;";
    assert_eq!(type_of(source, "item"), "number");
}

#[test]
fn contextual_overloads_filter_out_signatures_with_too_few_parameters() {
    let source = "interface Callback { (value: number): void; (value: string, extra: boolean): void }\n\
                  const callback: Callback = (item, extra) => item;";
    assert_eq!(type_of(source, "item"), "string");
    assert_eq!(type_of(source, "extra"), "boolean");
}

#[test]
fn contextual_overloads_union_corresponding_parameter_types() {
    let source = "interface Callback { (value: number): void; (value: string): void }\n\
                  const callback: Callback = item => item;";
    assert_eq!(type_of_with_implicit_any_check(source, "item", true), "string | number");
}

#[test]
fn contextual_overloads_decline_when_implicit_any_checking_is_disabled() {
    let source = "interface Callback { (value: number): void; (value: string): void }\n\
                  const callback: Callback = item => item;";
    assert_eq!(type_of(source, "item"), "any");
}

#[test]
fn incompatible_union_signature_parameters_leave_the_function_without_context() {
    let source = "const callback: ((value: number) => void) | ((value: string) => void) =\n\
                  item => item;";
    assert_eq!(type_of(source, "item"), "any");
}

#[test]
fn a_non_callable_union_constituent_does_not_remove_the_callable_context() {
    let source = "const callback: ((value: number) => void) | undefined = item => item;";
    assert_eq!(type_of(source, "item"), "number");
}

#[test]
fn optional_annotated_iife_parameters_respect_null_check_mode() {
    let null = "((observed?: null) => observed)(null as null);";
    let undefined = "((observed?: undefined) => observed)(undefined as undefined);";
    let number = "((observed?: number) => observed)(101);";
    for (strict, null_type, number_type) in
        [(false, "null", "number"), (true, "null | undefined", "number | undefined")]
    {
        assert_eq!(type_of_with_null_checks(null, "observed", strict), null_type);
        assert_eq!(type_of_with_null_checks(undefined, "observed", strict), "undefined");
        assert_eq!(type_of_with_null_checks(number, "observed", strict), number_type);
    }
}

#[test]
fn required_provided_and_defaulted_iife_parameters_do_not_gain_optionality() {
    for strict in [false, true] {
        for source in [
            "((observed: null) => observed)(null as null);",
            "((observed: null = null as null) => observed)();",
        ] {
            assert_eq!(type_of_with_null_checks(source, "observed", strict), "null");
        }
        assert_eq!(
            type_of_with_null_checks(
                "((observed: undefined) => observed)(undefined as undefined);",
                "observed",
                strict,
            ),
            "undefined"
        );
        for source in [
            "((observed) => observed)(101);",
            "((observed = 10) => observed)();",
            "((observed: number = 10) => observed)(undefined);",
        ] {
            assert_eq!(type_of_with_null_checks(source, "observed", strict), "number");
        }
    }
}

fn iife_parameter_ids(
    source: &str,
    name: &str,
    strict: bool,
) -> (tsr_checker::types::TypeId, tsr_checker::types::TypeId, tsr_checker::Intrinsics) {
    let arena = Arena::new();
    let parsed = tsr_parser::parse(&arena, source);
    assert!(parsed.diagnostics.is_empty());
    let bound = tsr_binder::bind(
        &arena,
        parsed.source_file,
        &parsed.nodes,
        tsr_binder::FileInfo { name: "test.ts", text: source },
    );
    let mut checker = Checker::new(&bound, &parsed.nodes, &parsed.node_map);
    checker.apply_compiler_options(&tsr_core::CompilerOptions {
        strict: tsr_core::Tristate::False,
        strict_null_checks: if strict {
            tsr_core::Tristate::True
        } else {
            tsr_core::Tristate::False
        },
        ..Default::default()
    });
    for index in 0..parsed.nodes.len() {
        let id = tsr_ast::NodeId::new(u32::try_from(index).unwrap());
        if let Some(tsr_ast::Node::ParameterDeclaration(parameter)) = parsed.node_map.get(id)
            && matches!(parameter.name, Some(tsr_ast::BindingName::Identifier(n)) if n.text == name)
        {
            let function = parsed.node_map.get(parsed.nodes.parent(id).unwrap()).unwrap();
            let function_type =
                checker.check_expression(tsr_ast::Expression::try_from(function).unwrap());
            // Public signatures already carry declaration-widened parameter
            // types. Raw helper identities are tested inside contextual.rs.
            let parameter = checker.signatures_of_type(function_type).unwrap()[0]
                .parameters
                .iter()
                .find(|p| p.name == name)
                .unwrap()
                .clone();
            let signature_type = checker.parameter_type(&parameter);
            let symbol_type = checker.get_type_of_symbol(bound.symbol_of(id).unwrap());
            return (signature_type, symbol_type, *checker.intrinsics());
        }
    }
    panic!("parameter {name} not found");
}

#[test]
fn omitted_iife_parameter_consumers_keep_their_null_mode_images() {
    for strict in [false, true] {
        for (source, name) in [
            ("((observed) => 42)();", "observed"),
            ("((observed?) => 42)();", "observed"),
            ("((observed) => 42)(...([] as []));", "observed"),
            ("((observed?) => 42)(...([] as []));", "observed"),
            ("((first, observed?) => 42)(73);", "observed"),
            ("((first, observed?) => 42)(...([73] as [number]));", "observed"),
            ("(function(undefined) { return 42; })();", "undefined"),
            ("(function(undefined?) { return 42; })(...([] as []));", "undefined"),
        ] {
            let (signature, parameter, i) = iife_parameter_ids(source, name, strict);
            assert_eq!(signature, if strict { i.undefined } else { i.any }, "{source}");
            assert_eq!(parameter, if strict { i.undefined } else { i.any });
        }
    }
}

#[test]
fn provided_iife_identities_and_omitted_defaults_remain_separate_from_missing_arguments() {
    for strict in [false, true] {
        for source in [
            "((observed) => observed)(undefined as undefined);",
            "((observed?) => observed)(undefined as undefined);",
            "((observed?) => observed)(...([undefined as undefined] as [undefined]));",
            "((observed = 37) => observed)(undefined as undefined);",
            // The synthetic global's type is `undefinedWideningType`
            // (`checker.go:1345`), the one argument that widens loose.
            "((observed?) => observed)(undefined);",
        ] {
            let (signature, parameter, i) = iife_parameter_ids(source, "observed", strict);
            // Native ordinary assertions stay undefined even loose: only
            // `createWideningType`'s twin widens to any (`getWidenedType`,
            // `checker.go:16090`; `tsgo` declares `a: undefined` for
            // `((observed) => observed)(undefined as undefined)` and `b: any`
            // for `((observed?) => observed)(undefined)` under
            // `strictNullChecks: false`).
            let widening = source.ends_with("(undefined);");
            let expected = if strict || !widening { i.undefined } else { i.any };
            assert_eq!(signature, expected, "strict={strict}: {source}");
            assert_eq!(parameter, expected);
        }
        for (source, name) in [
            ("((observed = 37) => observed)();", "observed"),
            ("((observed = 37) => observed)(...([] as []));", "observed"),
            ("(function(undefined) { return undefined; })(101);", "undefined"),
            ("(function(undefined = 37) { return undefined; })();", "undefined"),
        ] {
            let (contextual, parameter, i) = iife_parameter_ids(source, name, strict);
            assert_eq!(contextual, i.number, "strict={strict}: {source}");
            assert_eq!(parameter, i.number);
        }
        // The raw contextual argument is number in either mode, but the
        // optional public parameter also includes undefined in strict mode.
        assert_eq!(
            type_of_with_null_checks(
                "(function(undefined?) { return undefined; })(...([101] as [number]));",
                "undefined",
                strict,
            ),
            if strict { "number | undefined" } else { "number" },
        );
    }
}
