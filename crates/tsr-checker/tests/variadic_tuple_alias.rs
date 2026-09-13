//! A generic alias whose body is a variadic tuple NORMALISES at instantiation.
//! §791.
//!
//! `type TV0<T extends unknown[]> = [string, ...T]` instantiated as
//! `TV0<[boolean]>` is `[string, boolean]` upstream — the rest element is
//! SPLICED, not substituted. This port printed the alias reference.
//!
//! # Why the obvious route is the wrong one
//!
//! `instantiate_type`'s Arm 6 (`inference.rs`) substitutes a tuple
//! element-wise through `tuple_element_lists` and re-mints with
//! `create_tuple_type`. A §40 print-only variadic has **no** element-list
//! entry — that is exactly what "print-only" means — so Arm 6 answers
//! `errorType`, and teaching it to splice would mean adding per-element
//! rest-ness to the tuple representation, a type-model change.
//!
//! None of that is needed. §40's *structural* road already splices a rest over
//! a concrete tuple (it is how `excessivelyLargeTupleSpread` works); all it
//! lacked was the type parameter being concrete. So this binds the alias's type
//! parameters to the arguments using §91's own `alias_evaluation_bindings`
//! frame and **re-resolves the recorded node**. Nothing substitutes anything;
//! the existing splice runs.
//!
//! STATUS §5's §790 entry refused this as a subsystem and named the
//! prerequisite wrongly twice before a trace inside the branch settled it. The
//! entry is corrected there; the short version is that *a silent fall-through
//! and an unreached branch look identical from outside*.
//!
//! # What it is worth
//!
//! **+2 corpus lines, zero adverse** — and that number is the point. §790
//! predicted ~780 lines behind this subsystem. Those lines are **not** in
//! annotation positions, which is all this arm reaches; they are in EXPRESSION
//! positions (array literals with spreads, `as const`, `bind`-style
//! signatures). The estimate is corrected in STATUS.

use tsr_ast::Statement;
use tsr_checker::Checker;
use tsr_core::Arena;

/// The printed type of the first variable declaration's annotation.
fn type_of_annotation(source: &str) -> String {
    let arena = Arena::new();
    let parsed = tsr_parser::parse(&arena, source);
    assert!(parsed.diagnostics.is_empty(), "fixture must parse");
    let bound = tsr_binder::bind(
        &arena,
        parsed.source_file,
        &parsed.nodes,
        tsr_binder::FileInfo { name: "t.ts", text: source },
    );
    let mut checker = Checker::new(&bound, &parsed.nodes, &parsed.node_map);
    for statement in parsed.source_file.statements {
        let Statement::VariableStatement(node) = statement else { continue };
        let Some(declaration) =
            node.declaration_list.and_then(|list| list.declarations.first().copied())
        else {
            continue;
        };
        let annotation = declaration.r#type.expect("an annotation");
        let id = checker.get_type_from_type_node(annotation);
        return checker.type_to_string(id);
    }
    panic!("the fixture must declare a variable");
}

/// `variadicTuples1`'s own aliases.
const ALIASES: &str = "interface Array<T> { length: number }\n\
                       type TV0<T extends unknown[]> = [string, ...T];\n\
                       type TV1<T extends unknown[]> = [string, ...T, number];\n";

/// The head case, from `variadicTuples1`.
#[test]
fn a_leading_rest_splices() {
    assert_eq!(
        type_of_annotation(&format!("{ALIASES}declare let a: TV0<[boolean]>;")),
        "[string, boolean]"
    );
}

/// Several elements splice in order.
#[test]
fn a_multi_element_argument_splices_in_order() {
    assert_eq!(
        type_of_annotation(&format!("{ALIASES}declare let a: TV0<[boolean, number]>;")),
        "[string, boolean, number]"
    );
}

/// A rest in the MIDDLE keeps the elements on both sides — this is the shape
/// element-wise substitution could never produce.
#[test]
fn a_middle_rest_keeps_both_sides() {
    assert_eq!(
        type_of_annotation(&format!("{ALIASES}declare let a: TV1<[boolean, string]>;")),
        "[string, boolean, string, number]"
    );
}

/// The EMPTY tuple argument collapses the rest away entirely — `TN2` in the
/// fixture.
#[test]
fn an_empty_tuple_argument_collapses_the_rest() {
    assert_eq!(
        type_of_annotation(&format!("{ALIASES}declare let a: TV1<[]>;")),
        "[string, number]"
    );
}

/// The decline, and the reason the arm is safe: an argument that is not a
/// concrete tuple leaves the alias reference standing rather than inventing a
/// normalisation. Upstream answers `[string, ...string[]]` here, so this stays
/// an honest gap.
#[test]
fn a_non_tuple_argument_declines_rather_than_guessing() {
    assert_eq!(
        type_of_annotation(&format!("{ALIASES}declare let a: TV0<string[]>;")),
        "TV0<string[]>"
    );
}
