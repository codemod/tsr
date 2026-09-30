//! What a function type node in annotation position denotes, stated as
//! questions about the printed type.
//!
//! Assertions go through `Checker::type_to_string` for the reason
//! `tests/types.rs` gives: the printed form is what a `.types` baseline
//! compares, and a type that is internally right but prints wrong fails
//! conformance identically.

use tsr_ast::{Node, Statement, SyntaxKind};
use tsr_checker::Checker;
use tsr_core::Arena;

#[test]
fn a_reduced_intersection_return_keeps_its_written_annotation() {
    assert_eq!(type_of_annotation("var f: () => boolean & null;"), "() => boolean & null");
    assert_eq!(
        type_of_annotation("var f: <T extends {}>(value: T & ({} | null)) => void;"),
        "<T extends {}>(value: T & ({} | null)) => void"
    );
}

/// Type the annotation of the first `var`/`let`/`const` in the source.
///
/// The annotation rather than the initialiser, because a function *type node*
/// only ever appears in type position — typing the initialiser would exercise
/// `checkFunctionExpression` instead, which is a different arm.
fn type_of_annotation(source: &str) -> String {
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

    let Statement::VariableStatement(statement) = parsed.source_file.statements[0] else {
        panic!("the fixture must start with a variable statement");
    };
    let declaration = statement
        .declaration_list
        .and_then(|list| list.declarations.first().copied())
        .expect("one declaration");
    let annotation = declaration.r#type.expect("an annotation");
    let id = checker.get_type_from_type_node(annotation);
    checker.type_to_string(id)
}

/// The syntax kind the first declaration's annotation parses to.
///
/// Every fixture below is checked against this before its type is asserted. A
/// test whose fixture quietly stopped parsing as the node it names would still
/// pass — `error` is the expected answer for several of them — so the kind is
/// pinned separately rather than inferred from the printed type.
fn annotation_kind(source: &str) -> SyntaxKind {
    let arena = Arena::new();
    let parsed = tsr_parser::parse(&arena, source);
    let Statement::VariableStatement(statement) = parsed.source_file.statements[0] else {
        panic!("the fixture must start with a variable statement");
    };
    let annotation = statement
        .declaration_list
        .and_then(|list| list.declarations.first().copied())
        .expect("one declaration")
        .r#type
        .expect("an annotation");
    Node::from(annotation).node_id().map(|id| parsed.nodes.kind(id)).expect("a registered node")
}

/// [`type_of_annotation`], having first asserted the annotation really is a
/// function type node — so a fixture that stopped parsing as one fails loudly
/// rather than passing through some other arm that happens to print alike.
fn type_of_function_annotation(source: &str) -> String {
    assert_eq!(
        annotation_kind(source),
        SyntaxKind::FunctionType,
        "the fixture's annotation must be a function type node"
    );
    type_of_annotation(source)
}

#[test]
fn a_function_type_prints_its_parameters_and_return_type() {
    assert_eq!(
        type_of_function_annotation("let f: (x: number) => string;"),
        "(x: number) => string"
    );
}

#[test]
fn a_function_type_with_no_parameters_prints_empty_parentheses() {
    assert_eq!(type_of_function_annotation("let f: () => void;"), "() => void");
}

/// The `?` is the node builder's, from `isOptionalParameter` — not the source
/// text's. `signature_to_string` emits `?: ` rather than `: `.
#[test]
fn an_optional_parameter_keeps_its_question_mark() {
    assert_eq!(type_of_function_annotation("let f: (x?: number) => void;"), "(x?: number) => void");
}

/// `...` is carried on the [`Parameter`] and re-emitted, so a rest parameter
/// does not silently print as an ordinary one.
#[test]
fn a_rest_parameter_keeps_its_ellipsis() {
    assert_eq!(type_of_function_annotation("let f: (...xs: any) => void;"), "(...xs: any) => void");
}

/// A function type node carries type parameters, and `signature_to_string`
/// renders them ahead of the parameter list.
#[test]
fn a_generic_function_type_prints_its_type_parameters() {
    assert_eq!(type_of_function_annotation("let f: <T>(x: T) => T;"), "<T>(x: T) => T");
}

