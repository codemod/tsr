//! The pattern-implied contextual type — an array literal destructured by an
//! array pattern implies a **tuple** (`bd tsr-84iz`,
//! `docs/architecture/checker-notes-patctx.md`).
//!
//! `conformance/destructuringArrayBindingPatternAndAssignment1ES5.types`
//! records `>[1, 2, 3] : [number, number, number]` — the literal itself is a
//! tuple under the pattern's contextual type, which is why each element keeps
//! its own type instead of collapsing into a union.

use tsr_ast::{BindingName, Statement};
use tsr_checker::Checker;
use tsr_core::Arena;

/// The printed type of the binding element named `name`.
fn type_of_binding(source: &str, name: &str) -> String {
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
    let mut found = None;
    for statement in parsed.source_file.statements {
        let Statement::VariableStatement(node) = statement else { continue };
        for declaration in node.declaration_list.map(|l| l.declarations).unwrap_or_default() {
            if let Some(BindingName::BindingPattern(pattern)) = declaration.name {
                for element in pattern.elements {
                    if matches!(element.name, Some(BindingName::Identifier(id)) if id.text == name)
                    {
                        found = element.node_id;
                    }
                }
            }
        }
    }
    let id = found.unwrap_or_else(|| panic!("the fixture must destructure `{name}`"));
    let symbol = bound.symbol_of(id).expect("bound");
    let ty = checker.get_type_of_symbol(symbol);
    checker.type_to_string(ty)
}

/// The rule: each element keeps its own widened type, where an
/// uncontextualised array literal would collapse them into a union.
#[test]
fn each_element_keeps_its_own_type() {
    let source = r#"var [a, b] = [1, "x"];"#;
    assert_eq!(type_of_binding(source, "a"), "number");
    assert_eq!(type_of_binding(source, "b"), "string");
}

/// Widened, not literal: `conformance/destructuringArrayBindingPatternAndAssignment1ES5`
/// records `>[1, 2, 3] : [number, number, number]` for a non-`const`
/// declaration — the elements are `number`, not `1`.
#[test]
fn the_elements_widen() {
    let source = "var [a, b, c] = [1, 2, 3];";
    assert_eq!(type_of_binding(source, "a"), "number");
    assert_eq!(type_of_binding(source, "c"), "number");
}

/// The refused shapes, each beside a ported control, so the pair keeps
/// discriminating. Every one is a form `examples/patctx.rs` measured this arm
/// mispredicting.
#[test]
fn the_refused_shapes_stay_gaps() {
    // A pattern longer than the literal reads out of range, where upstream's
    // element is optional and prints `T | undefined`.
    let long = r#"var [a, b, c] = [1, "x"];"#;
    assert_eq!(type_of_binding(long, "c"), "error");
    let exact = r#"var [d, e] = [1, "x"];"#;
    assert_eq!(type_of_binding(exact, "d"), "number");

    // A spread in the literal needs `sliceTupleType`.
    let spread = "declare var rest: number[];\nvar [a, b] = [1, ...rest];";
    assert_eq!(type_of_binding(spread, "a"), "error");

    // A rest in the pattern, likewise.
    let rest_pattern = r#"var [a, ...rest] = [1, "x"];"#;
    assert_eq!(type_of_binding(rest_pattern, "a"), "error");

    // A literal element that itself gaps gaps the whole tuple. The gapping
    // element is a **template expression**, which `examples/tmplgap.rs`
    // measured as unported this session and which `checker-notes-tmplexpr.md`
    // refuses with a ratio — so this fixture asserts a *pair* whose other half
    // is a live refusal, not a guess.
    //
    // The first spelling tried here was `(undefined as Unresolved)`, on the
    // intuition that an unresolved type reference gaps. It does **not**: since
    // `bd tsr-eep` such a reference mints a type that prints the written name,
    // and `a` read `number`. That is the sixth expectation this project has
    // written from intuition and had corrected by the code, and the third in
    // the pessimistic direction.
    // The stand-in came due (the twenty-fifth): the template expression
    // landed (`checker-notes-narrow.md` §24) and `` `x${1}` `` FOLDS to the
    // fresh `"x1"`, so the tuple types and `a` reads its widened `number` —
    // upstream's own answer for this destructuring.
    let gapped = "var [a, b] = [1, `x${1}`];";
    assert_eq!(type_of_binding(gapped, "a"), "number");
}
