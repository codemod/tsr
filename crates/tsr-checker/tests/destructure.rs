//! Binding elements — the plain destructuring leg (`bd tsr-o00`,
//! `docs/architecture/checker-notes-destructure.md`).
//!
//! Every expectation below is copied from a `.types` baseline, named at each
//! assertion — five expectations across four sessions were written from
//! intuition and every one was wrong. Refused legs are asserted as *pairs*:
//! the refused form beside the ported one, so the frontier moving turns the
//! test red instead of silently widening (the tuple build's rule, its tenth
//! fixture having come due in one commit).

use tsr_ast::{BindingName, Statement};
use tsr_checker::Checker;
use tsr_core::Arena;

#[test]
fn a_distributed_generic_constraint_supplies_a_common_destructured_property() {
    let source = "type Params = { foo: string } & ({ tag: \"a\" } | { tag: \"b\" });\n\
                  function f<T extends Params>({ foo }: T) {}";
    assert_eq!(type_of_binding(source, "foo"), "string");
}

/// Find the binding element declaring `name` anywhere in the fixture's
/// variable statements, function declaration parameters, or `for-of` heads,
/// and return its printed symbol type.
fn type_of_binding(source: &str, name: &str) -> String {
    let arena = Arena::new();
    let parsed = tsr_parser::parse(&arena, source);
    let bound = tsr_binder::bind(
        &arena,
        parsed.source_file,
        &parsed.nodes,
        tsr_binder::FileInfo { name: "test.ts", text: source },
    );
    let mut checker = Checker::new(&bound, &parsed.nodes, &parsed.node_map);
    let mut found = None;
    for statement in parsed.source_file.statements {
        match statement {
            Statement::VariableStatement(node) => {
                for declaration in node.declaration_list.map(|l| l.declarations).unwrap_or_default()
                {
                    if let Some(binding) = declaration.name {
                        search_binding(binding, name, &mut found);
                    }
                }
            }
            Statement::FunctionDeclaration(function) => {
                for parameter in function.parameters {
                    if let Some(binding) = parameter.name {
                        search_binding(binding, name, &mut found);
                    }
                }
            }
            Statement::ForInOrOfStatement(for_of) => {
                if let Some(tsr_ast::ForInitializer::VariableDeclarationList(list)) =
                    for_of.initializer
                {
                    for declaration in list.declarations {
                        if let Some(binding) = declaration.name {
                            search_binding(binding, name, &mut found);
                        }
                    }
                }
            }
            // §902: a TRY statement's catch clause, so a catch-clause binding
            // pattern is reachable from this harness at all.
            Statement::TryStatement(try_statement) => {
                if let Some(clause) = try_statement.catch_clause
                    && let Some(declaration) = clause.variable_declaration
                    && let Some(binding) = declaration.name
                {
                    search_binding(binding, name, &mut found);
                }
            }
            _ => {}
        }
    }
    let id = found.unwrap_or_else(|| panic!("the fixture must destructure `{name}`"));
    let symbol = bound.symbol_of(id).expect("the binding element must be bound");
    let ty = checker.get_type_of_symbol(symbol);
    checker.type_to_string(ty)
}

fn search_binding(binding: BindingName<'_>, target: &str, found: &mut Option<tsr_ast::NodeId>) {
    let BindingName::BindingPattern(pattern) = binding else { return };
    for element in pattern.elements {
        match element.name {
            Some(BindingName::Identifier(identifier)) if identifier.text == target => {
                *found = element.node_id;
            }
            Some(nested) => search_binding(nested, target, found),
            None => {}
        }
    }
}

/// `conformance/declarationsAndAssignments.types`:
/// `var { x, y } = { x: 5, y: "hello" };` records `>x : number`,
/// `>y : string`.
#[test]
fn an_object_pattern_reads_the_initializers_members() {
    let source = r#"var { x, y } = { x: 5, y: "hello" };"#;
    assert_eq!(type_of_binding(source, "x"), "number");
    assert_eq!(type_of_binding(source, "y"), "string");
}

/// `compiler/declarationEmitDestructuringObjectLiteralPattern.types`:
/// `var { y8: b1 } = { x8: 5, y8: "hello" };` records `>b1 : string` — the
/// explicit property name is the lookup key, the binding name is not.
#[test]
fn a_renamed_element_looks_up_the_property_name() {
    let source = r#"var { y8: b1 } = { x8: 5, y8: "hello" };"#;
    assert_eq!(type_of_binding(source, "b1"), "string");
}

