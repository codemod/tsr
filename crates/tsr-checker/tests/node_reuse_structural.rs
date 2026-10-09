//! The pseudochecker's STRUCTURAL kinds (`docs/parity/notes/r5-nodereuse2.md`):
//! a single call signature and an object literal, found equivalent to the
//! checker's type by `pseudoTypeEquivalentToType`
//! (`pseudotypenodebuilder.go:362`) and printed by `pseudoTypeToNode`
//! (`:48`). Native asks them only with a print site, so each test asks the
//! decision at the final expression statement, the way a site-aware printer
//! does.

use tsr_ast::{Node, NodeId, Statement};
use tsr_checker::{Checker, TypeId};
use tsr_core::Arena;

/// Run `ask` against the type of the final expression statement, with that
/// statement's expression as the print site.
fn at_last_expression<R>(
    source: &str,
    ask: impl FnOnce(&mut Checker<'_, '_>, TypeId, NodeId) -> R,
) -> R {
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
    let site = Node::from(last).node_id().expect("a parsed node");
    let id = checker.check_expression(last);
    ask(&mut checker, id, site)
}

/// The return-slot decision for the only signature of the last expression,
/// at the site and with none.
fn reused_return(source: &str) -> (Option<String>, Option<String>) {
    at_last_expression(source, |checker, id, site| {
        let signature = checker.signatures_of_type(id).expect("a function type")[0].clone();
        (
            checker.reused_return_text(&signature, Some(site)),
            checker.reused_return_text(&signature, None),
        )
    })
}

/// The property-slot decision for property `name` of the last expression.
fn reused_property(source: &str, name: &str) -> Option<String> {
    at_last_expression(source, |checker, id, site| {
        let symbol = checker.get_property_of_type(id, name).expect("the property");
        let r#type = checker.get_type_of_property_of_type(id, name).expect("its type");
        checker.reused_property_type_text(symbol, r#type, Some(site))
    })
}

#[test]
fn a_returned_function_is_a_single_call_signature_scoped_by_the_printed_signature() {
    // `declarationEmitScopeConsistency3`: the outer signature's scope binds
    // `v`, so the inner return's `typeof v` survives at a site that cannot
    // see `v`. With no site the structural arm is not asked.
    assert_eq!(
        reused_return("const f = (v: \"inner\") => () => null! as typeof v;\nf;"),
        (Some("() => typeof v".to_string()), None)
    );
}

#[test]
fn an_inferred_member_of_a_structural_pseudo_type_refuses_it() {
    // `(y: number) => y` returns an identifier, which the pseudochecker
    // answers Inferred; this port does not ask its widened expression type
    // (§2), so the structure is serialized from the type.
    assert_eq!(reused_return("const f = () => (y: number) => y;\nf;").0, None);
}

#[test]
fn an_annotated_accessor_pair_keeps_its_accessors() {
    // `circularObjectLiteralAccessors`: `getAccessorMember` keeps both
    // accessors when both are annotated.
    assert_eq!(
        reused_property(
            "const a = { b: { get foo(): string { return \"\"; }, set foo(value: string) {} } };\na;",
            "b"
        )
        .as_deref(),
        Some("{ get foo(): string; set foo(value: string); }")
    );
}

#[test]
fn a_fresh_object_literal_requires_widening_and_is_serialized() {
    // `ObjectFlagsRequiresWidening`: the literal expression's own type is
    // fresh, so its property is not reused (`circularObjectLiteralAccessors`
    // 0:1 prints `b: { foo: string; }`).
    assert_eq!(
        reused_property(
            "({ b: { get foo(): string { return \"\"; }, set foo(value: string) {} } });",
            "b"
        ),
        None
    );
}

#[test]
fn a_reused_member_name_is_reclassified() {
    // `reuseName` (`nodecopy.go:24`): `"cli"` is identifier text and prints
    // bare; `"@ns/dep"` stays a string (`jsDeclarationsPackageJson`).
    assert_eq!(
        reused_property(
            "const a = { b: { \"cli\": \"x\" as string, \"@ns/dep\": \"y\" as string } };\na;",
            "b"
        )
        .as_deref(),
        Some("{ cli: string; \"@ns/dep\": string; }")
    );
}

#[test]
fn an_accessor_property_reuses_its_written_annotation_under_circularity() {
    // `circularAccessorAnnotations` (`tsr-2zk.1129`): `serializeTypeForDeclaration`
    // asks `GetTypeOfAccessor` for an accessor declaration. Circularity made
    // the property `any`, which is also `getTypeFromTypeNode(typeof c1.foo)`,
    // so the written query is reused. tsgo: `{ readonly foo: typeof c1.foo; }`.
    assert_eq!(
        reused_property("declare const c1: { get foo(): typeof c1.foo; };\nc1;", "foo").as_deref(),
        Some("typeof c1.foo")
    );
    // A setter alone is typed by its parameter's annotation (`typeFromAccessor`
    // falls back to the pair's other declarations). tsgo: `{ foo: typeof c2.foo; }`.
    assert_eq!(
        reused_property("declare const c2: { set foo(value: typeof c2.foo); };\nc2;", "foo")
            .as_deref(),
        Some("typeof c2.foo")
    );
}
