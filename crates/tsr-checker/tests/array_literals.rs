//! What an array literal **expression** must get right.
//!
//! These need a global `Array` for the same reason the array *type* tests do —
//! the result is a reference to it — so they bind a stand-in lib alongside the
//! fixture.
//!
//! See [`docs/architecture/checker.md`](../../../docs/architecture/checker.md),
//! "Array literals: two pieces of machinery meeting".

use tsr_ast::{NodeMap, NodeTable, Statement};
use tsr_checker::Checker;
use tsr_core::Arena;

const LIB: &str = "interface Array<T> {}\ninterface ReadonlyArray<T> {}\n";

/// The printed type of the first statement's first declaration's initialiser.
/// Type the LAST array literal in the fixture, wherever it sits. §887's arm is
/// about a literal in argument position, which no initialiser-shaped fixture can
/// reach.
fn type_of_last_array_literal(source: &str) -> String {
    let arena = Arena::new();
    let mut nodes = NodeTable::new();
    let mut node_map = NodeMap::new();
    let options = tsr_parser::ParseOptions::default();
    let lib = tsr_parser::parse_into(&arena, LIB, options, &mut nodes, &mut node_map);
    let file = tsr_parser::parse_into(&arena, source, options, &mut nodes, &mut node_map);
    assert!(
        file.diagnostics.is_empty(),
        "fixture must parse: {:?}",
        file.diagnostics.iter().map(tsr_diagnostics::Diagnostic::text).collect::<Vec<_>>()
    );
    let bound = tsr_binder::bind_into(
        tsr_binder::BindResult::empty(),
        &arena,
        lib.source_file,
        &nodes,
        tsr_binder::FileInfo { name: "lib.d.ts", text: LIB },
    );
    let bound = tsr_binder::bind_into(
        bound,
        &arena,
        file.source_file,
        &nodes,
        tsr_binder::FileInfo { name: "test.ts", text: source },
    );
    let mut checker = Checker::new(&bound, &nodes, &node_map);
    let mut last = None;
    for raw in 0..u32::try_from(nodes.len()).expect("fits") {
        let id = tsr_ast::NodeId::new(raw);
        if nodes.kind(id) == tsr_ast::SyntaxKind::ArrayLiteralExpression {
            last = Some(id);
        }
    }
    let id = last.expect("the fixture must contain an array literal");
    let node = node_map.get(id).expect("the literal is in the map");
    let expression = tsr_ast::Expression::try_from(node).expect("an expression");
    let ty = checker.check_expression(expression);
    checker.type_to_string(ty)
}

fn type_of_initialiser(source: &str) -> String {
    let arena = Arena::new();
    let mut nodes = NodeTable::new();
    let mut node_map = NodeMap::new();
    let options = tsr_parser::ParseOptions::default();
    let lib = tsr_parser::parse_into(&arena, LIB, options, &mut nodes, &mut node_map);
    let file = tsr_parser::parse_into(&arena, source, options, &mut nodes, &mut node_map);
    assert!(
        file.diagnostics.is_empty(),
        "fixture must parse: {:?}",
        file.diagnostics.iter().map(tsr_diagnostics::Diagnostic::text).collect::<Vec<_>>()
    );
    let bound = tsr_binder::bind_into(
        tsr_binder::BindResult::empty(),
        &arena,
        lib.source_file,
        &nodes,
        tsr_binder::FileInfo { name: "lib.d.ts", text: LIB },
    );
    let bound = tsr_binder::bind_into(
        bound,
        &arena,
        file.source_file,
        &nodes,
        tsr_binder::FileInfo { name: "test.ts", text: source },
    );
    let mut checker = Checker::new(&bound, &nodes, &node_map);
    let Statement::VariableStatement(statement) = file.source_file.statements[0] else {
        panic!("the fixture must start with a variable statement");
    };
    let initialiser = statement
        .declaration_list
        .and_then(|list| list.declarations.first().copied())
        .and_then(|declaration| declaration.initializer)
        .expect("an initialiser");
    let id = checker.check_expression(initialiser);
    checker.type_to_string(id)
}

