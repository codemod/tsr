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
        &arena,
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
fn an_unannotated_getter_is_inferred_from_its_body() {
    // `compiler/accessorBodyInTypeContext.types` records
    //
    //     get foo() { return 0 }
    //     >foo : number
    //
    // — upstream's case 4 (`checker.go:18531`), reached only because neither
    // annotation arm answered.
    //
    // **This test exists to reach the ACCESSOR arm**, not the inference beneath
    // it: `tests/return_inference.rs` already covers the inference through the
    // signature path, so a test that only proved "inference works" would pass
    // even if `get_return_type_from_body` read the wrong field off a
    // `GetAccessorDeclaration`. The accessor arm was shipped untested for
    // exactly that reason and this is the caller that exercises it.
    assert_eq!(type_of_member("class C { get foo() { return 0; } }", "foo"), "number");
    // A getter whose body CANNOT COMPLETE is `void`, not `never`:
    // `mayReturnNever` (`checker.go:20312`) covers a function expression, an
    // arrow and an object-literal method, and an accessor is none of the three.
    //
    // The fixture must `throw`. An empty body — `get foo() { }` — answers
    // `void` under either reading, so it exercises the arm without
    // discriminating on it; flipping `may_return_never` to `true` left it green.
    // `throw` is where the two readings diverge, and it reddens.
    assert_eq!(type_of_member("class C { get foo() { throw 1; } }", "foo"), "void");
    // No "control" assertion from the method side here: I guessed one and it
    // was wrong (a method answers `() => void`, not what I assumed), and a
    // guessed control is worse than none. What pins this is the mutation —
    // flipping `may_return_never` to `true` reddens the `throw` fixture.
    // And the annotation still wins over the body — case 1 before case 4, with
    // a body whose inferred type DIFFERS from the annotation, or the ordering
    // would not be observable.
    assert_eq!(
        type_of_member("class C { get foo(): string { return null as any; } }", "foo"),
        "string"
    );
}

#[test]
fn an_inference_this_port_cannot_make_is_error_and_never_a_plausible_any() {
    // `compiler/accessorBodyInTypeContext.types` records
    //
    //     get foo() { return 0 }
    //     >foo : number
    //
    // Upstream's `getReturnTypeFromBody` ALWAYS produces a type, so upstream
    // never falls from case 4 to case 5. This port's inference has gaps, and
    // every one of them is a declaration upstream would have inferred — so
    // `None` must become `errorType`. Letting it fall to the `any` arm would
    // print a plausible, wrong, indistinguishable-from-computed line on exactly
    // the accessors that have a real answer.
    //
    // Two return statements of distinct literal types: the sixteenth
    // unported-stand-in fixture to come due. Until the ninth session the
    // aggregate declined and this asserted `error`; §11's reduction answers,
    // and the corpus pins the shape — a multi-return union keeps its regular
    // literals (`compiler/capturedLetConstInLoop8.types` records
    // `() => "123" | "456" | undefined`), strings ordering before numbers by
    // `TypeFlags`.
    assert_eq!(
        type_of_member("class C { get foo() { if (1) { return 1; } return \"a\"; } }", "foo"),
        "\"a\" | 1"
    );
    // The discriminator for the `unwrap_or`: an accessor with NO body and no
    // annotation reaches case 5 and is a computed `any`. Same test, two
    // outcomes, so neither arm can be deleted without the other noticing.
    assert_eq!(type_of_member("interface I { get foo(); }", "foo"), "any");
}
