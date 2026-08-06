//! Tuple type nodes — the plain leg (`docs/architecture/checker-notes-tuple.md`).
//!
//! Every expectation comes from a baseline, not from intuition: the plain
//! spellings are the head of the corpus's printed-tuple population
//! (`[number, string]` 101, `[]` 90, `[number, number]` 72), and
//! `readonly [1, 2, 3]` is recorded 444 times.

use tsr_ast::Statement;
use tsr_checker::Checker;
use tsr_core::Arena;

/// The printed type of the first declaration's name.
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

#[test]
fn a_plain_tuple_prints_its_elements() {
    assert_eq!(type_of_declaration("declare const t: [number, string];", "t"), "[number, string]");
    assert_eq!(type_of_declaration("declare const t: [number];", "t"), "[number]");
    // 90 instances in the corpus, the second most common tuple text.
    assert_eq!(type_of_declaration("declare const t: [];", "t"), "[]");
    // Nested, so the element rendering is the general one rather than a
    // primitive-only path.
    assert_eq!(
        type_of_declaration("declare const t: [number, [string, boolean]];", "t"),
        "[number, [string, boolean]]"
    );
}

#[test]
fn a_readonly_tuple_keeps_its_modifier() {
    // `>arr : readonly [1, 2, 3]`, recorded 444 times. The `readonly` is read
    // off the *parent* type operator, exactly as the array arm reads it.
    assert_eq!(
        type_of_declaration("declare const t: readonly [number, string];", "t"),
        "readonly [number, string]"
    );
}

#[test]
fn an_element_modifier_gaps_the_whole_tuple() {
    // The model this arm deliberately does not port. Each must be a *gap*, not
    // an approximation: `[string, ...number[]]` rendered as
    // `[string, number[]]` would be a wrong line where there is a missing one.
    assert_eq!(type_of_declaration("declare const t: [string, ...number[]];", "t"), "error");
    assert_eq!(type_of_declaration("declare const t: [string, number?];", "t"), "error");
    assert_eq!(
        type_of_declaration("declare const t: [first: string, second: number];", "t"),
        "error"
    );
}

#[test]
fn a_gap_in_an_element_gaps_the_tuple() {
    // `[Unported, string]` is not `[any, string]` — the rule the array arm and
    // `get_instantiated_type_reference` already state. `keyof` is unported and
    // answers `errorType`, so it is the element that exercises the rule.
    assert_eq!(type_of_declaration("declare const t: [keyof string, string];", "t"), "error");

    // **An unresolved *name* is deliberately not a gap here**, and this
    // assertion was first written as `error` from intuition and was wrong.
    // `bd tsr-eep` mints a type printing the written name for a reference that
    // does not resolve, because upstream reports `TS2304` and *renders the
    // name anyway* — `conformance/parserRealSource11` records 1,006 such
    // errors and zero ` : any` lines. So the tuple embeds it and prints it,
    // which is upstream's line. The array arm behaves identically, and for the
    // same reason: both test identity against `intrinsics.error`, which an
    // unresolved-name type is not.
    assert_eq!(
        type_of_declaration("declare const t: [Unresolved, string];", "t"),
        "[Unresolved, string]"
    );
}

#[test]
fn the_same_tuple_written_twice_is_one_type() {
    // Identity, which is what the intern map is for: without it a union of two
    // spellings prints both constituents.
    assert_eq!(
        type_of_declaration("declare const t: [number, string] | [number, string];", "t"),
        "[number, string]"
    );
    // And two *different* tuples stay two, so the key is the elements and not
    // merely the arity. The ORDER here is the comparator's, not the source's:
    // a var-annotation union sorts — `contextualSignatureInstantiation.types`
    // records `var b: number | string;` as `>b : string | number` — and
    // same-arity tuples compare elementwise (`compareTupleTypes` →
    // `compareTypeLists`), so `[string, ...]` sorts before `[number, ...]`
    // exactly as bare `string` sorts before `number`. This expectation
    // previously asserted source order, written from intuition before the
    // `compare_types` tuple arm existed (`bd tsr-5ll`); the ASCII-text
    // comparison it leaned on happened to coincide.
    assert_eq!(
        type_of_declaration("declare const t: [number, string] | [string, number];", "t"),
        "[string, number] | [number, string]"
    );
}

#[test]
fn a_tuple_has_no_members_and_that_is_deliberate() {
    // The safety property the arm rests on: nothing structural about a tuple
    // is claimed, so every consumer that could answer wrongly stays a gap.
    // If this ever passes with a real type, the arm has grown members and the
    // no-loss argument in `checker-notes-tuple.md` §3 needs re-measuring.
    assert_eq!(
        type_of_declaration(
            "declare const t: [number, string];\ndeclare const u: typeof t[0];",
            "u"
        ),
        "error"
    );
}
