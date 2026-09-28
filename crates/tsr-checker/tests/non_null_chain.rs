//! §884: `checkNonNullAssertion` (`checker.go:10622`) is two branches, and which
//! one a `!` takes is decided by the **parser**, not by the operand.
//!
//! ```go
//! if node.Flags&ast.NodeFlagsOptionalChain != 0 {
//!     return c.checkNonNullChain(node)
//! }
//! return c.GetNonNullableType(c.checkExpression(node.Expression()))
//! ```
//!
//! `checkNonNullChain` (`checker.go:10631`) strips the chain's marker, takes the
//! non-nullable remainder, and re-unions `undefined` through
//! `propagateOptionalTypeMarker`. The plain branch does not.
//!
//! The flag is *not* "the operand spine contains `?.`". Every `!` is built with
//! `ast.NodeFlagsNone` (`parser.go:5375`); the flag is added retroactively by
//! `tryReparseOptionalChain` (`parser.go:5414`), reachable only from the
//! property-access, element-access and call rests. So a `!` is a chain link
//! exactly when another link was parsed **on top of** it, and the observable
//! consequence is that an inner `!` keeps the chain's `undefined` while the
//! trailing one removes it. Upstream's own baseline
//! (`conformance/elementAccessChain.types:163-166`) records both in one line:
//!
//! ```text
//! >o2?.["b"]!.c! : string
//! >o2?.["b"]!    : { c: string; } | undefined
//! ```
//!
//! Corpus effect when this landed: `WRONG->RIGHT 22`, `RIGHT->WRONG 0`.
//!
//! This harness has no `lib.d.ts` and none of `types_producer`'s position rules;
//! it pins the mechanism, and the corpus pinned the gain.

use tsr_checker::Checker;
use tsr_core::Arena;

/// Type the expression of the last expression statement under `strictNullChecks`.
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
    let options = tsr_core::CompilerOptions {
        strict_null_checks: tsr_core::Tristate::True,
        ..Default::default()
    };
    checker.apply_compiler_options(&options);
    let mut last = None;
    for statement in parsed.source_file.statements {
        if let tsr_ast::Statement::ExpressionStatement(node) = statement {
            last = node.expression;
        }
    }
    let id = checker.check_expression(last.expect("fixture ends with an expression statement"));
    checker.type_to_string(id)
}

const O2: &str = "declare const o2: undefined | { b: { c: string } };\n";

/// A `!` with another chain link on top of it: `checkNonNullChain`, and the
/// chain's `undefined` comes back.
#[test]
fn inner_non_null_in_a_chain_re_unions_undefined() {
    assert_eq!(type_of_last_expression(&format!("{O2}o2?.[\"b\"]!.c;")), "string | undefined");
}

/// The same `!`, read as its own expression rather than through the access
/// above it — still an inner link, because `.c` reparsed it.
#[test]
fn property_form_of_the_same_chain() {
    assert_eq!(type_of_last_expression(&format!("{O2}o2?.b!.c;")), "string | undefined");
}

/// The trailing `!`: nothing follows it, so no rest reparsed it, so it is not a
/// chain link and the plain branch removes `undefined` for good.
#[test]
fn trailing_non_null_removes_undefined() {
    assert_eq!(type_of_last_expression(&format!("{O2}o2?.[\"b\"]!.c!;")), "string");
}

/// `m?.[0]! && …`: a binary operator is not one of the three rests that can
/// reparse a chain, so this `!` is not a link either. The shape of
/// `compiler/narrowingWithNonNullExpression`, which the first build of §884
/// broke by deriving the flag from the operand alone.
#[test]
fn non_null_under_a_binary_operator_is_not_a_chain_link() {
    assert_eq!(
        type_of_last_expression("declare const m: undefined | { [k: number]: string };\nm?.[0]!;"),
        "string"
    );
}

/// A `!` on an operand with no `?.` anywhere is the plain branch, unchanged.
#[test]
fn non_null_on_a_plain_operand_is_untouched() {
    assert_eq!(type_of_last_expression("declare const x: string | undefined;\nx!;"), "string");
}

/// A parenthesis breaks the chain in the parser, so `(a?.b)!` is not a link —
/// the same rule `expression_is_optional_chain`'s spine walk already honours.
#[test]
fn a_parenthesis_breaks_the_chain() {
    assert_eq!(type_of_last_expression(&format!("{O2}(o2?.b)!.c;")), "string");
}
