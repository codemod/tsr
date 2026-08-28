//! Inferring a function's return type from its body.
//!
//! Until this slice, *any* body containing a `return` gapped the whole signature
//! — `getSignaturesOfSymbol` answered `None` and `getTypeOfSymbol` turned that
//! into `errorType`. The recorded reason was that the aggregate is a union
//! (`checkAndAggregateReturnExpressionTypes`, `checker.go:20259`) and unions were
//! unported. Unions landed; subtype reduction did not, so what is answered here
//! is the aggregate that needs neither: **one distinct return type**.
//!
//! Two things the baselines decide and reasoning would get wrong:
//!
//! - a block-bodied *function declaration* widens its unit result, because
//!   `getContextualSignatureForFunctionLikeDeclaration` (`checker.go:29711`)
//!   contextually types only function expressions, arrows and object-literal
//!   methods. `conformance/logicalAssignment10(target=es2021).types:13` records
//!   `>incr : () => number` for exactly this shape;
//! - an arrow assigned to an *annotated* `const` does **not** widen, which is
//!   why the contextual test cannot simply be "is this a declaration".

use tsr_checker::Checker;
use tsr_core::Arena;

/// The printed type of the top-level declaration named `name`.
fn type_of_declaration(source: &str, name: &str) -> String {
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
    let root = tsr_ast::Node::SourceFile(parsed.source_file).node_id().expect("registered");
    let symbol = bound.lookup_local(root, name).unwrap_or_else(|| panic!("`{name}` is declared"));
    let mut checker = Checker::new(&bound, &parsed.nodes, &parsed.node_map);
    let id = checker.get_type_of_symbol(symbol);
    checker.type_to_string(id)
}

#[test]
fn a_body_whose_returns_yield_one_type_infers_it() {
    // The shape `conformance/logicalAssignment10(target=es2021).types:13`
    // records as `>incr : () => number`: a block-bodied function declaration
    // with a single `return` of a numeric expression.
    assert_eq!(type_of_declaration("function f() { return 1; }", "f"), "() => number");
    assert_eq!(
        type_of_declaration("function f(s: string) { return s; }", "f"),
        "(s: string) => string"
    );

    // `core.AppendIfUnique` (`checker.go:20295`): *three* returns of the same
    // type aggregate to one, so this needs no union either. This is the
    // assertion that makes the slice "one distinct type" rather than "one
    // `return` statement", and it fails if the aggregate is a count.
    assert_eq!(
        type_of_declaration(
            "function f(s: string, c: boolean) { if (c) { return s; } if (!c) { return s; } return s; }",
            "f"
        ),
        "(s: string, c: boolean) => string"
    );

    // A `return` with no expression anywhere in the body is `void`, and reaching
    // it proves the body's end is reachable — so it is not `never` even for a
    // shape `mayReturnNever` covers.
    assert_eq!(
        type_of_declaration("function f(c: boolean) { if (c) return; }", "f"),
        "(c: boolean) => void"
    );
}