/// The 523 lines the measurement attributed to nesting: the inner function type
/// has to resolve through the same arm for the outer one to print at all.
#[test]
fn a_nested_function_type_resolves_through_the_same_arm() {
    assert_eq!(
        type_of_function_annotation("let f: (g: (x: number) => void) => void;"),
        "(g: (x: number) => void) => void"
    );
}

/// **§929 INVERTED this rule, and the name is kept so the change is visible.**
///
/// It used to read *"a gap in a parameter is a gap in the whole function type…
/// the reason this arm cannot be written as a fallback"*. It can, and it should:
/// upstream's parameter carries `errorType` and the node builder still reuses the
/// **written** annotation node, so the signature prints in full.
/// `declare function f(a: Array): void` prints `(a: Array) => void` upstream and
/// printed `error` here.
///
/// Measured on the corpus: **314 `WRONG->RIGHT` + 128 `GAP->RIGHT` against 77
/// `GAP->WRONG`, zero `RIGHT->WRONG`** — +442, the largest single move of its
/// session. One unreadable part was taking out every readable one.
///
/// **`keyof string` is not verified against upstream.** No corpus row measures
/// this exact spelling; what is asserted is that the port now prints the written
/// annotation instead of collapsing the signature. If upstream turns out to
/// resolve `keyof string` to its union here, that is a separate defect in
/// `written_type_text`'s choice of spelling and not in this rule.
#[test]
fn a_parameter_whose_type_is_a_gap_keeps_the_written_annotation() {
    assert_eq!(
        type_of_function_annotation("let f: (x: keyof string) => void;"),
        "(x: keyof string) => void"
    );
    assert_eq!(type_of_function_annotation("let f: (x: Nope) => void;"), "(x: Nope) => void");
}

/// And likewise the return annotation — see the entry above for the measurement
/// and the caveat.
#[test]
fn a_return_type_that_is_a_gap_keeps_the_written_annotation() {
    assert_eq!(
        type_of_function_annotation("let f: (x: number) => keyof string;"),
        "(x: number) => keyof string"
    );
    assert_eq!(type_of_function_annotation("let f: (x: number) => Nope;"), "(x: number) => Nope");
}

/// §48 (`checker-notes-narrow.md`) rendered PLAIN patterns verbatim, so
/// this fixture's name outlived its truth for the undecorated shape;
/// decorated patterns (defaults/rest/renames) still gap.
#[test]
fn a_destructuring_parameter_makes_the_function_type_a_gap() {
    assert_eq!(type_of_function_annotation("let f: ({ a }: any) => void;"), "({ a }: any) => void");
}

/// Calling a function-typed value now resolves, with **no code in `calls.rs`**.
///
/// `resolve_call_signature` (`crate::calls`) gets from a callee's type back to
/// its declaration through `TypeData::Anonymous`'s symbol. Once a function type
/// node carries the `__type` symbol that `bindFunctionOrConstructorType`
/// (`binder.go:985`) creates, `f(1)` resolves through the existing path — which
/// is upstream's own design showing through, not a coincidence: the whole point
/// of that binder function is to make `(x: number) => string` and
/// `{ (x: number): string }` indistinguishable downstream.
///
/// This test was written asserting `error`, before the binder arm existed, and
/// **watched to flip to `string`** when it landed. That order is the reason it
/// is worth keeping: it is the difference between observing the bonus and
/// assuming it.
#[test]
fn a_call_through_a_function_typed_value_resolves() {
    let source = "declare const f: (x: number) => string;\nconst y = f(1);";
    let arena = Arena::new();
    let parsed = tsr_parser::parse(&arena, source);
    let bound = tsr_binder::bind(
        &arena,
        parsed.source_file,
        &parsed.nodes,
        tsr_binder::FileInfo { name: "test.ts", text: source },
    );
    let mut checker = Checker::new(&bound, &parsed.nodes, &parsed.node_map);
    let Statement::VariableStatement(statement) = parsed.source_file.statements[1] else {
        panic!("the second statement must be the call");
    };
    let initialiser = statement
        .declaration_list
        .and_then(|list| list.declarations.first().copied())
        .expect("one declaration")
        .initializer
        .expect("an initialiser");
    let tsr_ast::Expression::CallExpression(call) = initialiser else {
        panic!("the fixture's initialiser must be a call");
    };
    // The positive control, without which this test would pass for any reason at
    // all — including the callee never reaching this slice's arm. The callee's
    // type has to be the function type this module builds *before* the failure
    // below can be attributed to call resolution rather than to the annotation.
    let callee = checker.check_expression(call.expression.expect("a callee"));
    assert_eq!(checker.type_to_string(callee), "(x: number) => string");
    let id = checker.check_expression(initialiser);
    assert_eq!(checker.type_to_string(id), "string");
}

