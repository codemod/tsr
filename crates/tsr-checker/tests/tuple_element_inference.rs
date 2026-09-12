//! A TUPLE target infers ELEMENT-WISE from a tuple source. §801.
//!
//! ```ts
//! declare function f<T>(x: [T, T]): T;
//! f([1, 2]);          // number
//! f([["a"], ["b"]]);  // string[]
//! ```
//!
//! A tuple is not a type REFERENCE in this port — `create_tuple_type` mints a
//! named type and records its elements in `tuple_element_lists`, with no entry
//! in `type_reference_targets`. So `infer_from_types_within`'s reference arm
//! never saw a tuple/tuple pair, no candidate was collected, and the whole call
//! answered `errorType`.
//!
//! The element-wise walk collects a candidate from every position; what it does
//! NOT do is union candidates that disagree — see
//! `differing_positions_do_not_yet_union`, a residue pinned as an assertion.
//!
//! Equal length only. A length mismatch is upstream's variadic arithmetic — a
//! rest element absorbing several positions — which this port does not have;
//! inferring positionally across a mismatch would pair the wrong source element
//! with the wrong parameter.
//!
//! # Found through a case it does not appear in
//!
//! The probe was `jsdocTemplateTag6`'s `f4<const T>(x: [T, T])` called with
//! `[[1, "x"], [2, "y"]]`, which upstream records as
//! `readonly [1, "x"] | readonly [2, "y"]`. Every literal in that expression
//! was ALREADY right in this port — the nested tuples, the elements, all of it
//! — and only the call answered `error`. Narrowing a failure to "everything
//! inside is correct, the combination is not" is what pointed at inference
//! rather than at another const-context arm.
//!
//! It then converted 13 lines across `strictOptionalProperties1`,
//! `typeInferenceWithTupleType` and `tupleTypes` — and **none** in the case it
//! was found from, whose remaining lines need the variadic arithmetic above.

use tsr_ast::Statement;
use tsr_checker::Checker;
use tsr_core::Arena;

/// The printed type of the named variable's initialiser.
fn type_of_initialiser(source: &str, name: &str) -> String {
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
        for declaration in node.declaration_list.map(|list| list.declarations).unwrap_or_default() {
            let Some(tsr_ast::BindingName::Identifier(identifier)) = declaration.name else {
                continue;
            };
            if identifier.text != name {
                continue;
            }
            let initialiser = declaration.initializer.expect("an initialiser");
            let id = checker.check_expression(initialiser);
            return checker.type_to_string(id);
        }
    }
    panic!("no declaration named {name}");
}

const LIB: &str = "interface Array<T> { length: number }\n\
                   interface ReadonlyArray<T> { length: number }\n";

/// Both positions contribute the same candidate.
#[test]
fn a_tuple_parameter_infers_from_its_elements() {
    let source = format!(
        "{LIB}declare function f<T>(x: [T, T]): T;\ndeclare const t: [number, number];\nconst a = f(t);"
    );
    assert_eq!(type_of_initialiser(&source, "a"), "number");
}

/// Positions at different types should UNION — upstream infers
/// `string | number` here. This port answers `number`.
///
/// **A residue pinned as an assertion, following §799/§800.** The element-wise
/// walk is right: both positions DO contribute candidates, which is why the
/// call resolves at all instead of gapping. What does not union them is the
/// candidate-combination step — `getCovariantInference`'s union of the
/// collected candidates, where this port takes a single one.
///
/// That is not §801's arm and is not reached by fixing it; it is the same
/// combination rule every other multi-candidate position uses. When it lands,
/// this assertion turns red, which is the point of writing it down as a test
/// rather than as a sentence.
#[test]
fn differing_positions_do_not_yet_union() {
    let source = format!(
        "{LIB}declare function f<T>(x: [T, T]): T;\n\
         declare const t: [number, string];\nconst a = f(t);"
    );
    assert_eq!(type_of_initialiser(&source, "a"), "number");
}

/// A nested tuple element infers as a whole.
#[test]
fn a_nested_tuple_element_infers_whole() {
    let source = format!(
        "{LIB}declare function f<T>(x: [T, T]): T;\n\
         declare const t: [[number, string], [number, string]];\nconst a = f(t);"
    );
    assert_eq!(type_of_initialiser(&source, "a"), "[number, string]");
}

/// The decline: a length mismatch needs variadic arithmetic this port does not
/// have, so it stays a gap rather than pairing positions that do not
/// correspond.
#[test]
fn a_length_mismatch_still_declines() {
    let source = format!(
        "{LIB}declare function f<T>(x: [T, T]): T;\n\
         declare const t: [number, number, number];\nconst a = f(t);"
    );
    assert_ne!(type_of_initialiser(&source, "a"), "number");
}
