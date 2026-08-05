//! `get x()` / `set x(v)` — which of the four sources an accessor's type comes
//! from, and which of them this port does not have.
//!
//! Every expectation is quoted from a `.types` baseline rather than reasoned
//! about. Accessors are the one place in `getTypeOfSymbol` where the fallback is
//! upstream's *answer* (`anyType`) rather than upstream's failure, so guessing
//! which lines are gaps would be easy and would corrupt the instrument.

use tsr_checker::Checker;
use tsr_core::Arena;

/// Type the accessor named `member` declared inside the first interface or class.
fn type_of_member(source: &str, member: &str) -> String {
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
    // The accessor's symbol lives in its container's member table, reached
    // through the container's own symbol rather than through file scope.
    let container = parsed
        .source_file
        .statements
        .iter()
        .find_map(|statement| match statement {
            tsr_ast::Statement::ClassDeclaration(node) => node.node_id,
            tsr_ast::Statement::InterfaceDeclaration(node) => node.node_id,
            _ => None,
        })
        .expect("the fixture must start with a class or interface");
    let container = bound.symbol_of(container).expect("the container is bound");
    let symbol = *bound
        .symbols()
        .get(container)
        .members
        .get(member)
        .unwrap_or_else(|| panic!("`{member}` is declared"));

    let mut checker = Checker::new(&bound, &parsed.nodes, &parsed.node_map);
    let id = checker.get_type_of_symbol(symbol);
    checker.type_to_string(id)
}

#[test]
fn a_getters_return_annotation_is_the_accessors_type() {
    // `>property : number` for `get property(): number { return 1; }`, and
    // `>foo : string` for the bodiless `get foo(): string;` — both quoted from
    // `.types` baselines under
    // `vendor/typescript-go/testdata/baselines/reference/submodule/compiler`.
    assert_eq!(
        type_of_member("class C { get property(): number { return 1; } }", "property"),
        "number"
    );
    assert_eq!(type_of_member("interface I { get foo(): string; }", "foo"), "string");
}

#[test]
fn a_setters_parameter_annotation_is_used_when_the_getter_has_none() {
    // `compiler/accessorBodyInTypeContext.types` records
    //
    //     set foo(v: any) { }
    //     >foo : any
    //
    // The annotation is on the setter's PARAMETER, not on the setter's own type
    // slot — `getEffectiveSetAccessorTypeAnnotationNode` (`checker.go:20118`).
    // Reading `node.r#type` for a setter answers `None` every time.
    assert_eq!(type_of_member("class C { set foo(v: string) { } }", "foo"), "string");

    // And the getter's annotation WINS when both are present, which is the only
    // observable consequence of upstream's ordering at `checker.go:18522`
    // vs `:18524`. Two different types are what make the order visible at all.
    assert_eq!(
        type_of_member("class C { get x(): number { return 1; } set x(v: string) { } }", "x"),
        "number"
    );
}

#[test]
fn an_accessor_with_nothing_to_go_on_is_any_because_that_is_upstreams_answer() {
    // The one place in this module where the fallback is `any` rather than
    // `error`. `checker.go:18545` assigns `c.anyType` and reports an
    // implicit-any diagnostic; the type is a real answer, not a failure, so
    // reporting `error` here would mark a correct line as missing.
    //
    // A bodiless, unannotated getter is the fixture that reaches it: nothing to
    // annotate and nothing to infer from.
    assert_eq!(type_of_member("interface I { get foo(); }", "foo"), "any");
}

#[test]
fn an_unannotated_getter_with_a_body_gaps_rather_than_claiming_any() {
    // `compiler/accessorBodyInTypeContext.types` records
    //
    //     get foo() { return 0 }
    //     >foo : number
    //
    // — upstream infers `number` from the body (`checker.go:18531`).
    // `getReturnTypeFromBody` is reachable only through `crate::signatures`,
    // which does not expose it, so this gaps.
    //
    // The load-bearing part is that it gaps *before* the `any` arm above.
    // Falling through to `any` would produce a plausible, wrong, and
    // indistinguishable-from-computed line on every inferable accessor in the
    // corpus — the exact failure the `errorType`-not-`anyType` rule exists to
    // prevent, arrived at from the opposite direction.
    assert_eq!(type_of_member("class C { get foo() { return 0; } }", "foo"), "error");
}
