//! Looking inside an object type: inherited members and what is *not* a member.
//!
//! Separate from `tests/types.rs` because the questions are different: these are
//! about which **symbol** a name resolves to on a type, not about how a type
//! prints. Where the answer is only observable as a symbol — a lookup that must
//! miss, where a miss and a gap print the same `any` — the assertion goes through
//! `get_property_of_type` directly rather than through a printed line that could
//! not tell them apart.

use tsr_binder::SymbolFlags;
use tsr_checker::Checker;
use tsr_core::Arena;

/// The printed type of the local `name`, wherever in the file it is declared.
fn type_of(source: &str, name: &str) -> String {
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
    for index in 0..parsed.nodes.len() {
        #[allow(clippy::cast_possible_truncation)]
        let id = tsr_ast::NodeId::new(index as u32);
        if let Some(symbol) = bound.lookup_local(id, name) {
            let ty = checker.get_type_of_symbol(symbol);
            return checker.type_to_string(ty);
        }
    }
    panic!("`{name}` is declared nowhere");
}

/// What `property` resolves to on the declared type of the top-level type
/// `owner`, as `(the symbol's name, its flags)`.
///
/// Goes through the symbol rather than the printed type, because the two
/// questions this file cares about — "is this a member at all" and "*whose*
/// member is it" — are both invisible in a printed line.
fn property_of(source: &str, owner: &str, property: &str) -> Option<(String, SymbolFlags)> {
    let arena = Arena::new();
    let parsed = tsr_parser::parse(&arena, source);
    assert!(parsed.diagnostics.is_empty(), "fixture must parse");
    let bound = tsr_binder::bind(
        &arena,
        parsed.source_file,
        &parsed.nodes,
        tsr_binder::FileInfo { name: "test.ts", text: source },
    );
    let root = tsr_ast::HasNodeId::node_id(parsed.source_file).expect("registered");
    let owner = bound.lookup_local(root, owner).expect("the owner is declared at the top level");
    let mut checker = Checker::new(&bound, &parsed.nodes, &parsed.node_map);
    let declared = checker.get_declared_type_of_symbol(owner);
    let found = checker.get_property_of_type(declared, property)?;
    let symbol = bound.symbols().get(found);
    Some((symbol.name.to_string(), symbol.flags))
}

/// The declaration a property came from, as the source text of its line.
///
/// Identity, not shape: an overridden member must answer with the *derived*
/// declaration and an inherited one with the *base*'s, and both print the same
/// type in the cases where a shortcut would be tempting.
fn declaring_line(source: &str, owner: &str, property: &str) -> String {
    let arena = Arena::new();
    let parsed = tsr_parser::parse(&arena, source);
    assert!(parsed.diagnostics.is_empty(), "fixture must parse");
    let bound = tsr_binder::bind(
        &arena,
        parsed.source_file,
        &parsed.nodes,
        tsr_binder::FileInfo { name: "test.ts", text: source },
    );
    let root = tsr_ast::HasNodeId::node_id(parsed.source_file).expect("registered");
    let owner = bound.lookup_local(root, owner).expect("the owner is declared at the top level");
    let mut checker = Checker::new(&bound, &parsed.nodes, &parsed.node_map);
    let declared = checker.get_declared_type_of_symbol(owner);
    let found = checker.get_property_of_type(declared, property).expect("the property is found");
    let declaration = bound.symbols().get(found).declarations[0];
    let span = parsed.nodes.span(declaration);
    let line_start = source[..span.start as usize].rfind('\n').map_or(0, |i| i + 1);
    let line_end = source[line_start..].find('\n').map_or(source.len(), |i| line_start + i);
    source[line_start..line_end].trim().to_string()
}

#[test]
fn a_base_classs_property_is_found_on_the_derived_class() {
    // The whole point: `p` is not in `C`'s members table, here or upstream.
    assert_eq!(
        type_of(
            "class B { p: number; }\nclass C extends B { m() { const x = this.p; return x; } }",
            "x"
        ),
        "number"
    );
}

#[test]
fn an_inherited_property_answers_with_the_bases_symbol_not_a_copy() {
    // `resolveObjectTypeMembers` layers base properties *under* the derived
    // ones, so the symbol that comes back for an inherited name belongs to the
    // base. A binder-side flattening would have produced a symbol on `C`, which
    // prints the same type and is the wrong identity.
    let source = "class B { p: number; }\nclass C extends B { q: string; }";
    assert_eq!(declaring_line(source, "C", "p"), "class B { p: number; }");
}

