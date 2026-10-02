//! Calling a callee whose type prints as a **name** — `bd tsr-4sa`.
//!
//! An interface's call and construct signatures live in its *members*, not in
//! its symbol's declarations, so `get_signatures_of_symbol` answers an empty
//! list for one and `get_signature_of_named_type` is the route that finds them.
//! See
//! [`docs/architecture/checker-notes-namedcallee.md`](../../../docs/architecture/checker-notes-namedcallee.md).
//!
//! # Every expectation here comes from a baseline
//!
//! `docs/conventions.md` records a cycle in which three expectations in a file
//! whose own header promised ground truth were written from intuition and all
//! three were wrong. So, by line:
//!
//! - `conformance/parserRealSource12.types:105` records
//!   `>this.pre(ast, parent, this) : AST` for `interface IAstWalkCallback { (…): AST }`
//!   — a call through a call-signature interface answers the signature's return
//!   type. That case converted 13 lines in the measured build.
//! - `compiler/inheritedOverloadedSpecializedSignatures` is the case the
//!   heritage refusal is *for*: seven of its lines want `void`, `boolean`,
//!   `boolean[]`, `number`, `number[]` and `string[]` off a callee whose direct
//!   call signature returns `string`, because the rest of the overload set is on
//!   a base interface this port does not fold in.
//!
//! # The refusals are asserted as pairs, deliberately
//!
//! `docs/architecture/checker-notes-tuple.md`'s standing prophylactic: a test
//! that only asserts a gap flips to green the day the gap is filled and stops
//! discriminating. Each refusal below sits beside the form that *does* answer,
//! so the pair keeps testing the boundary rather than the state.

use tsr_ast::Statement;
use tsr_checker::Checker;
use tsr_core::Arena;

/// Type the initialiser of the last statement, after any set-up declarations.
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
    let declaration = statement
        .declaration_list
        .and_then(|list| list.declarations.first().copied())
        .expect("one declaration");
    let initialiser = declaration.initializer.expect("an initialiser");
    let id = checker.check_expression(initialiser);
    checker.type_to_string(id)
}

#[test]
fn a_call_signature_member_answers_its_return_type() {
    // `conformance/parserRealSource12.types:105`, reduced to one file:
    // `>this.pre(ast, parent, this) : AST`.
    assert_eq!(
        type_of_last(
            "interface AST { kind: string; }\n\
             interface IAstWalkCallback { (ast: AST): AST; }\n\
             declare const pre: IAstWalkCallback;\n\
             const walked = pre(null as unknown as AST);"
        ),
        "AST"
    );
}

#[test]
fn a_construct_signature_member_answers_its_return_type() {
    assert_eq!(
        type_of_last(
            "interface Made { m: string; }\n\
             interface MadeConstructor { new (): Made; }\n\
             declare const C: MadeConstructor;\n\
             const made = new C();"
        ),
        "Made"
    );
}

#[test]
fn an_overload_set_that_agrees_needs_no_selection() {
    // `DateConstructor`'s four construct signatures all return `Date`, which is
    // 128 of the measured build's converted lines. Agreement is what makes the
    // answer available without assignability.
    assert_eq!(
        type_of_last(
            "interface Made { m: string; }\n\
             interface MadeConstructor { new (): Made; new (a: string): Made; }\n\
             declare const C: MadeConstructor;\n\
             const made = new C(\"a\");"
        ),
        "Made"
    );
}

#[test]
fn an_overload_set_that_disagrees_selects_by_the_subtype_pass() {
    // FLIPPED at §273: this asserted `error` with the reason "choosing
    // between these is `resolveCall`'s assignability, which this port has
    // over primitives only" — and `"a"` against `string` IS primitives, so
    // the reason named its own repair. The §273 pick runs upstream's subtype
    // pass over the clean prefix: `new ()` fails arity, `"a"` is a subtype
    // of `string`, `B` is upstream's answer (`checker.go:8924`'s first run).
    assert_eq!(
        type_of_last(
            "interface A { a: string; }\n\
             interface B { b: string; }\n\
             interface MadeConstructor { new (): A; new (a: string): B; }\n\
             declare const C: MadeConstructor;\n\
             const made = new C(\"a\");"
        ),
        "B"
    );
}

#[test]
fn a_generic_signature_member_infers_from_arguments() {
    // This test pinned the OPPOSITE ("still a gap") until §74, then pinned
    // `Box<1>` on the reasoning "exactly as the call road keeps it for
    // `id(1)`" — an INDUCTION, and §162 corrected it against the oracle:
    // `getCovariantInference` widens a literal candidate unless the
    // parameter is at top level IN THE RETURN TYPE
    // (`isTypeParameterAtTopLevelInReturnType`, `inference.go:1442/1501`).
    // `id(1)` returns `T` — top level, keep `1`. `new C(1)` returns
    // `Box<T>` — NOT top level, widen. The corpus says so directly:
    // `isomorphicMappedTypeInference` records `>box(42) : Box<number>` for
    // `declare function box<T>(v: T): Box<T>`.
    assert_eq!(
        type_of_last(
            "interface Box<T> { value: T; }\n\
             interface BoxConstructor { new <T>(value: T): Box<T>; }\n\
             declare const C: BoxConstructor;\n\
             const made = new C(1);"
        ),
        "Box<number>"
    );
}

