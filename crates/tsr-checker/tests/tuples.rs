//! Tuple type nodes — the plain leg (`docs/architecture/checker-notes-tuple.md`).
//!
//! Every expectation comes from a baseline, not from intuition: the plain
//! spellings are the head of the corpus's printed-tuple population
//! (`[number, string]` 101, `[]` 90, `[number, number]` 72), and
//! `readonly [1, 2, 3]` is recorded 444 times.

use tsr_ast::Statement;
use tsr_checker::Checker;
use tsr_core::Arena;

#[test]
fn a_variadic_tuple_exposes_its_fixed_prefix() {
    assert_eq!(
        type_of_declaration(
            "interface Array<T> { length: number }\n\
             declare const t: [string, ...number[]]; const a = t[0];",
            "a",
        ),
        "string"
    );
}

#[test]
fn a_rest_tuple_exposes_its_unbounded_element() {
    assert_eq!(
        type_of_declaration(
            "interface Array<T> { length: number }\n\
             declare const t: [string, ...number[]]; const a = t[2];",
            "a",
        ),
        "number"
    );
}

#[test]
fn a_named_rest_tuple_exposes_its_fixed_prefix() {
    assert_eq!(
        type_of_declaration(
            "interface Array<T> { length: number }\n\
             type Tuple = [string, ...number[]]; declare const t: Tuple; const a = t[0];",
            "a",
        ),
        "string"
    );
}

#[test]
fn an_optional_tuple_has_a_union_of_possible_lengths() {
    assert_eq!(
        type_of_declaration(
            "declare const t: [string, number?, boolean?]; const a = t.length;",
            "a"
        ),
        "1 | 2 | 3"
    );
}

fn type_of_assignment_target(source: &str) -> String {
    let arena = Arena::new();
    let parsed = tsr_parser::parse(&arena, source);
    assert!(parsed.diagnostics.is_empty());
    let bound = tsr_binder::bind(
        &arena,
        parsed.source_file,
        &parsed.nodes,
        tsr_binder::FileInfo { name: "t.ts", text: source },
    );
    let mut checker = Checker::new(&bound, &parsed.nodes, &parsed.node_map);
    let Some(Statement::ExpressionStatement(statement)) = parsed.source_file.statements.last()
    else {
        panic!("an assignment statement");
    };
    let Some(tsr_ast::Expression::BinaryExpression(binary)) = statement.expression else {
        panic!("an assignment expression");
    };
    let t = checker.check_expression(binary.left.expect("assignment target"));
    checker.type_to_string(t)
}

#[test]
fn a_readonly_optional_tuple_length_write_is_erroneous() {
    assert_eq!(
        type_of_assignment_target("declare const t: readonly [number?]; t.length = 0;"),
        "any"
    );
}

#[test]
fn a_readonly_rest_tuple_element_write_is_erroneous() {
    assert_eq!(
        type_of_assignment_target(
            "interface Array<T> { length: number }\n\
         declare const t: readonly [number, ...number[]]; t[2] = 0;"
        ),
        "any"
    );
}

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
        &arena,
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
    // §79 retired the OPTIONAL half of this pin: `[string, number?]` now
    // mints with the `?` in the print and the mask read at index positions.
    assert_eq!(type_of_declaration("declare const t: [string, ...number[]];", "t"), "error");
    assert_eq!(
        type_of_declaration("declare const t: [string, number?];", "t"),
        "[string, (number | undefined)?]"
    );
    // §80 retired the LABELED half too: labels render, and the label owns
    // the `?`.
    assert_eq!(
        type_of_declaration("declare const t: [first: string, second: number];", "t"),
        "[first: string, second: number]"
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
fn a_tuple_indexed_by_a_literal_type_node_answers_the_element() {
    // **§621 landed the flip this fixture was waiting for**, and corrected the
    // prediction it carried. What stood here read: *"When indexed-access TYPE
    // nodes land, this expectation flips to `string` per
    // `declarationEmitTypeofIndexedAccessNoParens.types`-family baselines and
    // must be updated with a bar, not silently."*
    //
    // The bar is paid — §621 measured +5 conversions (`indexerWithTuple` 2,
    // `unionsOfTupleTypes1` 2, `partiallyNamedTuples` 1) against 1 GAP→WRONG
    // and zero R→W — but the predicted **`string` was wrong**, and the very
    // baseline it cites is what settles that: it records
    // `export type C = typeof C[keyof typeof C]` as `number`, i.e. the form
    // parses as `(typeof C)[…]`. So `typeof t[0]` is `(typeof t)[0]`, which for
    // `[number, string]` is element ZERO — `number`.
    //
    // Kept as the type-node road's pin: the VALUE road (`t[0]`) has answered
    // since §8 and is a different arm.
    assert_eq!(
        type_of_declaration(
            "declare const t: [number, string];\ndeclare const u: typeof t[0];",
            "u"
        ),
        "number"
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
    // §921: the `tuple[number]` form is no longer refused. §8's pair asserted
    // it as the unported case beside the ported ones; the union of the element
    // types is what `array_or_tuple_element_access` computes, and the reason it
    // never ran is that its call site sat past an earlier `return error` in the
    // branch for an index that names no property — which is every non-literal
    // index.
    assert_eq!(type_of_declaration(source, "e3"), "string | number");
}

/// §770: a tuple's non-numeric members come from `Array<T>`.
///
/// Upstream's tuple is a REFERENCE to a target whose base is
/// `Array<union of the element types>` (`createNormalizedTupleType`,
/// `checker.go:24148`), so array methods resolve through the ordinary
/// base-member road. This port mints a tuple as a bare named object with an
/// element list and no base, so it answered its numeric indices and nothing
/// else — §769 found that by giving a parameter its correct tuple type and
/// watching `.some(…)` go from RIGHT to GAP.
///
/// **This test does NOT redden when §770 is reverted, and is not claimed to.**
/// This harness builds no program, so `global_type_symbol("Array")` is `None`
/// and the fallback never fires here at all; what the assertion pins is that a
/// tuple's `length` is its element COUNT, which is true with or without the
/// fallback in this harness. §770's guard is the CORPUS, which is the right
/// instrument for it: +44 W→R / +43 G→R, and the `length` exclusion is itself
/// measured — letting `length` through the fallback took `tupleTypes`'
/// `readonly [number?]` from `0 | 1` to `number`, 2 R→W.
#[test]
fn a_tuple_length_is_its_element_count() {
    assert_eq!(
        type_of_declaration("declare const t: [number, string];\nconst n = t.length;", "n"),
        "2"
    );
}

/// `TupleNormalizer.add` (`checker.go:23440`) stores every optional element as
/// `addOptionalityEx(t, true, true)`, so a tuple inferred from optional
/// parameters carries `undefined` in each optional slot
/// (`genericRestParameters1.types:581`).
#[test]
fn a_tuple_inferred_from_optional_parameters_carries_undefined() {
    assert_eq!(
        type_of_declaration(
            "interface Array<T> { length: number }\n\
             declare function bind<T, U extends unknown[], V>(f: (x: T, ...rest: U) => V, x: T): (...rest: U) => V;\n\
             declare const g20: (x: number, y?: string, z?: boolean) => string[];\n\
             const g21 = bind(g20, 42);",
            "g21",
        ),
        "(y?: string | undefined, z?: boolean | undefined) => string[]"
    );
}