#[test]
fn a_derived_declaration_shadows_the_base_and_keeps_its_own_identity() {
    let source = "class B { p: number; }\nclass C extends B { p: string; }";
    assert_eq!(declaring_line(source, "C", "p"), "class C extends B { p: string; }");
    assert_eq!(
        type_of(
            "class B { p: number; }\n\
             class C extends B { p: string; m() { const x = this.p; return x; } }",
            "x"
        ),
        "string"
    );
}

#[test]
fn an_interface_inherits_through_extends_too() {
    let source = "interface B { p: number }\ninterface I extends B {}";
    assert_eq!(declaring_line(source, "I", "p"), "interface B { p: number }");
    assert_eq!(
        type_of(
            "interface B { p: number }\n\
             interface I extends B {}\n\
             declare const i: I;\n\
             const x = i.p;",
            "x"
        ),
        "number"
    );
}

#[test]
fn inheritance_is_transitive() {
    let source = "class A { p: number; }\nclass B extends A {}\nclass C extends B {}";
    assert_eq!(declaring_line(source, "C", "p"), "class A { p: number; }");
}

#[test]
fn a_circular_base_chain_terminates() {
    // `class A extends B` with `class B extends A` is a real cycle in the base
    // graph, and the corpus contains such cases deliberately. Without the guard
    // this recurses until the stack goes; the assertion is that the call returns
    // at all, and the value it returns is a miss.
    let source = "class A extends B { }\nclass B extends A { }";
    assert_eq!(property_of(source, "A", "p"), None);
    // A member that genuinely exists on the cycle is still found — the guard
    // stops the walk revisiting a symbol, not the walk itself.
    let source = "class A extends B { a: number; }\nclass B extends A { b: string; }";
    assert!(
        property_of(source, "A", "b").is_some(),
        "`b` is one hop away, before the cycle closes"
    );
}

#[test]
fn a_type_parameter_is_not_a_property_of_the_class() {
    // A class's type parameters live in the same `members` table as its
    // properties (`binder.go:429-441`), so `getPropertyOfObjectType`'s
    // `symbolIsValue` gate (`checker.go:21407`) is what keeps `new C().T` from
    // answering with `T`.
    //
    // This asserts through the symbol on purpose. `get_type_of_symbol` of a type
    // parameter is `errorType`, which prints `any` — exactly what a miss prints
    // — so a test on the printed line could not tell the gate from its absence.
    let source = "class C<T> { p: T; }";
    assert_eq!(property_of(source, "C", "T"), None);
    let (name, flags) = property_of(source, "C", "p").expect("`p` is a property");
    assert_eq!(name, "p");
    assert!(flags.contains(SymbolFlags::PROPERTY));
}

#[test]
fn implements_contributes_no_members() {
    // Upstream reads only the `extends` clause for base types
    // (`getEffectiveBaseTypeNode`). An `implements` clause is checked for
    // conformance and inherits nothing.
    let source = "interface B { p: number }\nclass C implements B { }";
    assert_eq!(property_of(source, "C", "p"), None);
}

#[test]
fn a_base_with_type_arguments_is_a_gap_rather_than_an_uninstantiated_answer() {
    // `class C extends B<number>` should give `p` the type `number`. Nothing
    // instantiates yet (`bd tsr-4sc.7`), so answering with `B`'s own `p` would
    // report `T`. A miss is the honest answer.
    let source = "class B<T> { p: T; }\nclass C extends B<number> { }";
    assert_eq!(property_of(source, "C", "p"), None);
    // The same base without arguments *is* followed, so the fixture is testing
    // the arguments and not the shape.
    let source = "class B { p: number; }\nclass C extends B { }";
    assert!(property_of(source, "C", "p").is_some());
}

#[test]
fn a_base_that_cannot_be_followed_does_not_let_a_later_base_answer_in_its_place() {
    // `interface I extends A, B<number>`: upstream would take `p` from whichever
    // of the two declares it. Skipping the base this port cannot follow and
    // answering from the other would be the wrong symbol whenever both declare
    // the name — so an unfollowable base makes the whole lookup a miss.
    let source = "interface A { p: number }\n\
                  interface B<T> { p: T }\n\
                  interface I extends A, B<number> {}";
    assert_eq!(property_of(source, "I", "p"), None);
}