/// `conformance/declarationsAndAssignments.types`:
/// `var [b3, b4, b5]: [number, number, string] = [1, 2, "string"];` records
/// `>b5 : string` — the element's position indexes the annotated tuple; and
/// `var [[c5], c6]: [[string|number], boolean] = [[1], true];` records
/// `>c5 : string | number` — a nested pattern recurses through its holder
/// element's slice.
#[test]
fn an_annotated_tuple_types_elements_by_position() {
    let source = r#"var [b3, b4, b5]: [number, number, string] = [1, 2, "string"];"#;
    assert_eq!(type_of_binding(source, "b5"), "string");
    assert_eq!(type_of_binding(source, "b3"), "number");
    let nested = "var [[c5], c6]: [[string | number], boolean] = [[1], true];";
    assert_eq!(type_of_binding(nested, "c5"), "string | number");
    assert_eq!(type_of_binding(nested, "c6"), "boolean");
}

/// `conformance/destructuringParameterDeclaration1ES5.types`:
/// `function a1([a, b, [[c]]]: [number, number, string[][]]) { }` records
/// `>a : number` — an annotated parameter pattern types by position. The
/// baseline's `[[c]]` level needs the global `Array` symbol, which this
/// lib-less harness does not load (`string[]` itself gaps here); the array
/// level is exercised by the corpus, whose `destructuringParameterDeclaration1ES5`
/// gained 69 lines on this build.
#[test]
fn an_annotated_parameter_pattern_types() {
    let source = "function a1([a, b]: [number, boolean]) { }";
    assert_eq!(type_of_binding(source, "a"), "number");
    assert_eq!(type_of_binding(source, "b"), "boolean");
}

/// The hole rule the parser change carries: upstream represents `[, b]`'s
/// hole as an all-nil `BindingElement` (`parser.go:1663`, "These are all nil
/// for a missing element") and indexes elements by slice position
/// (`checker.go:17750`). The position rule itself is pinned by
/// `declarationsAndAssignments`' `>b5 : string` above; this composes the two:
/// with the hole recorded, `h` sits at index 1 and reads `string`. Before the
/// parser change `h` sat at index 0 and this read `number`.
#[test]
fn a_hole_shifts_the_positions_after_it() {
    let source = r#"var [, h]: [number, string] = ["x" as any, "y"];"#;
    assert_eq!(type_of_binding(source, "h"), "string");
}

/// `compiler/defaultValueInFunctionTypes.types`:
/// `({ first = 0 }: { first?: number })` records `>first : number` — under
/// an **annotated root**, a default of a non-`undefined` type strips
/// `undefined` from the optional property's `number | undefined`
/// (`checker.go:17782`–`:17786`, the `IS_UNDEFINED` facts test). The same
/// element with an *initializer-typed* root stays refused —
/// `the_refused_legs_stay_gaps` pins that side of the pair.
#[test]
fn an_annotated_default_strips_undefined() {
    let source = "function f({ first = 0 }: { first?: number }) { }";
    assert_eq!(type_of_binding(source, "first"), "number");
}