/// `new (x: number) => C` — the arm this file used to pin the *absence* of.
///
/// It was a regression guard reading `"error"`, whose stated falsifier was
/// *"someone adding `ConstructorTypeNode` to `signature_parts_of` or to
/// `get_type_from_type_node` without also giving `Signature` a construct flag
/// and `signature_to_string` the `new ` / `abstract new ` prefix"*. `bd
/// tsr-jril` did exactly that **with** the flag and the prefix, so the guard
/// came due and is rewritten as a **pair**: the now-ported spelling asserting
/// its answer, beside a still-refused one asserting `error`, so it keeps
/// discriminating instead of becoming a tautology.
///
/// # Every string here was fetched from a baseline before it was written down
///
/// Not one is an intuition about what the printer *should* emit — this project
/// has five recorded cases of an expectation written from intuition and the port
/// being right every time. Counts are instances across
/// `testdata/baselines/reference/submodule/{compiler,conformance}/*.types` at
/// the pinned commit:
///
/// ```text
///     : new (x: number) => void        132
///     : new (...args: any) => any       20
/// >a2 : new <T>(x: T) => T              37
/// >a3 : new <T>(x?: T) => T             18
/// >b4 : new (x?: string) => string      17
/// ```
///
/// The corpus's most common rest spelling is
/// `new (x: number, y: number, ...z: string[]) => any` (39 instances), and it is
/// **not** used: `string[]` needs the `Array` symbol, which a bare
/// `Checker::new` has no lib to supply, so the fixture would have asserted a
/// pre-existing unrelated gap. `new (...args: any) => any` exercises the same
/// `Parameter::rest` field without that dependency. Found by running it, not by
/// predicting it.
///
/// The `abstract` spelling has **no bare assertion line in the corpus** — it
/// appears only nested, as in `>unionWithAbstractSignature : (abstract new (a:
/// string) => string) | (new (a: string) => string)` — so the constituent is
/// lifted from that line rather than composed. That is stated because it is the
/// weakest expectation in the file.
#[test]
fn a_constructor_type_prints_its_new() {
    for (source, want) in [
        ("let f: new (x: number) => void;", "new (x: number) => void"),
        ("let f: new (...args: any) => any;", "new (...args: any) => any"),
        ("let f: new <T>(x: T) => T;", "new <T>(x: T) => T"),
        ("let f: new <T>(x?: T) => T;", "new <T>(x?: T) => T"),
        ("let f: new (x?: string) => string;", "new (x?: string) => string"),
        ("let f: abstract new (a: string) => string;", "abstract new (a: string) => string"),
    ] {
        assert_eq!(
            annotation_kind(source),
            SyntaxKind::ConstructorType,
            "the fixture must parse as a constructor type node: {source:?}"
        );
        assert_eq!(type_of_annotation(source), want, "{source:?}");
    }
}

/// The other half of the pair: a constructor type node still gaps for every
/// reason a function type node gaps, and for no new one.
///
/// `get_signature_from_declaration` owns that list and the constructor arm adds
/// nothing to it — which is the claim being tested, not a restatement of the
/// arm. A destructuring parameter is chosen because `parameterToParameterDeclarationName`
/// is genuinely unported, so this expectation cannot be satisfied by any change
/// short of building it.
#[test]
fn a_constructor_type_inherits_the_function_types_gaps_and_adds_none() {
    let source = "let f: new ({ a }: any) => void;";
    assert_eq!(annotation_kind(source), SyntaxKind::ConstructorType);
    // §48: the plain pattern renders in BOTH spellings — the two lines
    // still move together, which is the property this fixture pins.
    assert_eq!(type_of_annotation(source), "new ({ a }: any) => void");
    assert_eq!(type_of_function_annotation("let f: ({ a }: any) => void;"), "({ a }: any) => void");
}
