//! `new C<T>()` — written type arguments instantiate the class
//! (`bd tsr-tgov`, `docs/architecture/checker-notes-callres.md` §14).
//!
//! Every expectation is copied from a `.types` baseline and named at the
//! assertion. Refused legs are asserted beside the ported one so the frontier
//! moving turns the test red rather than silently widening.

use tsr_ast::Statement;
use tsr_checker::Checker;
use tsr_core::Arena;

/// The printed type of the named variable's declaration.
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
    let mut checker = Checker::new(&bound, &parsed.nodes, &parsed.node_map);
    for statement in parsed.source_file.statements {
        let Statement::VariableStatement(node) = statement else { continue };
        for declaration in node.declaration_list.map(|l| l.declarations).unwrap_or_default() {
            let Some(tsr_ast::BindingName::Identifier(identifier)) = declaration.name else {
                continue;
            };
            if identifier.text != name {
                continue;
            }
            let symbol = bound
                .symbol_of(declaration.node_id.expect("registered"))
                .expect("the declaration must be bound");
            let id = checker.get_type_of_symbol(symbol);
            return checker.type_to_string(id);
        }
    }
    panic!("the fixture must declare `{name}`");
}

/// `conformance/genericSetterInClassType.types`:
/// `var c = new C<number>();` records `>c : C<number>` and
/// `>new C<number>() : C<number>`.
#[test]
fn written_type_arguments_instantiate_the_class() {
    let source = "class C<T> { m(x: T): void {} }
var c = new C<number>();";
    assert_eq!(type_of_declaration(source, "c"), "C<number>");
}

/// Two parameters, from the same family
/// (`conformance/objectTypesIdentityWithGenericConstructSignaturesOptionalParams2`
/// records `C<T, U>`).
#[test]
fn a_two_parameter_class_instantiates_positionally() {
    let source = "class C<T, U> { x: T; y: U; }
var c = new C<number, string>();";
    assert_eq!(type_of_declaration(source, "c"), "C<number, string>");
}

/// The instance type must be **interned to the same type** as the annotation's,
/// which is why the arm goes through `create_type_reference` rather than
/// building a name: `let c: C<number> = new C<number>()` is one type, and
/// `bd tsr-4qx`'s instantiated members hang off it.
#[test]
fn the_instantiation_is_the_same_type_the_annotation_builds() {
    let source = "class C<T> { m: T; }
var a: C<number> = new C<number>();
var b = new C<number>();";
    assert_eq!(type_of_declaration(source, "a"), type_of_declaration(source, "b"));
    // And the member reads through it — the seam `tsr-4qx` built.
    let member = "class C<T> { m: T; }
var b = new C<number>();
var got = b.m;";
    assert_eq!(type_of_declaration(member, "got"), "number");
}

/// `conformance/objectTypesIdentityWithGenericConstructSignaturesDifferingTypeParameterNames.types`:
/// `var b = { new<A>(x: A) { return new C<A>(x); } };` records
/// `>b : { "new"<A>(x: A): C<A>; }` — a **method** named `new` prints quoted,
/// because `{ new<A>(x: A): C<A>; }` would re-parse as a construct signature.
/// `classifyPropertyName` (`nodebuilderimpl.go:2384`) is that one special case.
#[test]
fn a_method_named_new_prints_quoted() {
    let source = "class C<A> { constructor(x: A) {} }
var b = { new<A>(x: A): C<A> { return new C<A>(x); } };";
    assert_eq!(type_of_declaration(source, "b"), r#"{ "new"<A>(x: A): C<A>; }"#);
    // Method-only: a *property* named `new` stays bare. Upstream's test is
    // `isMethod && name == "new"`, both halves.
    let property = "var p = { new: 1 };";
    assert_eq!(type_of_declaration(property, "p"), "{ new: number; }");
}

/// The refused legs, each beside the ported one. If one starts answering, the
/// leg has been built and its pair here must move.
#[test]
fn the_refused_legs_stay_gaps() {
    // No written type arguments: upstream infers them from the constructor's
    // arguments (`inferTypeArguments`), which is unported — 155 lines in
    // `newgen.rs`'s counterfactual.
    let inferred = "class C<T> { constructor(x: T) {} }
var a = new C(1);
var b = new C<number>(1);";
    assert_eq!(type_of_declaration(inferred, "a"), "error");
    assert_eq!(type_of_declaration(inferred, "b"), "C<number>");
    // An arity that differs from the class's type parameters: upstream errors
    // the whole call (`checkTypeArguments`), and `fillMissingTypeArguments`'
    // defaults are ported only for the no-candidate case (`bd tsr-1uz`).
    let arity = "class C<T, U> { x: T; }
var a = new C<number>();
var b = new C<number, string>();";
    assert_eq!(type_of_declaration(arity, "a"), "error");
    assert_eq!(type_of_declaration(arity, "b"), "C<number, string>");
    // A written type argument that itself gaps gaps the whole `new` —
    // `C<Unported>` is not `C<any>`, the tuple and array arms' rule.
    let gapped = "class C<T> { x: T; }
var a = new C<keyof { q: 1 }[]>();
var b = new C<string>();";
    assert_eq!(type_of_declaration(gapped, "b"), "C<string>");
    assert_ne!(type_of_declaration(gapped, "a"), "C<any>");
}