/// The refused legs, each beside a ported positive control
/// (`checker-notes-destructure.md` §3). If one of these starts answering,
/// the leg has been built and its pair here must move to the ported side.
#[test]
fn the_refused_legs_stay_gaps() {
    // The default leg CAME DUE at §315 — `UnionReductionSubtype` exists
    // (`union_with_subtype_reduction`) and the annotation-less union runs
    // (`checker.go:17789`): the fresh `1` survives reduction against the
    // widened `number` and the mutable-root wrap widens the result —
    // `>x : number` for `var [x = 20] = [1, 2]`
    // (`sourceMapValidation…ArrayBindingPattern6`). Moved to the ported side,
    // as this test's own doc demands.
    let default = "var { d = 1, x } = { d: 5, x: 2 };";
    assert_eq!(type_of_binding(default, "d"), "number");
    assert_eq!(type_of_binding(default, "x"), "number");
    // The OBJECT rest leg CAME DUE at §319 — `getRestType`'s member
    // subtraction runs over `spread_members_of`
    // (`>rest : { b: string; }`, `conformance/objectRest`). The ARRAY rest
    // (`sliceTupleType`) is the half still refused.
    let rest = r#"var { a, ...rest } = { a: 1, b: "x" };"#;
    assert_eq!(type_of_binding(rest, "rest"), "{ b: string; }");
    assert_eq!(type_of_binding(rest, "a"), "number");
    // sliceTupleType preserves the optional flags in the copied tuple.
    // Pinned tsgo declaration output with strictNullChecks spells the
    // optional rest as [(string | undefined)?].
    let array_rest = r#"var [x, ...tail]: [number, string, string] = [1, "a", "b"];"#;
    assert_eq!(type_of_binding(array_rest, "tail"), "[string, string]");
    let optional_rest = "var [y, ...opt]: [number, string?] = [1];";
    assert_eq!(type_of_binding(optional_rest, "opt"), "[(string | undefined)?]");
    // An array literal destructured by an array pattern **is now ported**
    // (`bd tsr-84iz`): upstream infers the *tuple* `[number, string]` through
    // the pattern's implied contextual type
    // (`conformance/destructuringArrayBindingPatternAndAssignment1ES5` records
    // `>[1, 2, 3] : [number, number, number]`). This asserted `error` until
    // that landed — the fourteenth stand-in fixture in this project to come
    // due — and is now the **pair**: the ported form answers, and the shapes
    // `checker-notes-patctx.md` still refuses gap beside it. `tests/pattern_context.rs`
    // is the wider suite.
    let literal = r#"var [q] = [1, "x"];"#;
    assert_eq!(type_of_binding(literal, "q"), "number");
    // **Came due at §549** (the fifteenth stand-in in this project to do so).
    // An OUT-OF-RANGE element used to gap because `tuple_from_array_literal`
    // refuses a pattern longer than the literal and that refusal was returned
    // as `error`, poisoning the whole parent. §549 made the refusal fall
    // through to the plain initializer type, and the element now reads
    // `undefined` — which is UPSTREAM'S ANSWER, not a side effect:
    // `conformance/destructuringArrayBindingPatternAndAssignment1ES5.types`
    // records `>c2 : undefined`, `>c3 : undefined`, `>c4 : undefined` for
    // exactly this shape. The assertion is updated to the baseline rather than
    // to whatever the port now prints.
    let out_of_range = r#"var [q1, q2, q3] = [1, "x"];"#;
    assert_eq!(type_of_binding(out_of_range, "q3"), "undefined");
    // **Came due at §565.** A parameter pattern with no annotation used to gap
    // because the pattern-implied object type is minted with NO SYMBOL, so
    // `get_type_of_property_of_type` had no members table and every element
    // read `error` — the type's own printed form (`{ p: any; }`) showed a
    // member nothing could consult. §565's `pattern_implied_members` side
    // table makes it readable, and the element reads `any`, which is what
    // upstream records for an unannotated, uncontextual destructured
    // parameter (`getTypeFromObjectBindingPattern`, `checker.go:17938`, has no
    // initializer to infer from, so every implied member IS `any`).
    //
    // The old comment's reasoning — *"upstream consults contextual typing
    // first and `None` cannot distinguish absent from unported"* — was about
    // the GATE (§429/§561), not about this shape: `function h(…)` is a
    // DECLARATION and takes no contextual return, so there was never a
    // contextual type here to be confused by.
    let contextual = "function h({ p }) { }";
    assert_eq!(type_of_binding(contextual, "p"), "any");
    // A `for-of` head: needs `checkRightHandSideOfForOf`.
    let for_of = "declare var pairs: [number, string][];
for (var [f] of pairs) { }";
    assert_eq!(type_of_binding(for_of, "f"), "error");
}

/// §776: a GENERIC object rest mints `Omit<source, keys>`
/// (`checker.go:17813`-`:17827`), and the omit list includes the UNSPREADABLE
/// properties as well as the bound ones (`:17806`-`:17818`).
///
/// §775 wrote this arm with the bound names only and was refused: it took
/// `destructuringUnspreadableIntoRest` 30 RIGHT→WRONG, because an omit list
/// missing the private, protected, method and accessor members is a
/// confidently wrong type where the gap was honest. The unspreadable half is
/// what made it landable.
///
/// **Only the never-case is testable here.** The `Omit<…>` mint needs
/// `getGlobalOmitSymbol`, and this harness builds no program, so the lib is
/// absent and the arm answers `error` — the same limitation §751 recorded for
/// `let a: E.A` and §760 for `String`. §776's guard for the mint is the CORPUS
/// (+12 G→R / +10 W→R, zero RIGHT→anything).
///
/// What this DOES pin is the ORDER: upstream tests `omitKeyType.flags & Never`
/// at `:17820`, BEFORE `getGlobalOmitSymbol` at `:17823`, so a rest that omits
/// nothing answers the source even in a lib-less program. A first draft had
/// the two the other way round and answered `error` here.
#[test]
fn a_generic_object_rest_that_omits_nothing_is_the_source() {
    assert_eq!(type_of_binding("function f<T extends { a: string }>({ ...r }: T) {}", "r"), "T");
}