#[test]
fn assignment_target_spreads_use_tuple_normalization() {
    // `checkArrayLiteral` passes a trailing array-like target to
    // `createTupleTypeEx` as a Variadic element. A fixed tuple is flattened,
    // while a generic variadic and a plain array remain deferred/rest.
    assert_eq!(
        type_of_last_array_literal(
            "let x: number; let r: [string, boolean]; [x, ...r] = null as any;"
        ),
        "[number, string, boolean]"
    );
    assert_eq!(
        type_of_last_array_literal(
            "function f<T extends unknown[]>(r: T) { let x: number; [x, ...r] = null as any; }"
        ),
        "[number, ...T]"
    );
    assert_eq!(
        type_of_last_array_literal(
            "let x: number; let r: [string?, boolean?]; [x, ...r] = null as any;"
        ),
        "[number, (string | undefined)?, (boolean | undefined)?]"
    );
    assert_eq!(
        type_of_last_array_literal("let x: number; let r: string[]; [x, ...r] = null as any;"),
        "[number, ...string[]]"
    );
    // A non-array-like destructuring rest uses its numeric index type (or
    // unknown) as a Rest element rather than treating the operand itself as a
    // variadic tuple argument.
    assert_eq!(
        type_of_last_array_literal("let x: number; let r: string; [x, ...r] = null as any;"),
        "[number, ...string[]]"
    );
    assert_eq!(
        type_of_last_array_literal("let x: number; let r: {}; [x, ...r] = null as any;"),
        "[number, ...unknown[]]"
    );
    assert_eq!(
        type_of_last_array_literal(
            "let x: number; let r: { [n: number]: string }; [x, ...r] = null as any;"
        ),
        "[number, ...string[]]"
    );
}

#[test]
fn an_empty_array_literal_is_never_and_not_a_special_case_of_nothing() {
    // `implicitNeverType` under `strictNullChecks` (`checker.go:8098`), which
    // the harness defaults on; the corpus splits 461 `never[]` to 297
    // `undefined[]` on exactly that option. The cheapest test that separates a
    // real implementation from one that only handles the non-empty path.
    assert_eq!(type_of_initialiser("const a = [];"), "never[]");
}

#[test]
fn an_empty_array_literal_is_undefined_when_strict_null_checks_is_off() {
    // The other branch of `checker.go:8098` — `undefinedWideningType`. From
    // `typedArrays.types:177` (a non-strict case): `>[] : undefined[]`. The
    // 212-line `undefined[] -> never[]` W2 row, ninth session.
    let arena = Arena::new();
    let mut nodes = NodeTable::new();
    let mut node_map = NodeMap::new();
    let options = tsr_parser::ParseOptions::default();
    let lib = tsr_parser::parse_into(&arena, LIB, options, &mut nodes, &mut node_map);
    let source = "const a = [];";
    let file = tsr_parser::parse_into(&arena, source, options, &mut nodes, &mut node_map);
    let bound = tsr_binder::bind_into(
        tsr_binder::BindResult::empty(),
        &arena,
        lib.source_file,
        &nodes,
        tsr_binder::FileInfo { name: "lib.d.ts", text: LIB },
    );
    let bound = tsr_binder::bind_into(
        bound,
        &arena,
        file.source_file,
        &nodes,
        tsr_binder::FileInfo { name: "test.ts", text: source },
    );
    let mut checker = Checker::new(&bound, &nodes, &node_map);
    checker.set_strict_null_checks(false);
    let Statement::VariableStatement(statement) = file.source_file.statements[0] else {
        panic!("the fixture must start with a variable statement");
    };
    let initialiser = statement
        .declaration_list
        .and_then(|list| list.declarations.first().copied())
        .and_then(|declaration| declaration.initializer)
        .expect("an initialiser");
    let id = checker.check_expression(initialiser);
    assert_eq!(checker.type_to_string(id), "undefined[]");
}