#[test]
fn a_function_declaration_widens_its_unit_result_but_a_contextual_arrow_does_not() {
    // `getWidenedLiteralLikeTypeForContextualReturnTypeIfNeeded`
    // (`checker.go:20221`) widens a unit type unless a contextual signature
    // supplied one, and `getContextualSignatureForFunctionLikeDeclaration`
    // (`:29711`) hands one only to a function expression, an arrow or an
    // object-literal method.
    //
    // So the *same* `return "a"` widens in a declaration and does not under an
    // annotation. Getting this backwards prints `() => "a"` for every
    // `function f() { return "a"; }` in the corpus.
    assert_eq!(type_of_declaration(r#"function f() { return "a"; }"#, "f"), "() => string");
    assert_eq!(type_of_declaration(r#"const f = () => "a";"#, "f"), "() => string");
    // Annotated: a contextual signature exists, so the literal is kept — and
    // this port cannot see *what* it supplies, so it gaps rather than guessing.
    assert_eq!(type_of_declaration(r#"const f: () => "a" = () => "a";"#, "f"), r#"() => "a""#);
}

#[test]
fn an_aggregate_this_slice_cannot_reduce_is_still_a_gap() {
    // Two *distinct* return types need `getUnionTypeEx(types,
    // UnionReductionSubtype)` (`checker.go:20191`), and subtype reduction is
    // unported. An unreduced union would print constituents upstream collapses,
    // which is a wrong answer wearing the shape of a result.
    // §11's reduction now answers both of these — the seventeenth stand-in
    // pair to come due. Distinct primitives union unreduced (`string | number`
    // — nothing is a subtype of anything), and two numeric literals keep
    // their regular forms exactly as `capturedLetConstInLoop8.types` pins for
    // string literals.
    assert_eq!(
        type_of_declaration(
            "function f(s: string, n: number, c: boolean) { if (c) { return s; } return n; }",
            "f"
        ),
        "(s: string, n: number, c: boolean) => string | number"
    );
    assert_eq!(
        type_of_declaration("function f(c: boolean) { if (c) { return 1; } return 2; }", "f"),
        "(c: boolean) => 1 | 2"
    );

    // A bare `return;` beside a valued one is the one configuration where
    // `strictNullChecks` changes the answer (`checker.go:20301` appends
    // `undefinedType` under it and not otherwise). The old pin here was
    // `error` under the rationale "this port has no compiler options" —
    // expired by ADR-0042 and cashed in by §741: `Checker::new` defaults
    // `strict_null_checks` to `true`, so this harness takes the strict
    // spelling, the appended `undefined` beside the unwidened literal —
    // the same shape the `1 | 2` pin above records.
    assert_eq!(
        type_of_declaration("function f(c: boolean) { if (c) { return 1; } return; }", "f"),
        "(c: boolean) => 1 | undefined"
    );

    // Async and generator returns wrap in `Promise`/`Generator`, which are
    // globals this port cannot resolve (`bd tsr-9or.1`).
    assert_eq!(type_of_declaration("async function f() { return 1; }", "f"), "error");
    assert_eq!(type_of_declaration("function* f() { return 1; }", "f"), "error");
}

/// The printed type of `name`, bound beside a stand-in lib declaring `Promise`.
fn type_of_declaration_with_promise(source: &str, name: &str) -> String {
    let lib = "interface Promise<T> {}\n";
    let arena = Arena::new();
    let mut nodes = tsr_ast::NodeTable::new();
    let mut node_map = tsr_ast::NodeMap::new();
    let options = tsr_parser::ParseOptions::default();
    let lib_file = tsr_parser::parse_into(&arena, lib, options, &mut nodes, &mut node_map);
    let parsed = tsr_parser::parse_into(&arena, source, options, &mut nodes, &mut node_map);
    assert!(
        parsed.diagnostics.is_empty(),
        "fixture must parse: {:?}",
        parsed.diagnostics.iter().map(tsr_diagnostics::Diagnostic::text).collect::<Vec<_>>()
    );
    let bound = tsr_binder::bind_into(
        tsr_binder::BindResult::empty(),
        &arena,
        lib_file.source_file,
        &nodes,
        tsr_binder::FileInfo { name: "lib.d.ts", text: lib },
    );
    let bound = tsr_binder::bind_into(
        bound,
        &arena,
        parsed.source_file,
        &nodes,
        tsr_binder::FileInfo { name: "test.ts", text: source },
    );
    let root = tsr_ast::Node::SourceFile(parsed.source_file).node_id().expect("registered");
    let symbol = bound.lookup_local(root, name).unwrap_or_else(|| panic!("`{name}` is declared"));
    let mut checker = Checker::new(&bound, &nodes, &node_map);
    let id = checker.get_type_of_symbol(symbol);
    checker.type_to_string(id)
}

#[test]
fn an_async_declaration_with_no_valued_return_is_promise_void() {
    // `getReturnTypeFromBody`'s zero-aggregate async arm (`checker.go:20175`
    // -> `:20184`): `compiler/asyncFunctionWithForStatementNoInitializer.types`
    // records `>useFor : () => Promise<void>` for exactly this shape.
    assert_eq!(
        type_of_declaration_with_promise("async function f() {}", "f"),
        "() => Promise<void>"
    );
    // A bare `return;` is still the empty aggregate (`checker.go:20266`).
    assert_eq!(
        type_of_declaration_with_promise("async function f() { return; }", "f"),
        "() => Promise<void>"
    );
    // A valued PRIMITIVE return is its own awaited type (`checker.go:20149`
    // is an identity on a type that cannot carry a `then` member) —
    // `compiler/awaitInClassInAsyncFunction.types` records
    // `>foo : () => Promise<number>` for this shape.
    assert_eq!(
        type_of_declaration_with_promise("async function f() { return 1; }", "f"),
        "() => Promise<number>"
    );
    // A returned OBJECT with no `then` member is its own awaited type —
    // `getAwaitedTypeNoAlias`'s tail (`checker.go:31417`). Before the §443
    // widening this was the gap sentinel; the `then`-carrying shape still is,
    // because the promised-type signature walk is unported.
    assert_eq!(
        type_of_declaration_with_promise("async function f() { return { a: 1 }; }", "f"),
        "() => Promise<{ a: number; }>"
    );
}

/// The printed type of `name`, bound beside a stand-in lib declaring `Generator`.
fn generator_declaration_type(source: &str, name: &str, strict_null_checks: bool) -> String {
    let lib = "interface Generator<T, TReturn, TNext> {}\n";
    let arena = Arena::new();
    let mut nodes = tsr_ast::NodeTable::new();
    let mut node_map = tsr_ast::NodeMap::new();
    let options = tsr_parser::ParseOptions::default();
    let lib_file = tsr_parser::parse_into(&arena, lib, options, &mut nodes, &mut node_map);
    let parsed = tsr_parser::parse_into(&arena, source, options, &mut nodes, &mut node_map);
    assert!(
        parsed.diagnostics.is_empty(),
        "fixture must parse: {:?}",
        parsed.diagnostics.iter().map(tsr_diagnostics::Diagnostic::text).collect::<Vec<_>>()
    );
    let bound = tsr_binder::bind_into(
        tsr_binder::BindResult::empty(),
        &arena,
        lib_file.source_file,
        &nodes,
        tsr_binder::FileInfo { name: "lib.d.ts", text: lib },
    );
    let bound = tsr_binder::bind_into(
        bound,
        &arena,
        parsed.source_file,
        &nodes,
        tsr_binder::FileInfo { name: "test.ts", text: source },
    );
    let root = tsr_ast::Node::SourceFile(parsed.source_file).node_id().expect("registered");
    let symbol = bound.lookup_local(root, name).unwrap_or_else(|| panic!("`{name}` is declared"));
    let mut checker = Checker::new(&bound, &nodes, &node_map);
    checker.set_strict_null_checks(strict_null_checks);
    let id = checker.get_type_of_symbol(symbol);
    checker.type_to_string(id)
}

/// The strict default, which is [`Checker::new`]'s own.
fn type_of_declaration_with_generator(source: &str, name: &str) -> String {
    generator_declaration_type(source, name, true)
}

#[test]
fn a_generator_declaration_infers_generator_of_its_yields() {
    // `getReturnTypeFromBody`'s generator arm (`checker.go:20151`): yield
    // aggregate, `void` return fallback, `unknown` next for a declaration
    // (`:20242`). `conformance/generatorReturnTypeInference*` baselines record
    // exactly these shapes.
    assert_eq!(
        type_of_declaration_with_generator("function* g() { yield 1; }", "g"),
        "() => Generator<number, void, unknown>"
    );
    // No yield at all: the aggregate is empty and the slot is `never`
    // (`checker.go:20239`).
    assert_eq!(
        type_of_declaration_with_generator("function* g() {}", "g"),
        "() => Generator<never, void, unknown>"
    );
    // `yield*` needs the iteration protocol and declines whole.
    assert_eq!(type_of_declaration_with_generator("function* g() { yield* [1]; }", "g"), "error");
    // A valued return — **flipped by §135 slice 1** (the thirty-fifth
    // stand-in): the R slot takes the return aggregate through the same
    // widening the yield slot uses; `return 2` widens to `number`.
    assert_eq!(
        type_of_declaration_with_generator("function* g() { yield 1; return 2; }", "g"),
        "() => Generator<number, number, unknown>"
    );
}

#[test]
fn await_unwraps_the_global_promise_and_passes_primitives_through() {
    // `checkAwaitExpression` (`checker.go:10845`) through §18's slice:
    // `conformance/await_unaryExpression_es6_3.types` records primitives
    // passing through, and any `.types` baseline with `await p` on a
    // `Promise<T>` records `T`.
    assert_eq!(
        type_of_declaration_with_promise(
            "declare var p: Promise<number>;\nasync function f() { const x = await p; return; }",
            "f"
        ),
        "() => Promise<void>"
    );
    // `await 1` is `1` — a primitive is its own awaited type, freshness kept.
    assert_eq!(
        type_of_declaration_with_promise("async function f() { return await 1; }", "f"),
        "() => Promise<number>"
    );
}

/// §220. A bare `yield;` under **no-strict** contributes `any`.
///
/// §135 built the strict half and deferred this one in as many words — "under
/// no-strict the contribution is `any`; decline there rather than model it this
/// slice" — so the whole signature answered `error` for
/// `function* foo() { yield; yield; }`, which is
/// `conformance/YieldExpression3_es6` verbatim. Five cases.
///
/// `any` is not an assertion about no-strict, it is the widening rule: upstream
/// has two undefined types and a bare yield under no-strict contributes
/// `undefinedWideningType`, which `getWidenedType` (`checker.go:20224`) maps to
/// `any` — the same rule that makes `var x;` an `any`.
#[test]
fn a_bare_yield_under_no_strict_contributes_any() {
    assert_eq!(
        generator_declaration_type("function* g() { yield; yield; }", "g", false),
        "() => Generator<any, void, unknown>"
    );
}

/// The two controls that make the arm's shape observable, because without them
/// "contribute `any` unconditionally" and "contribute `undefined`
/// unconditionally" each pass the test above or its strict sibling.
#[test]
fn the_bare_yield_contribution_is_the_only_thing_strictness_changes() {
    // Strict keeps `undefined` — §135's measured half, unchanged by §220.
    assert_eq!(
        generator_declaration_type("function* g() { yield; }", "g", true),
        "() => Generator<undefined, void, unknown>"
    );
    // A VALUED yield is unaffected either way: the contribution rule applies to
    // the bare form alone, so no-strict must not smear `any` over a real
    // operand type.
    assert_eq!(
        generator_declaration_type("function* g() { yield 1; }", "g", false),
        "() => Generator<number, void, unknown>"
    );
    // And mixed, which is where the two contributions actually meet. §220's
    // draft of this line asserted `any` on the DERIVATION that the bare
    // yield's `any` absorbs the union. The real mechanics are upstream's
    // two-stage pipeline (§357): the contribution is `undefinedWideningType`
    // in BOTH modes; under no-strict `addTypesToUnion` (checker.go:25783)
    // DROPS the nullable from the aggregate, leaving `1`, and
    // `getWidenedType` widens the survivor to `number`. What the old
    // derivation got right: the single-operand cases above (`any` under
    // no-strict, `undefined` under strict — the empty-set exit at
    // checker.go:25698 re-supplies `undefinedWideningType`). What replaces
    // it: `generatorTypeCheck22` (strict-by-default) keeps `undefined`
    // beside class operands, so the contribution cannot be `any` — only the
    // union's non-strict drop produces the absorbing behaviour, and it
    // produces `number` here, not `any`.
    assert_eq!(
        generator_declaration_type("function* g() { yield 1; yield; }", "g", false),
        "() => Generator<number, void, unknown>"
    );
}

/// §223. The no-contextual-type gate is `getContextualType`'s complement, not
/// a list of two positions.
///
/// A yield contributes to the NEXT slot only from a position that HAS a
/// contextual type, so a yield with none is answerable without modelling
/// contextual typing. §135 admitted `ExpressionStatement` and
/// `ComputedPropertyName` on exactly that reasoning and refused the rest —
/// true about both, but a list is not a rule. `getContextualType`
/// (`checker.go:29354`) switches on the PARENT's kind and every kind absent
/// from it provably has no contextual type; upstream has no
/// `ExpressionWithTypeArguments` arm, so a heritage-clause expression is one
/// of them. `conformance/generatorTypeCheck40`.
#[test]
fn a_yield_in_a_heritage_clause_has_no_contextual_type() {
    assert_eq!(
        generator_declaration_type("function* g() { class C extends (yield 0) { } }", "g", false),
        "() => Generator<number, void, unknown>"
    );
    // The bare form, which is §220's contribution reached through §223's gate —
    // the two arms compose and `generatorTypeCheck55`/`60` need both.
    assert_eq!(
        generator_declaration_type("function* g() { class C extends (yield) { } }", "g", false),
        "() => Generator<any, void, unknown>"
    );
}

/// The three controls, one per way the predicate could be wrong.
#[test]
fn the_intra_expression_road_cannot_reenter_a_return_inference() {
    // §469's declaration park (`contextual_return_in_flight`). The shape is
    // `intraExpressionInferences`' first repro distilled: a literal-returning
    // member of an object literal that is the argument of a generic call.
    // `produce`'s return inference asks its contextual signature; the
    // call-argument road consults the pass-1 memo, whose parameter type
    // carries the argument literal's OWN members; reading `produce` back out
    // of it re-builds this very signature. Upstream cannot loop here because
    // `signatureLinks` caches per DECLARATION as well as per call; this port
    // rebuilds signatures per query, so without the park this recursed
    // unboundedly — one declaration at every depth to the probe's cap,
    // overflowing an 8 MiB worker.
    //
    // **Measured vacuity, recorded rather than papered over**: with the park
    // deleted this fixture stays GREEN — the cycle needs lib machinery the
    // unit harness does not load, and the same mutation makes the corpus
    // case overflow (verified both ways, 2026-08-13). So the load-bearing
    // gate for the park is `conformance/intraExpressionInferences` aborting
    // the entire conformance run, which no one can miss; this test only PINS
    // the distilled shape's answer so a change in the road is visible here.
    assert_eq!(
        type_of_declaration(
            "declare function callIt<T>(obj: { produce: (n: number) => T, consume: (x: T) => void }): T;\n\
             const r = callIt({ produce: () => 0, consume: n => n });",
            "r",
        ),
        "number"
    );
}

#[test]
fn the_contextual_gate_declines_where_upstream_has_an_arm_that_answers() {
    // §583 CORRECTED THIS ASSERTION, which read `"error"` and was named
    // "...where upstream has an ARM". Having an arm was never the test:
    // `checkAndAggregateYieldOperandTypes` (`checker.go:20334`) appends the
    // NEXT slot only when `getContextualType` returns NON-NIL, and an empty
    // `nextTypes` becomes `unknownType` (`:20242`). `KindVariableDeclaration`
    // routes to `getContextualTypeForInitializerExpression` (`:29423`), which
    // answers nil for an UNANNOTATED declaration.
    //
    // Upstream's own baseline settles it rather than this comment:
    // `generatorImplicitAny` records `const value = yield;` as
    // `() => Generator<undefined, void, unknown>` (g2) and
    // `const value: string = yield;` as
    // `() => Generator<undefined, void, string>` (g3). Unannotated is
    // `unknown`; the ANNOTATION is what makes the position contextual.
    assert_eq!(
        generator_declaration_type("function* g() { var v = yield 1; }", "g", false),
        "() => Generator<number, void, unknown>"
    );
    // **§635 made this computable and the expectation flips.** What stood here
    // read: *"an ANNOTATED declaration really is a contextual position,
    // upstream reads the NEXT slot from it, and this port does not compute
    // that — so it must still decline whole rather than print `unknown`
    // there."* The premise was true when written and is no longer: §635 reads
    // the annotation into the NEXT slot, so `var v: string = yield 1` gives
    // `yield 1` → `number`, the annotation → `string`, and no returns → `void`.
    //
    // This is `generatorImplicitAny`'s g3 shape, which the corpus records as
    // `() => Generator<undefined, void, string>` for a BARE yield — the same
    // rule with a different operand.
    assert_eq!(
        generator_declaration_type("function* g() { var v: string = yield 1; }", "g", false),
        "() => Generator<number, void, string>"
    );
    // FLIPPED at §353. `KindYieldExpression` (`:29360`) IS in the switch —
    // §223's derivation was right about that — but its arm resolves through
    // the OUTER yield's contextual iteration type, and in the
    // declaration-only arm this loop guards, that chain provably dead-ends:
    // upstream's own baseline records `Generator<any, void, unknown>` for
    // `yield yield 0` (`generatorTypeCheck36/50`, both landed). "In the
    // switch" was the right test for kinds whose arm reads a DECLARATION
    // (the `var v = yield 1` control above stands); a kind whose arm reads
    // another CONTEXTUAL CHAIN needs the chain's own resolvability asked.
    assert_eq!(
        generator_declaration_type("function* g() { yield yield 0; }", "g", false),
        "() => Generator<any, void, unknown>"
    );
    // The parenthesis walk (`:29392` delegates to its own parent). **This
    // assertion is the one that catches it, and the obvious one does not.**
    // `(yield 1);` in statement position passes with the walk deleted, because
    // `ParenthesizedExpression` is not in the decline list either way — so the
    // outcome is the same for the wrong reason. Only a paren whose own parent
    // IS contextual can tell them apart. Deleting
    // `SyntaxKind::ParenthesizedExpression` from the walk reddens exactly this
    // line and nothing else.
    //
    // §583 had to ANNOTATE this fixture to keep it discriminating: it read
    // `var v = (yield 1)`, and an unannotated declaration is no longer a
    // contextual position, so both the walk and its deletion now answer the
    // same `Generator` type and the control had gone silent. The annotation is
    // what puts a contextual parent back on the far side of the parenthesis.
    // A control that stops discriminating is worse than no control, because it
    // still reads green.
    //
    // **§635 changed the answer and the control got STRONGER.** It read
    // `"error"`; now that the annotated next slot is computed, the walk's
    // presence is visible in the SLOT VALUE rather than in error-vs-not:
    // with the walk, `child` is the parenthesis, it matches the declaration's
    // initialiser, and the annotation lands `string` in the next slot; without
    // it, `child` is the yield, its parent is a `ParenthesizedExpression` —
    // which is not in the decline list at all — so the position reads
    // non-contextual and the slot falls back to `unknown`.
    //
    // `string` vs `unknown` in the third argument is a sharper discriminator
    // than `error` vs anything, because it also proves WHICH parent was found.
    assert_eq!(
        generator_declaration_type("function* g() { var v: string = (yield 1); }", "g", false),
        "() => Generator<number, void, string>"
    );
    // Kept beside it as the case that does NOT discriminate, so the next
    // reader does not mistake it for the control.
    assert_eq!(
        generator_declaration_type("function* g() { (yield 1); }", "g", false),
        "() => Generator<number, void, unknown>"
    );
}

/// §743: `functionHasImplicitReturn` reads the flow graph, so a body whose
/// end is dead only through an EXHAUSTIVE switch gets no `| undefined`.
#[test]
fn an_exhaustive_switch_leaves_no_implicit_return() {
    // The literal arm. (The enum-member spelling of the same shape —
    // `stringEnumLiteralTypes1` f10 — is exercised by the corpus; this
    // harness has no lib and types `Choice.Yes` as a case expression to
    // `error`, which declines the clause-type list before the predicate.)
    assert_eq!(
        type_of_declaration(
            "function f10(x: \"yes\" | \"no\") { switch (x) { case \"yes\": return \"true\"; case \"no\": return \"false\"; } }",
            "f10"
        ),
        "(x: \"yes\" | \"no\") => \"false\" | \"true\""
    );
    assert_eq!(
        type_of_declaration(
            "function f(x: string | number | boolean) { switch (typeof x) { case 'string': return 1; case 'number': return 2; case 'boolean': return 3; } }",
            "f"
        ),
        "(x: string | number | boolean) => 1 | 2 | 3"
    );
    // A call after the last `return` does not kill the end (§741's residue).
    assert_eq!(
        type_of_declaration(
            "declare function log(s: string): void;\nfunction f(c: boolean) { if (c) { return 1; } log(\"x\"); }",
            "f"
        ),
        "(c: boolean) => 1 | undefined"
    );
}