// ---------------------------------------------------------------------------
// A class's or interface's type parameters, reached from the checker.
//
// The binder was already filing these correctly (`bd tsr-y4u.21`, corrected);
// what was missing was the resolver arm that reads the members table, and then
// a call site passing a meaning. These assert the whole path — annotation ->
// `get_type_from_type_reference` -> `resolve_name(.., SymbolFlags::TYPE)` ->
// `getDeclaredTypeOfSymbol` -> the printed name.
//
// **Before the arm existed these printed `any`**, because the reference resolved
// to nothing and `errorType` prints `any`. That is exactly the trap the method
// warns about: the binder bound `T` correctly all along, so a test written
// against the *symbol* would have passed for the wrong reason. These go through
// the printed type of a member, which did not.
// ---------------------------------------------------------------------------

/// The printed type of `property` on the top-level type `owner`.
fn type_of_property(source: &str, owner: &str, property: &str) -> String {
    let arena = Arena::new();
    let parsed = tsr_parser::parse(&arena, source);
    assert!(parsed.diagnostics.is_empty(), "fixture must parse");
    let bound = tsr_binder::bind(
        &arena,
        parsed.source_file,
        &parsed.nodes,
        tsr_binder::FileInfo { name: "test.ts", text: source },
    );
    let root = tsr_ast::HasNodeId::node_id(parsed.source_file).expect("registered");
    let owner = bound.lookup_local(root, owner).expect("the owner is declared at the top level");
    let mut checker = Checker::new(&bound, &parsed.nodes, &parsed.node_map);
    let declared = checker.get_declared_type_of_symbol(owner);
    let found = checker.get_property_of_type(declared, property).expect("the property is found");
    let ty = checker.get_type_of_symbol(found);
    checker.type_to_string(ty)
}

#[test]
fn a_class_type_parameter_resolves_in_a_member_annotation() {
    assert_eq!(type_of_property("class C<T> { p: T; }", "C", "p"), "T");
    // Two parameters, so the answer is the right one and not merely "a type
    // parameter" — `U` and `T` print differently.
    assert_eq!(type_of_property("class C<T, U> { p: U; }", "C", "p"), "U");
}

#[test]
fn an_interface_type_parameter_resolves_in_a_member_annotation() {
    assert_eq!(type_of_property("interface I<T> { p: T }", "I", "p"), "T");
}

#[test]
fn a_type_parameter_shadowed_by_an_outer_declaration_still_wins() {
    // `type T = string` at the file scope and `T` as the class's parameter. The
    // walk reaches the class before the file, so the parameter wins and the
    // member prints `T`, not `string`. Without the members arm the walk would
    // sail past the class and answer `string` — a *wrong* answer rather than a
    // gap, which is the failure mode worth a test of its own.
    assert_eq!(type_of_property("type T = string;\nclass C<T> { p: T; }", "C", "p"), "T");
}

// ---------------------------------------------------------------------------
// `typeof X` exposes the symbol's `exports`: statics, namespace exports, enum
// members. Ported from the tail of `resolveAnonymousTypeMembers`
// (`checker.go:20650`), whose last branch is `getExportsOfSymbol`
// (`checker.go:20672`).
// ---------------------------------------------------------------------------

fn value_property(source: &str, owner: &str, property: &str) -> Option<(String, SymbolFlags)> {
    let arena = Arena::new();
    let parsed = tsr_parser::parse(&arena, source);
    assert!(parsed.diagnostics.is_empty(), "fixture must parse");
    let bound = tsr_binder::bind(
        &arena,
        parsed.source_file,
        &parsed.nodes,
        tsr_binder::FileInfo { name: "test.ts", text: source },
    );
    let root = tsr_ast::HasNodeId::node_id(parsed.source_file).expect("registered");
    let owner = bound.lookup_local(root, owner).expect("the owner is declared at the top level");
    let mut checker = Checker::new(&bound, &parsed.nodes, &parsed.node_map);
    let value = checker.get_type_of_symbol(owner);
    let found = checker.get_property_of_type(value, property)?;
    let symbol = bound.symbols().get(found);
    Some((symbol.name.to_string(), symbol.flags))
}