#[test]
fn an_interface_with_a_heritage_clause_folds_its_bases_signatures_in() {
    // **A stand-in come due, and it named its own fix.** This asserted `error`
    // on the ground that "the direct members are only part of the candidate
    // set — upstream's `resolveDeclaredMembers` folds the base's signatures in
    // — so answering from them is answering off data known to be incomplete",
    // and priced the refusal at 20 convertible lines to prevent 7 wrong ones.
    //
    // §215 folds them in, so the data is no longer incomplete and the premise
    // is gone. Both signatures here return `AST`, the all-equal-return gate
    // passes over the WHOLE set rather than over a truncated one, and the
    // answer is upstream's.
    assert_eq!(
        type_of_last(
            "interface AST { kind: string; }\n\
             interface Base { (): AST; }\n\
             interface Derived extends Base { (a: string): AST; }\n\
             declare const pre: Derived;\n\
             const walked = pre(\"a\");"
        ),
        "AST"
    );
}

#[test]
fn a_symbol_call_in_a_const_position_gaps_rather_than_printing_symbol() {
    // The stand-in came due (the twenty-eighth): §26 MINTS the fresh
    // `unique symbol` this fixture existed to keep un-faked — the 291-line
    // row converted (+377), and the name now pins the mint rather than the
    // gap.
    assert_eq!(
        type_of_last(
            "interface SymbolConstructor { (description?: string): symbol; }\n\
             declare const Symbol: SymbolConstructor;\n\
             const s = Symbol();"
        ),
        "unique symbol"
    );
}

#[test]
fn a_symbol_call_outside_a_const_position_answers_symbol() {
    // The pair, and the reason the refusal above is on the *position* rather
    // than on the return type: `getESSymbolLikeTypeForNode` (`checker.go:22982`)
    // falls through to `esSymbolType` when the declaration is not a valid ES
    // symbol declaration, so `let` answers `symbol` upstream too. 150 of the
    // measured build's converted lines are this form.
    assert_eq!(
        type_of_last(
            "interface SymbolConstructor { (description?: string): symbol; }\n\
             declare const Symbol: SymbolConstructor;\n\
             let s = Symbol();"
        ),
        "symbol"
    );
}

/// §215. An interface's call signatures include its bases'.
///
/// `resolveDeclaredMembers` unions the declared signatures with the inherited
/// ones (`checker.go:18410`'s interface arm). This port declined any interface
/// carrying a heritage clause outright — a refusal whose scope was wider than
/// its reason — so `interface I7 extends I6 {}` over
/// `interface I6 { (): void }` could not be called at all.
/// `compiler/interfaceDeclaration1` records `>v1() : void`.
#[test]
fn an_interface_inherits_its_bases_call_signature() {
    assert_eq!(
        type_of_last(
            "interface A { (): void }\ninterface B extends A { }\ndeclare var b: B;\nconst x = b();"
        ),
        "void"
    );
    // Two levels, because the walk recurses.
    assert_eq!(
        type_of_last(
            "interface A { (): string }\ninterface B extends A { }\ninterface C extends B { }\ndeclare var c: C;\nconst x = c();"
        ),
        "string"
    );
}

/// This road cannot observe the ORDER of the folded signatures —
/// `get_signature_of_named_type` declines any candidate set whose returns are
/// not all equal, so a fixture that could tell the orders apart is refused for
/// that reason first. But the mutation run showed it observes something better
/// than nothing: **it detects the base's PRESENCE.** Skip the `extends` walk
/// and `B` has one signature returning `number`, so the answer becomes
/// `number` rather than a decline. So this pins that the base really is being
/// folded in, by the decline it causes.
///
/// ~~The day overload selection reaches here, this fixture fails and asks to
/// be rewritten — which is the right time to pin the order.~~ **That day was
/// §273**, and this now pins the ORDER as the comment asked: the derived
/// interface's own signature folds BEFORE its base's, so upstream's subtype
/// pass answers `number` for the zero-argument call — a fixture that could
/// only gap while selection was absent.
#[test]
fn differing_returns_across_the_heritage_boundary_pick_the_derived_first() {
    assert_eq!(
        type_of_last(
            "interface A { (): string }\ninterface B extends A { (): number }\ndeclare var b: B;\nconst x = b();"
        ),
        "number"
    );
}

/// resolveObjectTypeMembers applies a heritage mapper to inherited signatures.
/// Pinned tsgo declares x: string; this formerly asserted the unported gap.
#[test]
fn an_instantiated_base_substitutes_its_return() {
    assert_eq!(
        type_of_last(
            "interface A<T> { (): T }\ninterface B extends A<string> { }\ndeclare var b: B;\nconst x = b();"
        ),
        "string"
    );
}
