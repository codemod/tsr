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
    // `undefinedType` under it and not otherwise). This port has no compiler
    // options, so it declines to pick a spelling.
    assert_eq!(
        type_of_declaration("function f(c: boolean) { if (c) { return 1; } return; }", "f"),
        "error"
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
    // A returned OBJECT could be a thenable and needs the awaited machinery —
    // `error` is the gap sentinel, not an answer.
    assert_eq!(
        type_of_declaration_with_promise("async function f() { return { a: 1 }; }", "f"),
        "error"
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
    // And mixed, which is where the two contributions actually meet: `any`
    // absorbs the union, so this reads `any` under no-strict and
    // `number | undefined` under strict. A single-contribution model cannot
    // produce both.
    assert_eq!(
        generator_declaration_type("function* g() { yield 1; yield; }", "g", false),
        "() => Generator<any, void, unknown>"
    );
}