#[test]
fn a_static_is_a_property_of_typeof_c() {
    // `submodule/conformance/staticMemberInitialization.types` records, for
    // `class C { static x = 1; }`:
    //
    // ```text
    // var r = C.x;
    // >r : number       (:18)
    // >C.x : number     (:19)
    // >C : typeof C     (:20)
    // ```
    //
    // `static x = 1` widens to `number` because a property declaration is not
    // `const`-declared, which is the existing initialiser path; what is new is
    // that the lookup reaches it at all.
    assert_eq!(type_of("class C { static x = 1; }\nvar r = C.x;", "r"), "number");
    // Through the symbol as well as the printed line, because the two questions
    // differ: this is the *static* `x`, from the class's `exports`.
    assert_eq!(
        value_property("class C { static x = 1; }", "C", "x"),
        Some(("x".to_string(), SymbolFlags::PROPERTY))
    );
    // A namespace export reaches the same arm — upstream's branch is one line
    // for "combinations of function, class, enum and module"
    // (`checker.go:20671`). `submodule/compiler/constDeclarations-access4.types`
    // records, for `declare namespace M { const x: number; }`:
    //
    // ```text
    // var a = M.x + 1;
    // >a : number       (:137)
    // >M.x : number     (:139)
    // >M : typeof M     (:140)
    // ```
    assert_eq!(
        type_of("declare namespace M { const x: number; }\nvar a = M.x + 1;", "a"),
        "number"
    );
}

#[test]
fn an_instance_member_does_not_leak_through_typeof_c() {
    // **The reason this arm reads `exports` and not `members`.** `crate::symbols`
    // recorded the hazard when it made `typeof C` carry no members table at all:
    // pointing the lookup at `members` resolves `C.x` against the *instance*
    // side, which answers a symbol upstream would never have given — a wrong
    // answer, not a missing one, and it prints as a plausible type.
    //
    // `y` is an instance property, so `typeof C` has no `y`.
    assert_eq!(value_property("class C { y = 1; }", "C", "y"), None);
    // And the converse, or the assertion above would hold for a lookup that had
    // simply stopped working: the static side of the *same* class is found.
    assert_eq!(
        value_property("class C { y = 1; static z = 1; }", "C", "z"),
        Some(("z".to_string(), SymbolFlags::PROPERTY))
    );
}

#[test]
fn the_shapes_typeof_x_still_gaps() {
    // **Every assertion below is paired with a positive control**, because a gap
    // test whose fixture is a syntactic *form* stops testing anything the moment
    // that form is ported, and — worse — can keep passing for a reason its
    // comment does not claim. The control is what distinguishes "this specific
    // step is unported" from "nothing here works at all".

    // **A static inherited from a base class — PORTED by §122**
    // (`checker-notes-narrow.md`; the thirtieth stand-in to come due). This
    // used to pin `error` on the argument that the walk needed
    // `getBaseConstructorTypeOfClass`'s construct signatures; §122 walks
    // `base_symbols_of` reading each base's `exports` instead, own-exports
    // first — upstream's `addInheritedMembers` (`checker.go:20690`) layering
    // (only absent names) reproduced by ordering.
    let inheriting = "class B { static x = 1; }\nclass C extends B {}\n";
    assert_eq!(type_of(&format!("{inheriting}var r = C.x;"), "r"), "number");
    // The control keeps its original job: read through `B` directly.
    assert_eq!(type_of(&format!("{inheriting}var r = B.x;"), "r"), "number");

    // **An exported type in a value position**, held out by `symbolIsValue`
    // (`checker.go:18916`). A namespace's `exports` holds its exported types too.
    let namespace = "namespace M { export interface I {} export const v = 1; }\n";
    assert_eq!(value_property(namespace, "M", "I"), None);
    // The control, and it is load-bearing here rather than ornamental: probed
    // directly, `M`'s exports table is `["v", "I"]` — `I` **is** in the table the
    // lookup reads. So the `None` above is the `symbolIsValue` gate rejecting it
    // and not an empty table, which is the difference between this assertion
    // testing the gate and testing nothing.
    assert_eq!(
        value_property(namespace, "M", "v"),
        Some(("v".to_string(), SymbolFlags::BLOCK_SCOPED_VARIABLE))
    );

    // **The type-literal case has no test here, deliberately.** The obvious one —
    // `value_property("let f: (x: number) => string;", "f", "foo")` is `None` —
    // is *vacuous*: probed directly, no symbol in that source has a non-empty
    // `members` or `exports` table at all, so it returns `None` whether or not
    // the flags gate exists. That is the same finding as the gate's
    // unobservability, recorded in `crate::members`, and asserting it would
    // dress a guaranteed `None` up as coverage.
}

