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
    // §3's "nothing structural is claimed" argument was deliberately SPENT by
    // §8 (`t[0]` answers the element, with its own registered bar), so this
    // fixture no longer guards the whole claim — what keeps it red is the
    // *type-node* path alone: `typeof t[0]` is an `IndexedAccessTypeNode`,
    // which `get_type_from_type_node` has no arm for. When indexed-access
    // TYPE nodes land, this expectation flips to `string` per
    // `declarationEmitTypeofIndexedAccessNoParens.types`-family baselines and
    // must be updated with a bar, not silently.
    assert_eq!(
        type_of_declaration(
            "declare const t: [number, string];\ndeclare const u: typeof t[0];",
            "u"
        ),
        "error"
    );
}

/// `t[0]` answers the element — `checker-notes-tuple.md` §8, which spends
/// §3's "no members" safety argument deliberately. Every expectation is a
/// line of `conformance/indexerWithTuple.types`:
///   var ele10 = strNumTuple[0];   >strNumTuple[0] : string
///   var ele11 = strNumTuple[1];   >strNumTuple[1] : number
///   var ele12 = strNumTuple[2];   >strNumTuple[2] : undefined
///   var ele15 = strNumTuple[quoted "0"]; >ele15 : string
///   var ele13 = strNumTuple[idx0] (idx0: number)  >ele13 : string | number — REFUSED, stays a gap
#[test]
fn a_tuple_element_access_answers_the_element() {
    let source = "declare const t: [string, number];\nvar e0 = t[0];\nvar e1 = t[1];\nvar e2 = t[2];\nvar e5 = t[\"0\"];\nvar idx0 = 0;\nvar e3 = t[idx0];";
    assert_eq!(type_of_declaration(source, "e0"), "string");
    assert_eq!(type_of_declaration(source, "e1"), "number");
    assert_eq!(type_of_declaration(source, "e2"), "undefined");
    assert_eq!(type_of_declaration(source, "e5"), "string");
    // The `tuple[number]` form needs the union of the element types and is
    // refused in §8 — the PAIR rule: the unported case asserted beside the
    // ported ones.
    assert_eq!(type_of_declaration(source, "e3"), "error");
}
