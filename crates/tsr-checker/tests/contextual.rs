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
    // A REST parameter declines rather than taking the positional answer —
    // it needs `getSpreadArgumentType`, which is not ported.
    assert_eq!(type_of("((...omega) => omega)(5, 6);", "omega"), "any");
}
