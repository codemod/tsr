//! §840: `typeof a.b` in a type query reads the **narrowed** type of `a.b`.
//!
//! `isMatchingReference` (`vendor/typescript-go/internal/checker/flow.go:1639`)
//! has a `KindQualifiedName` arm that equates a qualified name with the
//! property-access spelling of the same dotted path. The two spellings are the
//! same reference written in the two grammars: `typeof properties.foo` parses
//! its path as a `QualifiedName` because it sits in a type position, while the
//! guard `if (properties.foo)` above it is a `PropertyAccessExpression`.
//!
//! The binder half was already ported — `record_flow` records a flow node for a
//! `QualifiedName` gated on `is_part_of_type_query`, exactly as
//! `binder.go:605-608` gates it — so the walk reached the guard and declined to
//! apply it for want of the matching arm alone.
//!
//! Corpus effect: `+16, zero adverse` (`narrowingOfQualifiedNames` 14,
//! `controlFlowIfStatement` 2).

use tsr_ast::Statement;
use tsr_checker::Checker;
use tsr_core::Arena;

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
                if let Some(inner) = last_expression_statement(block.statements) {
                    found = Some(inner);
                }
            }
            Statement::IfStatement(node) => {
                for branch in [node.then_statement, node.else_statement].into_iter().flatten() {
                    if let Some(inner) = last_expression_statement(std::slice::from_ref(&branch)) {
                        found = Some(inner);
                    }
                }
            }
            _ => {}
        }
    }
    found
}

/// Inside the guard, `typeof properties.foo` is the narrowed type — the
/// `undefined` the declaration allows is gone. This is the assertion §840 turns
/// on, and it was `{ aaa: string; } | undefined` before the matching arm existed.
#[test]
fn a_type_query_over_a_qualified_name_narrows() {
    assert_eq!(
        type_of_last_expression(
            "declare const properties: { foo?: { aaa: string } };
             if (properties.foo) { declare const v: typeof properties.foo; v; }"
        ),
        "{ aaa: string; }"
    );
}

/// Outside the guard it does not, which is what says the arm narrows rather
/// than unconditionally stripping `undefined`.
#[test]
fn a_type_query_over_a_qualified_name_is_unnarrowed_outside_the_guard() {
    assert_eq!(
        type_of_last_expression(
            "declare const properties: { foo?: { aaa: string } };
             declare const v: typeof properties.foo;
             v;"
        ),
        "{ aaa: string; } | undefined"
    );
}

/// The name must match: a guard on a *different* property narrows nothing.
#[test]
fn a_guard_on_a_different_property_does_not_narrow() {
    assert_eq!(
        type_of_last_expression(
            "declare const properties: { foo?: { aaa: string }, bar?: string };
             if (properties.bar) { declare const v: typeof properties.foo; v; }"
        ),
        "{ aaa: string; } | undefined"
    );
}

/// §843: an ALIAS narrows. `checkIdentifier`
/// (`vendor/typescript-go/internal/checker/checker.go:11104-11118`) has three
/// outcomes — `VARIABLE` narrows, `isAlias` narrows, everything else returns the
/// declared type — and this port's gate tested `VARIABLE` alone, putting every
/// alias into the third.
///
/// The corpus witness is `narrowedImports` (`if (a0) x = a0` over an imported
/// binding), which this harness cannot express for want of a `ModuleHost`; an
/// `import a = M.x` alias reaches the same gate without needing one.
/// Corpus effect: `+10, zero adverse`, and the case went `10 WRONG -> 0`.
#[test]
fn an_alias_narrows() {
    assert_eq!(
        type_of_last_expression(
            "namespace M { export declare let x: number | undefined; }
             import a = M.x;
             if (a) { a; }"
        ),
        "number"
    );
}