/// §893: a binding-pattern element with a DEFAULT is an OPTIONAL member of the
/// pattern's implied type, and its type comes from the initializer rather than
/// being `any`.
///
/// `getTypeFromObjectBindingPattern` (`checker.go:17938`):
///
/// ```go
/// flags := ast.SymbolFlagsProperty | core.IfElse(e.Initializer() != nil, ast.SymbolFlagsOptional, 0)
/// ```
///
/// §429's arm required **every** element to have no initializer — its own
/// comment says *"defaults, rests and nested patterns keep the implicit any"* —
/// so a pattern holding any default declined whole.
///
/// The initializer is read **syntactically**. Calling `check_expression` on it
/// overflows the stack: a defaulted parameter's initializer is checked with the
/// parameter's own contextual type, which is the implied type being computed.
/// Upstream is re-entrant there and this port is not.
///
/// Corpus effect: `+14 W→R`, `+5 G→R` against `6 G→W`, **zero `RIGHT→`** — the
/// safety leg exactly as the bar predicted, since the change only admits
/// patterns that previously declined.
#[test]
fn a_defaulted_object_pattern_element_takes_the_initializers_type() {
    let source = "function f({ a = \"x\", b }) { }";
    assert_eq!(type_of_binding(source, "a"), "string");
    // The element beside it keeps §429's implicit `any`.
    assert_eq!(type_of_binding(source, "b"), "any");
}

/// The widening is upstream's `widenTypeInferredFromInitializer`: `= 1` gives
/// `number`, not `1`.
#[test]
fn a_defaulted_element_widens_its_literal() {
    assert_eq!(type_of_binding("function f({ n = 1, t = true }) { }", "n"), "number");
    assert_eq!(type_of_binding("function f({ n = 1, t = true }) { }", "t"), "boolean");
}

/// An ARRAY pattern's defaulted element stays refused. Its upstream answer is
/// the element type from the initializer, and the tuple branch fills `any` for
/// every position, so admitting it there would mint a confident wrong tuple.
#[test]
fn a_defaulted_array_pattern_element_is_still_refused() {
    assert_ne!(type_of_binding("function f([a = 1]) { }", "a"), "number");
}

/// §894: a RENAMED element is admitted too. The member is named by the PROPERTY
/// name and the local binding by the other — which is exactly why §429 excluded
/// them: it read `element.name` for both.
///
/// Corpus effect: `+7 W→R`, `+3 G→R`, zero adverse.
#[test]
fn a_renamed_defaulted_element_names_the_member_by_its_property() {
    let source = "function f({ primary: p = \"none\" }) { }";
    // The LOCAL name is what a read of the binding resolves.
    assert_eq!(type_of_binding(source, "p"), "string");
}

/// §895: a NESTED object pattern's member type is that pattern's own implied
/// type — `getTypeFromBindingElement` (`checker.go:17950`) recursing into
/// `getTypeFromBindingPattern`. This port could not, because the builder lived
/// inline in `get_type_of_symbol`'s body keyed off a `ParameterDeclaration`;
/// §895 made it a method so it can call itself.
///
/// Corpus effect: `+11 W→R`, `+10 G→R`, zero adverse.
#[test]
fn a_nested_object_pattern_takes_its_own_implied_type() {
    let source = "function f({ outer: { inner = \"x\" } }) { }";
    assert_eq!(type_of_binding(source, "inner"), "string");
}

/// A nested ARRAY pattern still declines: its implied type is a tuple whose
/// element types this arm does not compute, so minting `any` positions would be
/// a confident wrong answer where the gap is honest.
#[test]
fn a_nested_array_pattern_is_still_refused() {
    assert_ne!(type_of_binding("function f({ outer: [a = 1] }) { }", "a"), "number");
}

/// §902: a CATCH CLAUSE's variable is `any` — `unknown` under
/// `useUnknownInCatchVariables` — and destructuring it gives every element that
/// type, through the `parent_type == any` short-circuit `checker.go:17709`
/// already ports.
///
/// `symbols.rs` has computed the catch variable's type since §21 and this road
/// never asked: a catch variable with a PATTERN name has no symbol of its own
/// (§429), so the symbol road that knows the answer is unreachable from here.
///
/// Corpus effect: `GAP->RIGHT 19`, zero adverse — `conformance/destructuringCatch`
/// closed entirely (16 rows).
#[test]
fn destructuring_a_catch_variable_gives_any() {
    let source = "try { } catch ([a, b]) { }";
    assert_eq!(type_of_binding(source, "a"), "any");
    assert_eq!(type_of_binding(source, "b"), "any");
}

/// The object form takes the same road.
#[test]
fn destructuring_a_catch_variable_by_property_gives_any() {
    assert_eq!(type_of_binding("try { } catch ({ p }) { }", "p"), "any");
}
