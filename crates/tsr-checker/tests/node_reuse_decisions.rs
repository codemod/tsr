//! The shared node-reuse decisions every printer asks
//! (`docs/parity/notes/r5-nodereuse.md`): the return slot
//! (`serializeReturnTypeForSignature`, `nodebuilderimpl.go:2023`, with the
//! pseudochecker's single-return Direct answer and
//! `pseudoReturnTypeMatchesPredicate`), the type-parameter constraint
//! (`typeParameterToDeclaration`, `:1611`), and the property slot
//! (`addPropertyToElementList` → `serializeTypeForDeclaration`, `:2486`).
//!
//! Each test asks the decision directly, the way a printer does, so it pins
//! the rule independently of which printer reaches it.

use tsr_ast::Statement;
use tsr_checker::{Checker, TypeId};
use tsr_core::Arena;

/// Run `ask` against the type of the final expression statement.
fn with_last_expression<R>(source: &str, ask: impl FnOnce(&mut Checker<'_, '_>, TypeId) -> R) -> R {
    let arena = Arena::new();
    let parsed = tsr_parser::parse(&arena, source);
    assert!(parsed.diagnostics.is_empty(), "fixture must parse");
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
    let last = parsed
        .source_file
        .statements
        .iter()
        .rev()
        .find_map(|statement| match statement {
            Statement::ExpressionStatement(node) => node.expression,
            _ => None,
        })
        .expect("the fixture must end with an expression statement");
    let id = checker.check_expression(last);
    ask(&mut checker, id)
}

/// The return-slot decision for the only signature of the last expression.
fn reused_return(source: &str) -> Option<String> {
    with_last_expression(source, |checker, id| {
        let signature = checker.signatures_of_type(id).expect("a function type")[0].clone();
        checker.reused_return_text(&signature, None)
    })
}

#[test]
fn a_single_returned_assertion_is_the_pseudo_return() {
    // `baseClassImprovedMismatchErrors`: the written `number | string`, not
    // the comparator's `string | number`.
    assert_eq!(
        reused_return("function f() { return 10 as number | string; }\nf;").as_deref(),
        Some("number | string")
    );
    // An arrow's expression body is the candidate.
    assert_eq!(
        reused_return("const g = () => 1 as number | string;\ng;").as_deref(),
        Some("number | string")
    );
}

#[test]
fn the_pseudo_return_needs_exactly_one_top_level_return() {
    // A nested return makes `typeFromSingleReturnExpression` give up.
    assert_eq!(
        reused_return(
            "function f(b: boolean) { if (b) { return 1 as number | string; } return 2 as number | string; }\nf;"
        ),
        None
    );
    // `as const` is not Direct: its operand answers, and a literal is not.
    assert_eq!(reused_return("function f() { return 1 as const; }\nf;"), None);
}

#[test]
fn a_contextually_typed_return_is_direct_only_as_an_assertion() {
    // Inside a call argument (`isContextuallyTyped`), a non-assertion
    // candidate is not inferred syntactically.
    assert_eq!(
        reused_return(
            "declare function id<T>(x: T): T;\nconst h = id(() => 1 as number | string);\nh;"
        )
        .as_deref(),
        Some("number | string")
    );
}

#[test]
fn a_written_predicate_is_reused_while_it_matches_the_signature() {
    // `typePredicatesOptionalChaining3`: the written `undefined | null`.
    assert_eq!(
        reused_return(
            "function isNil(value: unknown): value is undefined | null { return value == null; }\nisNil;"
        )
        .as_deref(),
        Some("value is undefined | null")
    );
}

#[test]
fn a_pseudo_return_does_not_see_an_enclosing_functions_parameters() {
    // `declarationEmitScopeConsistency3`: printed with no site, `v` is a
    // parameter of the ENCLOSING arrow, which the printed signature's scope
    // does not bind; the node is not reused.
    assert_eq!(
        reused_return("const f = (v: \"inner\") => () => null! as typeof v;\nf(\"inner\");"),
        None
    );
}

#[test]
fn a_constraint_reuses_its_written_node() {
    // `objectFreeze`: the lib overload's written `U | null | undefined | object`.
    let text = with_last_expression(
        "declare function f<T extends { [k: string]: U | null | undefined | object }, U extends string>(o: T): T;\nf;",
        |checker, id| {
            let signature = checker.signatures_of_type(id).expect("a function type")[0].clone();
            let parameter = signature.type_parameters[0].clone();
            let constraint = parameter.constraint.expect("a constraint");
            checker.type_parameter_constraint_text(&parameter, constraint, None)
        },
    );
    assert_eq!(text.as_deref(), Some("{ [k: string]: U | null | undefined | object; }"));
}

/// The property-slot decision for property `name` of the last expression.
fn reused_property(source: &str, name: &str) -> Option<String> {
    with_last_expression(source, |checker, id| {
        let symbol = checker.get_property_of_type(id, name).expect("the property");
        let r#type = checker.get_type_of_symbol(symbol);
        checker.reused_property_type_text(symbol, r#type, None)
    })
}

#[test]
fn an_optional_property_reuses_its_annotation_without_undefined() {
    // `spreadIdenticalTypesRemoved`: `owner?: string` prints `string`, not
    // `string | undefined` (`isOptionalAnnotated`'s `NEUndefined` strip).
    assert_eq!(
        reused_property("interface A { owner?: string }\ndeclare const a: A;\na;", "owner")
            .as_deref(),
        Some("string")
    );
}

#[test]
fn a_property_assignment_reuses_its_assertion() {
    // `spreadObjectNoCircular1`: `content: x as Foo | Box`.
    assert_eq!(
        reused_property(
            "interface Foo { f: 1 }\ninterface Box { b: 1 }\ndeclare const x: unknown;\n({ content: x as Foo | Box });",
            "content"
        )
        .as_deref(),
        Some("Foo | Box")
    );
}

#[test]
fn a_property_annotation_keeps_its_written_shape() {
    // `collisionRestParameterInType`: a rest parameter without an annotation
    // inside a reused function type prints `any` (`nodecopy.go:686`).
    assert_eq!(
        reused_property("declare const o: { prop: (i: number, ...rest) => void };\no;", "prop")
            .as_deref(),
        Some("(i: number, ...rest: any) => void")
    );
}
