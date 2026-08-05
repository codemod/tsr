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
