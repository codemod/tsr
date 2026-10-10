//! What a tagged template must get right.
//!
//! `checkTaggedTemplateExpression` (`checker.go:10034`) resolves the tag's
//! signature through the **same** `getResolvedSignature` a call expression uses
//! and returns its return type — a tagged template is a call whose arguments are
//! the template strings array and the substitutions. So the port shares
//! `resolve_call_signature` and inherits its restrictions, which is what these
//! tests pin.
//!
//! `>tag`\x` : string` and friends are the baseline shape, under
//! `vendor/typescript-go/testdata/baselines/reference/submodule`.

use tsr_ast::Statement;
use tsr_checker::Checker;
use tsr_core::Arena;

/// Type the initialiser of the last statement.
fn type_of_last(source: &str) -> String {
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
fn a_tagged_template_is_the_tags_return_type() {
    // `>tag`\x` : string`. The answer comes from the TAG's signature, not from
    // the template — which is the whole point of the construct and the thing a
    // port could plausibly get wrong by answering `string` for the literal.
    assert_eq!(
        type_of_last("function tag(s: any): string { return \"\"; }\nconst x = tag`hi`;"),
        "string"
    );
    assert_eq!(
        type_of_last("function tag(s: any): number { return 1; }\nconst x = tag`hi`;"),
        "number"
    );
}

#[test]
fn the_answer_does_not_come_from_the_template() {
    // A tag returning something unrelated to a string proves the template's own
    // type is not what is being reported.
    assert_eq!(
        type_of_last("class C {}\nfunction tag(s: any): C { return new C(); }\nconst x = tag`hi`;"),
        "C"
    );
}

#[test]
fn a_template_with_substitutions_still_takes_the_tags_return_type() {
    // The substitution makes the template a `TemplateExpression`, which is
    // unported and answers a gap on its own line — but the tagged template does
    // not inherit that, because the tag's signature is what decides.
    assert_eq!(
        type_of_last(
            "var n: number;\nfunction tag(s: any): string { return \"\"; }\nconst x = tag`a${n}b`;"
        ),
        "string"
    );
}

#[test]
fn consuming_a_callback_context_keeps_its_inference_fixed_and_widened() {
    let source = "declare function tag<T>(strings: any, callback: (input: T) => T, value: T): T;
        const result = tag`${item => item} ${10}`;";
    assert_eq!(type_of_last(source), "number");
}

#[test]
fn a_generic_tag_is_a_gap() {
    // A generic signature's return type depends on inference over the template
    // strings array and each substitution. `String.raw` and every typed template
    // helper is generic, which is why this form's gap lines do not all close.
    assert_eq!(
        type_of_last("function tag<T>(s: any): T { return null as any; }\nconst x = tag`hi`;"),
        "error"
    );
}

/// §914: **no longer a gap.** Written type arguments instantiate the return,
/// exactly as a call's do. This road used to reject the whole expression the
/// moment it saw any — `!node.type_arguments.is_empty() → error` — before it
/// resolved anything, so the generic return above it never got a chance.
///
/// `a_generic_tag_is_a_gap` above still holds and is the live half: a generic
/// tag with NO written arguments needs inference over the strings array and each
/// substitution, which this road does not build.
#[test]
fn explicit_type_arguments_instantiate_the_return() {
    assert_eq!(
        type_of_last(
            "function tag<T>(s: any): T { return null as any; }\nconst x = tag<string>`hi`;"
        ),
        "string"
    );
}

#[test]
fn a_tag_that_is_not_callable_is_a_gap() {
    // Zero call signatures: `resolve_call_signature` refuses, the same as for a
    // plain call. Upstream reports "this expression is not callable".
    assert_eq!(type_of_last("class C {}\nconst x = C`hi`;"), "error");
    assert_eq!(type_of_last("var s: string;\nconst x = s`hi`;"), "error");
}

// The optional-chain guard (``tag?.`x` ``) is deliberately NOT tested. The form
// is prohibited by the grammar — TypeScript reports "Tagged template expressions
// are not permitted in an optional chain" and this port's parser rejects it with
// "Identifier expected" — so no fixture can reach the guard. It is kept because
// it mirrors `check_call_expression`'s shape and becomes live if the parser ever
// produces the node, and because a guard that costs one `is_some()` is cheaper
// than the wrong answer it would prevent.

/// §198. A span this port could not type does not make the template a gap.
///
/// `checkTemplateExpression` (`checker.go:7976`) checks each span for one thing
/// — an ESSymbol-like type, which it *reports* on — and then returns
/// `stringType` unless the node is in a const or template-literal context. The
/// span types are not an input to the answer outside those contexts, so
/// propagating a span's `error` was a refusal justified by "the answer depends
/// on something unknown" where upstream's answer depends on nothing of the
/// kind. `conformance/destructuringParameterProperties4` and twenty-one other
/// cases record `` `${x}` : string `` over substitutions this port cannot type.
///
/// **The first assertion is the anti-vacuity guard**, and it is the point of
/// writing the test this way: the arm can only be exercised by a span that
/// genuinely gaps, and any particular gap may close. `import.meta` was the
/// span until it typed (tsr-2zk.990) and this line failed loudly, as designed;
/// `import.foo` was next, until it answered upstream's own `errorType`
/// (`checkImportMetaProperty`, `checker.go:10782`; r6-errorsplit2 §8). A
/// missed property on a declared reference receiver keeps the gap
/// (r6-errorsplit2 §3), so it is the span now.
#[test]
fn a_template_over_an_untypeable_span_is_still_string() {
    assert_eq!(
        type_of_last("declare const o: { a: 1 };\nvar a = o.b;"),
        "error",
        "this fixture needs a span that GAPS; pick another construct"
    );
    assert_eq!(type_of_last("declare const o: { a: 1 };\nvar s = `x${o.b}y`;"), "string");
}

/// The control that keeps the fold alive: a template whose spans DO type still
/// folds to a string literal, which is §101/§147's behaviour and must not have
/// been traded away for the line above.
#[test]
fn a_template_over_literal_spans_still_folds() {
    assert_eq!(type_of_last("const a = `x${1}y`;"), "\"x1y\"");
}

/// §916: inference from a tagged template's SUBSTITUTIONS.
///
/// Upstream's argument list is `[TemplateStringsArray, ...spans]`, so
/// substitution *i* pairs with parameter *i + 1*. Every index pairing in
/// `check_generic_call` is `parameters[i] ↔ arguments[i]`; threading an offset
/// through all of them is one way to get the `+1`, and **dropping the first
/// parameter from the signature is the same thing while touching nothing** — the
/// remaining parameters line up with the substitutions by construction, and the
/// return type and type parameters are untouched.
///
/// Corpus effect: 64 `WRONG->RIGHT`, 13 `GAP->RIGHT` against 3 `GAP->WRONG`, zero
/// `RIGHT->WRONG`. `taggedTemplateStringsTypeArgumentInference` 29 and its ES6
/// twin 29.
#[test]
fn a_generic_tag_infers_from_its_substitutions() {
    assert_eq!(
        type_of_last(
            "interface TemplateStringsArray { readonly raw: readonly string[]; }\n\
             declare function tag<T>(s: TemplateStringsArray, x: T): T;\n\
             const x = tag`a${42}b`;"
        ),
        // **`42`, not `number`** — my first expectation here was wrong.
        // `getCovariantInference` widens a literal candidate only when
        // `!primitiveConstraint && topLevel && (isFixed || !isTypeParameterAtTopLevelInReturnType)`.
        // `T` IS at top level in the return `T` and is not fixed, so
        // `widenLiteralTypes` is false and the literal survives — the same rule
        // that makes `f(42)` on `f<T>(x: T): T` record `42`.
        "42"
    );
}

/// Two substitutions pair with the second and third parameters.
#[test]
fn two_substitutions_pair_with_the_parameters_after_the_strings_array() {
    assert_eq!(
        type_of_last(
            "interface TemplateStringsArray { readonly raw: readonly string[]; }\n\
             declare function tag<T, U>(s: TemplateStringsArray, x: T, y: U): U;\n\
             const x = tag`a${42}b${\"s\"}c`;"
        ),
        // `"s"` for the same reason as above: `U` is the return type.
        "\"s\""
    );
}

/// A tag with NO substitutions has nothing to infer from and keeps its gap —
/// `a_generic_tag_is_a_gap` above states the same rule from the other side.
#[test]
fn a_generic_tag_with_no_substitutions_still_gaps() {
    assert_eq!(
        type_of_last(
            "interface TemplateStringsArray { readonly raw: readonly string[]; }\n\
             declare function tag<T>(s: TemplateStringsArray, x: T): T;\n\
             const x = tag`ab`;"
        ),
        "error"
    );
}

/// §917: two or more arity survivors go to `chooseOverload`'s argument pass,
/// which §916's shift makes reachable — the candidates are shifted past the
/// strings-array parameter so their remaining parameters pair with the
/// substitutions by index, exactly as a call's do. The pick is mapped back to
/// the UNSHIFTED candidate by declaration.
///
/// Corpus effect: 34 `WRONG->RIGHT`, 4 `GAP->RIGHT` against 5 `GAP->WRONG`, zero
/// `RIGHT->WRONG`.
#[test]
fn an_overloaded_tag_is_selected_by_its_substitution_types() {
    let prelude = "interface TemplateStringsArray { readonly raw: readonly string[]; }\n\
         declare function f(s: TemplateStringsArray, a: number): string;\n\
         declare function f(s: TemplateStringsArray, a: string): boolean;\n";
    assert_eq!(type_of_last(&format!("{prelude}const x = f`v${{1}}w`;")), "string");
    assert_eq!(type_of_last(&format!("{prelude}const x = f`v${{\"s\"}}w`;")), "boolean");
}
