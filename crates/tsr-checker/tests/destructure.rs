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

/// Find the binding element declaring `name` anywhere in the fixture's
/// variable statements, function declaration parameters, or `for-of` heads,
/// and return its printed symbol type.
fn type_of_binding(source: &str, name: &str) -> String {
    let arena = Arena::new();
    let parsed = tsr_parser::parse(&arena, source);
    let bound = tsr_binder::bind(
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
    // A default: needs `UnionReductionSubtype` (`checker.go:17789`).
    let default = "var { d = 1, x } = { d: 5, x: 2 };";
    assert_eq!(type_of_binding(default, "d"), "error");
    assert_eq!(type_of_binding(default, "x"), "number");
    // A rest element: needs `getRestType` (`checker.go:17792`).
    let rest = r#"var { a, ...rest } = { a: 1, b: "x" };"#;
    assert_eq!(type_of_binding(rest, "rest"), "error");
    assert_eq!(type_of_binding(rest, "a"), "number");
    // An array literal destructured by an array pattern: upstream infers the
    // *tuple* `[number, string]` through the pattern's implied contextual
    // type (`conformance/destructuringArrayBindingPatternAndAssignment1ES5`
    // records `>[1, 2, 3] : [number, number, number]`), which this port's
    // `(string | number)[]` answer would contradict on every element.
    let literal = r#"var [q] = [1, "x"];"#;
    assert_eq!(type_of_binding(literal, "q"), "error");
    // A parameter pattern with no annotation: upstream consults contextual
    // typing first (`checker.go:16735`), and `None` from the ported slice
    // cannot distinguish absent from unported.
    let contextual = "function h({ p }) { }";
    assert_eq!(type_of_binding(contextual, "p"), "error");
    // A `for-of` head: needs `checkRightHandSideOfForOf`.
    let for_of = "declare var pairs: [number, string][];
for (var [f] of pairs) { }";
    assert_eq!(type_of_binding(for_of, "f"), "error");
}
