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
    assert_eq!(
        type_of_declaration(
            "function f(s: string, n: number, c: boolean) { if (c) { return s; } return n; }",
            "f"
        ),
        "error"
    );

    // Two numeric *literals* are two distinct types before any widening, and the
    // aggregate is taken on the unwidened types exactly as upstream's
    // `AppendIfUnique` is. Upstream reduces and then widens to `number`; this
    // port stops at the union it cannot build.
    assert_eq!(
        type_of_declaration("function f(c: boolean) { if (c) { return 1; } return 2; }", "f"),
        "error"
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
        lib_file.source_file,
        &nodes,
        tsr_binder::FileInfo { name: "lib.d.ts", text: lib },
    );
    let bound = tsr_binder::bind_into(
        bound,
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
    // A VALUED return needs `getAwaitedType` and stays a gap — `error` is the
    // gap sentinel, not an answer.
    assert_eq!(type_of_declaration_with_promise("async function f() { return 1; }", "f"), "error");
}

/// The printed type of `name`, bound beside a stand-in lib declaring `Generator`.
fn type_of_declaration_with_generator(source: &str, name: &str) -> String {
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
        lib_file.source_file,
        &nodes,
        tsr_binder::FileInfo { name: "lib.d.ts", text: lib },
    );
    let bound = tsr_binder::bind_into(
        bound,
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
    // A valued return needs the subtype-reduced aggregate and declines whole.
    assert_eq!(
        type_of_declaration_with_generator("function* g() { yield 1; return 2; }", "g"),
        "error"
    );
}