#[test]
fn an_enum_member_is_a_property_of_typeof_e() {
    // This was a gap until the binder stopped filing enum members in the wrong
    // table. Upstream's `declareSymbolAndAddToSymbolTable` has a case of its own
    // for an enum container, and it takes `exports`
    // (`internal/binder/binder.go:436-437`); this port had `Node::EnumMember`
    // classified as `Destination::Members`, so the lookup above — which reads
    // `exports`, correctly — could never find it.
    assert_eq!(
        value_property("enum E { A, B }", "E", "B"),
        Some(("B".to_string(), SymbolFlags::ENUM_MEMBER))
    );

    // `submodule/conformance/validEnumAssignments.types:31` records
    // `>E.A : E.A` for the property access itself. A `const` declaration keeps
    // the initialiser's type unwidened (`checker.go:16898`), so the declared
    // type is that same `E.A`.
    assert_eq!(type_of("enum E { A, B }\nconst v = E.A;", "v"), "E.A");
}

#[test]
fn a_var_initialised_from_an_enum_member_widens_to_the_enum() {
    // This test carried an `#[ignore]` from the commit that made the line
    // reachable until the commit that made it right, with the baseline quoted
    // throughout rather than rewritten to assert the wrong answer of the day.
    // Removing the attribute *is* the verification: nothing about the assertion
    // changed, only whether the checker could satisfy it.
    //
    // Quoted, not extrapolated. `submodule/conformance/enumAssignability.types`
    // for `enum E { A }`:
    //
    // ```text
    // var e = E.A;
    // >e : E        (:15)
    // >E.A : E      (:16)
    // >E : typeof E (:17)
    // ```
    //
    // and `submodule/conformance/validNumberAssignments.types:29-30` records the
    // same two lines for the same shape. Note the *declaration* site in both
    // files prints `>A : E.A` (`enumAssignability.types:8`) — the same symbol,
    // two printed types, which is what makes this widening and not a property of
    // the enum.
    assert_eq!(type_of("enum E { A }\nvar e = E.A;", "e"), "E");
    // A multi-member enum too, so the assertion is not satisfied by the
    // single-member collapse it would be easy to mistake this rule for: `E` is a
    // union of two here and `E.A` still widens to the whole enum.
    assert_eq!(type_of("enum E { A, B }\nvar e = E.A;", "e"), "E");

    // **Three controls against over-widening**, because an arm placed first in
    // the chain can swallow more than it should.
    //
    // A `const` keeps the member type — `submodule/conformance/
    // validEnumAssignments.types:31` records `>E.A : E.A` — and it reaches this
    // function not at all, because `getWidenedLiteralTypeForInitializer` returns
    // early for a constant (`checker.go:16898`).
    assert_eq!(type_of("enum E { A, B }\nconst v = E.A;", "v"), "E.A");
    // The enum type itself is not a member type and must come back unchanged.
    //
    // **What stops it is `!fresh`, not the table** — probed rather than assumed,
    // because the reverse is the natural guess: the enum type is `EnumLike`, so
    // the flags test alone would match it. Measured, it is
    // `fresh=false, flags=ENUM_LITERAL | UNION`, and it carries no
    // `enum_member_owners` entry either. So `get_widened_literal_type` returns
    // at its first line and the enum arm never sees it. The table check is the
    // *second* line of defence here rather than the first, and this control
    // therefore pins the freshness gate; it would still hold if the table check
    // were removed, which is why it is described as covering that and not more.
    assert_eq!(type_of("enum E { A, B }\ndeclare var d: E;\nvar e = d;", "e"), "E");
    // And the four literal arms the enum arm now sits in front of still widen.
    assert_eq!(type_of("var s = \"a\";", "s"), "string");
    assert_eq!(type_of("var n = 1;", "n"), "number");
}