#[test]
fn the_element_type_is_the_union_of_the_elements() {
    // Written so the union's own ordering rule is exercised: `STRING` is
    // `1 << 5` and `NUMBER` is `1 << 6`, so a number-first source still prints
    // string-first. Upstream records `>[1, "a"] : (string | number)[]`.
    assert_eq!(type_of_initialiser(r#"const a = [1, "a"];"#), "(string | number)[]");
    assert_eq!(type_of_initialiser(r#"const a = ["a", 1];"#), "(string | number)[]");
    // One constituent after deduplication needs no parentheses.
    assert_eq!(type_of_initialiser("const a = [1, 2, 3];"), "number[]");
}

#[test]
fn elements_widen_where_a_bare_const_does_not() {
    // Freshness stops at the element boundary, exactly as it stops at the
    // property boundary: `const n = 1` is `1`, `const a = [1]` is `number[]`.
    assert_eq!(type_of_initialiser("const a = [1];"), "number[]");
    assert_eq!(type_of_initialiser(r#"const a = ["x"];"#), "string[]");
    assert_eq!(type_of_initialiser("let a = [1];"), "number[]");
}

#[test]
fn a_nested_array_literal_is_typed_by_the_same_rule() {
    assert_eq!(type_of_initialiser("const a = [[1]];"), "number[][]");
}

#[test]
fn array_literal_identity_is_separate_from_the_canonical_annotation() {
    // Native createArrayLiteralType clones the reference to carry literal flags.
    // It retains the same target and arguments as the annotated array.
    let source = "const a = [1];\nvar b: number[];";
    let arena = Arena::new();
    let mut nodes = NodeTable::new();
    let mut node_map = NodeMap::new();
    let options = tsr_parser::ParseOptions::default();
    let lib = tsr_parser::parse_into(&arena, LIB, options, &mut nodes, &mut node_map);
    let file = tsr_parser::parse_into(&arena, source, options, &mut nodes, &mut node_map);
    let bound = tsr_binder::bind_into(
        tsr_binder::BindResult::empty(),
        &arena,
        lib.source_file,
        &nodes,
        tsr_binder::FileInfo { name: "lib.d.ts", text: LIB },
    );
    let bound = tsr_binder::bind_into(
        bound,
        &arena,
        file.source_file,
        &nodes,
        tsr_binder::FileInfo { name: "test.ts", text: source },
    );
    let mut checker = Checker::new(&bound, &nodes, &node_map);
    let Statement::VariableStatement(first) = file.source_file.statements[0] else {
        panic!("variable statement")
    };
    let Statement::VariableStatement(second) = file.source_file.statements[1] else {
        panic!("variable statement")
    };
    let inferred = checker.check_expression(
        first
            .declaration_list
            .and_then(|l| l.declarations.first().copied())
            .and_then(|d| d.initializer)
            .expect("an initialiser"),
    );
    let written = checker.get_type_from_type_node(
        second
            .declaration_list
            .and_then(|l| l.declarations.first().copied())
            .and_then(|d| d.r#type)
            .expect("an annotation"),
    );
    assert_ne!(inferred, written, "literal flags must not mark the canonical reference");
    assert_eq!(checker.type_reference_target(inferred), checker.type_reference_target(written));
    assert_eq!(checker.type_to_string(inferred), "number[]");
    assert!(checker.is_type_assignable_to(inferred, written));
    assert!(checker.is_type_assignable_to(written, inferred));
}

#[test]
fn an_element_this_port_cannot_type_makes_the_literal_a_gap() {
    // §31 (`checker-notes-narrow.md`) made a truly-unresolved name answer
    // upstream's `any` (TS2304's errorType observable), so an unknown-name
    // element now builds `any[]` — upstream's own answer for this literal.
    // The fixture's NAME outlived its truth; an element this port cannot
    // type still gaps (the omitted-element case below keeps that pinned).
    assert_eq!(type_of_initialiser("const a = [unknownThing];"), "any[]");
    assert_eq!(type_of_initialiser("const a = [1, unknownThing];"), "any[]");
}

#[test]
fn a_spread_and_an_omitted_element_both_have_answers_now() {
    // The spread half came due (the twenty-sixth stand-in): the §6 arm
    // (`checker-notes-arrays.md`) spreads an `Array<T>` operand as `T`, so
    // `[...[1]]` is `number[]` — upstream's own answer.
    assert_eq!(type_of_initialiser("const a = [...[1]];"), "number[]");
    // And the omission half came due at §203, under a reason that was simply
    // untrue: "an omission still needs the tuple element flags". It needs
    // nothing — `checkExpressionWorker` (`checker.go:7815`) answers
    // `undefinedWideningType` for `KindOmittedExpression` in one unconditional
    // line, and the tuple element flags decide how a TUPLE prints, not what an
    // array-literal element contributes.
    assert_eq!(type_of_initialiser("const a = [1, , 2];"), "(number | undefined)[]");
}

#[test]
fn two_object_typed_elements_are_a_gap_because_subtype_reduction_is_missing() {
    // The eighteenth unported-stand-in fixture to come due: this asserted
    // `error` for the exact mechanism the ninth session built. In the one
    // provably-uncontextual position — an un-annotated variable initialiser —
    // the §9 decidability-gated reduction now runs upstream's
    // `UnionReductionSubtype` call (`checker.go:8096`,
    // `checker-notes-assign.md` §13), and two mutual-subtype object types
    // collapse to one exactly as upstream's do.
    assert_eq!(type_of_initialiser("const a = [{ a: 1 }, { a: 1 }];"), "{ a: number; }[]");
    // One object-typed element is unaffected — subtype reduction would not have
    // merged these either, so the guard is specific rather than a blanket
    // refusal of object elements.
    assert_eq!(type_of_initialiser("const a = [{ a: 1 }];"), "{ a: number; }[]");
    // Order predicted wrong, implementation right, again: `NUMBER` is `1 << 6`
    // and `OBJECT` is `1 << 20`, so the object type sorts *second*.
    assert_eq!(type_of_initialiser("const a = [{ a: 1 }, 1];"), "(number | { a: number; })[]");
}

/// §203. An elision is `undefined`, not a gap.
///
/// `checkExpressionWorker` (`checker.go:7815`) answers `undefinedWideningType`
/// for `KindOmittedExpression` — one line, no condition — so `[1, 2, ,]` is
/// `(number | undefined)[]` (`compiler/commentOnArrayElement3`). This port
/// refused, which read as caution and was a refusal to transcribe a constant:
/// fourteen whole cases, all but one of them `parserArrayLiteralExpression*`.
#[test]
fn an_elision_contributes_undefined_rather_than_gapping_the_array() {
    assert_eq!(type_of_initialiser("const a = [1, 2, ,];"), "(number | undefined)[]");
}

/// The control: an elision is not a licence to DROP the element. An array of
/// nothing but holes still has them in its element type, so "skip omitted
/// elements" — the other one-line change that passes the test above — does not
/// pass this one.
#[test]
fn an_array_of_only_elisions_is_still_an_array_of_undefined() {
    assert_eq!(type_of_initialiser("const a = [, ,];"), "undefined[]");
}

/// §887: `isSpreadIntoCallOrNew` (`checker.go:8117`), the first disjunct of
/// upstream's `inTupleContext`. A literal spread into a call keeps its positions
/// — widening to `number[]` discards exactly what the call is about to consume.
///
/// Corpus effect when this landed: `WRONG->RIGHT 59`, zero adverse,
/// `conformance/arraySpreadInCall` 37.
#[test]
fn a_literal_spread_into_a_call_is_a_tuple() {
    let source = "declare function f(a: number, b: number): void;\nf(...[1, 2]);\n";
    assert_eq!(type_of_last_array_literal(source), "[number, number]");
}

/// Upstream walks up parentheses explicitly (`WalkUpParenthesizedExpressions`),
/// so unlike the optional chain's flag a parenthesis does NOT break this.
#[test]
fn a_parenthesis_does_not_break_the_spread_context() {
    let source = "declare function f(a: number, b: number): void;\nf(...([1, 2]));\n";
    assert_eq!(type_of_last_array_literal(source), "[number, number]");
}

/// The control: the SAME literal passed without a spread is not in tuple context
/// from this arm, and keeps whatever the other seven arms decide.
#[test]
fn a_literal_passed_without_a_spread_is_unaffected() {
    let source = "declare function f(a: number[]): void;\nf([1, 2]);\n";
    assert_eq!(type_of_last_array_literal(source), "number[]");
}

/// A spread into something that is neither a call nor a `new` — another array
/// literal — is not tuple context. The fixture holds two literals and the helper
/// types one of them; the assertion is deliberately the answer that must hold for
/// BOTH, since neither the inner `[1, 2]` nor the outer may become a tuple here.
#[test]
fn a_spread_into_an_array_literal_is_not_tuple_context() {
    let source = "const outer = [...[1, 2]];\n";
    assert_eq!(type_of_last_array_literal(source), "number[]");
}

/// §888: upstream's second `inTupleContext` disjunct (`checker.go:8029`) — *is
/// the contextual type tuple-like?* — asked once through `get_contextual_type`
/// rather than approximated by the seven hand-rolled parent-kind arms.
///
/// A `satisfies` expression is one of the eight parent kinds the generic road
/// covers and the hand-rolled list does not.
///
/// Corpus effect when this landed: `WRONG->RIGHT 60` against one `WRONG->GAP`.
#[test]
fn a_satisfies_annotation_supplies_tuple_context() {
    let source = "const a = [1, 2] satisfies [number, number];\n";
    assert_eq!(type_of_last_array_literal(source), "[number, number]");
}

/// A PROPERTY ASSIGNMENT inside an annotated object literal — another parent
/// kind reached only through the generic road.
#[test]
fn a_property_assignment_supplies_tuple_context() {
    let source = "const o: { t: [number, string] } = { t: [1, \"a\"] };\n";
    assert_eq!(type_of_last_array_literal(source), "[number, string]");
}

/// The control: with no tuple anywhere in the contextual type, the literal still
/// widens. The generic arm must not make every literal a tuple.
#[test]
fn a_non_tuple_context_still_widens() {
    let source = "const o: { t: number[] } = { t: [1, 2] };\n";
    assert_eq!(type_of_last_array_literal(source), "number[]");
}

/// §889: upstream's predicate is applied through `someType`, which maps over a
/// UNION's constituents. `[number, string] | undefined` is a tuple context —
/// and that is exactly the shape §885 mints for an optional member.
///
/// Corpus effect: `WRONG->RIGHT 3`, zero adverse.
#[test]
fn a_union_containing_a_tuple_is_tuple_context() {
    let source = "const o: { t?: [number, string] } = { t: [1, \"a\"] };\n";
    assert_eq!(type_of_last_array_literal(source), "[number, string]");
}

/// The control: a union with no tuple constituent is not tuple context.
#[test]
fn a_union_without_a_tuple_still_widens() {
    let source = "const o: { t: number[] | undefined } = { t: [1, 2] };\n";
    assert_eq!(type_of_last_array_literal(source), "number[]");
}

/// §921: the `Array<T>`/tuple numeric road, reached from the branch that needs
/// it.
///
/// `array_or_tuple_element_access` sits at the TAIL of `element_access_lookup`,
/// after `let Some(name) = … else { … }`, so it was reachable only when the index
/// NAMES A PROPERTY. A plain `number` index names none — which is exactly the
/// case the road exists for — so `a[i]` on a `string[]` returned `error` and
/// never got there.
///
/// **`a[0]` worked and hid it**: a numeric LITERAL does name a property and
/// answers through the members table above, so the road looked live.
///
/// Corpus effect: `WRONG->RIGHT 14`, zero adverse (`indexerWithTuple` 12).
#[test]
fn a_number_typed_index_reads_an_arrays_element() {
    assert_eq!(
        type_of_last_array_literal(
            "declare const i: number;\ndeclare const a: string[];\nconst r = [a[i]];"
        ),
        "string[]"
    );
}

/// The literal-index form, which always worked and is what masked the gap.
#[test]
fn a_literal_index_still_reads_through_the_members_road() {
    assert_eq!(
        type_of_last_array_literal("declare const a: string[];\nconst r = [a[0]];"),
        "string[]"
    );
}

/// §923: a member inherited through a GENERIC heritage entry needs TWO
/// substitutions composed.
///
/// `generic_heritage_member` instantiated the inherited member for `base_type`
/// — for `interface D<T> extends C<T>` that maps `C`'s `U := T`, which leaves
/// `T`. The REFERENCE's own arguments (`D<string>`) are a separate map and
/// nothing applied them, so `d.m` answered `(x: T) => T`.
///
/// **`extends C<string>` worked and hid it**: a concrete heritage argument needs
/// no second step, so the road looked complete.
///
/// Corpus effect: 37 `WRONG->RIGHT`, 3 `GAP->RIGHT`, zero adverse —
/// `builtinIterator` 10, `genericClasses3` 6, `genericTypeWithMultipleBases3` 6.
#[test]
fn an_inherited_generic_member_takes_the_references_arguments() {
    // The heritage argument is the derived's OWN parameter — the failing shape.
    assert_eq!(
        type_of_last_array_literal(
            "interface C<T> { m(x: T): T }\n             interface D<T> extends C<T> {}\n             declare const d: D<string>;\nconst r = [d.m];"
        ),
        "((x: string) => string)[]"
    );
    // Distinct parameter names: the same defect, so this is not name shadowing.
    assert_eq!(
        type_of_last_array_literal(
            "interface C<U> { m(x: U): U }\n             interface D<T> extends C<T> {}\n             declare const d: D<string>;\nconst r = [d.m];"
        ),
        "((x: string) => string)[]"
    );
    // Classes take the same road.
    assert_eq!(
        type_of_last_array_literal(
            "class C<U> { m(x: U): U { return x; } }\n             class D<T> extends C<T> {}\n             declare const d: D<string>;\nconst r = [d.m];"
        ),
        "((x: string) => string)[]"
    );
}

/// The CONCRETE heritage argument, which always worked and is what masked it.
#[test]
fn a_concrete_heritage_argument_is_unchanged() {
    assert_eq!(
        type_of_last_array_literal(
            "interface C<U> { m(x: U): U }\n             interface D extends C<string> {}\n             declare const d: D;\nconst r = [d.m];"
        ),
        "((x: string) => string)[]"
    );
}
