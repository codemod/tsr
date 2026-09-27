//! A deferred `keyof X` / `X[Y]` mint is an OBJECT, and it narrows by
//! intersection. §812–§814.
//!
//! ```ts
//! function f<T, U>(k: keyof T | keyof U) { k; }   // answered `any`
//! function g<T, K extends keyof T>(x: T[K] | undefined) {
//!     if (x === undefined) return;
//!     x;                                          // answered `T[K]`
//! }
//! ```
//!
//! # §812 — the mint was ANY, so it absorbed its own union
//!
//! §34 and §35 mint a deferred `keyof T` / `T[K]` as a *print-only citizen*: the
//! written text, registered in `unresolved_types` so `is_error` stays true and
//! the diagnostic channel is untouched. Both minted it with `TypeFlags::ANY`.
//!
//! An `any` constituent **absorbs its whole union**, so `keyof T | keyof U` was
//! reduced to a bare `any` before anything could print it. §36's
//! template-literal mint had already met this and used `OBJECT`, recording the
//! reason at the site — *"a template mint in a UNION must not trip
//! any-absorption or string-literal reduction"*. The same one word, two
//! constructs over.
//!
//! # §813/§814 — the intersection road, and its ORDER
//!
//! `getAdjustedTypeWithFacts` (`checker.go:31159`) narrows a possibly-nullable
//! operand by INTERSECTION, and `removeNullableByIntersection` (`:31179`) gates
//! on the operand's FACTS and on nothing else — so a deferred `T[K]` is
//! intersected exactly as a type parameter `T` is. This port gated that road on
//! `TYPE_PARAMETER | UNKNOWN`, which is narrower than upstream (§813).
//!
//! Upstream also does the two things **in order**: `getTypeWithFacts` filters,
//! and its *result* is handed to `removeNullableByIntersection`. This port did
//! the intersection first, so it only ever saw an operand already free of its
//! nullable — which is why the FIRST narrowing of `T[K] | undefined` answered a
//! bare `T[K]` while a second identical guard answered `T[K] & ({} | null)`
//! (§814).
//!
//! The corpus asked for all of this by name: `keyofAndIndexedAccessErrors` (24
//! lines) and `indexedAccessAndNullableNarrowing` (12).

use tsr_ast::Statement;
use tsr_checker::Checker;
use tsr_core::Arena;

/// Type the expression of the **last** expression statement, descending into
/// function bodies so a fixture can introduce a type parameter.
fn type_of_last_expression(source: &str) -> String {
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
    // The intersection road is strict-only, exactly as upstream's is
    // (`getAdjustedTypeWithFacts`' `if c.strictNullChecks`), so the harness
    // states the flag rather than inheriting the optionless default.
    let options = tsr_core::CompilerOptions {
        strict_null_checks: tsr_core::Tristate::True,
        ..Default::default()
    };
    checker.apply_compiler_options(&options);

    let last = last_expression_statement(parsed.source_file.statements)
        .expect("the fixture must end with an expression statement");
    let id = checker.check_expression(last);
    checker.type_to_string(id)
}

fn last_expression_statement<'a>(statements: &[Statement<'a>]) -> Option<tsr_ast::Expression<'a>> {
    let mut found = None;
    for statement in statements {
        match statement {
            Statement::ExpressionStatement(node) => found = node.expression,
            Statement::Block(block) => {
                found = last_expression_statement(block.statements).or(found);
            }
            Statement::IfStatement(node) => {
                for branch in [node.then_statement, node.else_statement].into_iter().flatten() {
                    found = last_expression_statement(std::slice::from_ref(&branch)).or(found);
                }
            }
            Statement::FunctionDeclaration(node) => {
                if let Some(tsr_ast::FunctionBody::Block(block)) = node.body {
                    found = last_expression_statement(block.statements).or(found);
                }
            }
            _ => {}
        }
    }
    found
}

/// §812. `keyofAndIndexedAccessErrors` wants both of these verbatim; before the
/// flag changed, an `ANY` constituent collapsed each union to a bare `any`.
#[test]
fn a_keyof_mint_survives_its_union_and_its_intersection() {
    assert_eq!(
        type_of_last_expression("function f<T, U>(k: keyof T | keyof U) { k; }"),
        "keyof T | keyof U"
    );
    assert_eq!(
        type_of_last_expression("function f<T, U>(k: keyof T & keyof U) { k; }"),
        "keyof T & keyof U"
    );
}

/// §812, the indexed-access half: the second constituent used to be absorbed.
#[test]
fn an_indexed_access_mint_survives_a_nullable_union() {
    assert_eq!(
        type_of_last_expression(
            "function f<T, K extends keyof T>(x: T[K] | undefined) { x; }"
        ),
        "T[K] | undefined"
    );
}

/// §813 and §814 together. The FIRST guard is the one that used to answer a
/// bare `T[K]`, because the intersection ran before the filter and therefore
/// never saw the union.
#[test]
fn the_first_guard_on_a_deferred_access_answers_the_intersection() {
    assert_eq!(
        type_of_last_expression(
            "function f<T, K extends keyof T>(x: T[K] | undefined) {\n\
             if (x === undefined) return;\n\
             x;\n\
             }"
        ),
        "T[K] & ({} | null)"
    );
    assert_eq!(
        type_of_last_expression(
            "function f<T, K extends keyof T>(x: T[K] | null) {\n\
             if (x === null) return;\n\
             x;\n\
             }"
        ),
        "T[K] & ({} | undefined)"
    );
}

/// The DOUBLE-nullable composition, pinned as it behaves and **not** as a
/// claim about upstream.
///
/// `T[K] | null | undefined` under two guards of opposite kind answers
/// `T[K] & ({} | undefined)` here. §85's refinement lattice does not combine the
/// two into `T[K] & {}`, because the first guard's filtered result is the union
/// `T[K] | null` — not a mint — so §814's second attempt declines and no prior
/// kind is recorded for the second guard to refine.
///
/// **This assertion is a description, not an oracle.** The expectation written
/// first was `T[K] & {}`, from reading `removeNullableByIntersection`'s lattice,
/// and it was withdrawn on finding that **the string `X[Y] & {}` does not occur
/// in any `.types` baseline in the corpus** — so there is no evidence for either
/// answer and inventing one would be worse than recording the gap. Filed as
/// §814's residue. If a baseline for this shape ever appears, this test is the
/// thing to re-derive, and the fix is to carry the prior kind through a filtered
/// union rather than only through a bare mint.
#[test]
fn the_double_nullable_composition_is_unmeasured_residue() {
    assert_eq!(
        type_of_last_expression(
            "function f<T, K extends keyof T>(x: T[K] | null | undefined) {\n\
             if (x === undefined) return;\n\
             if (x === null) return;\n\
             x;\n\
             }"
        ),
        "T[K] & ({} | undefined)"
    );
}

/// The boundary §814 deliberately did NOT cross: the second attempt is
/// restricted to deferred mints, so a plain TYPE PARAMETER keeps §85's measured
/// behaviour. Widening it measured +2 against −2 for two regressed cases.
#[test]
fn a_plain_type_parameter_keeps_the_eighty_five_road() {
    assert_eq!(
        type_of_last_expression(
            "function f<T>(x: T | undefined) {\n\
             if (x === undefined) return;\n\
             x;\n\
             }"
        ),
        "T"
    );
}
