//! A parameter spelled `[...T]` puts its argument in TUPLE context and infers
//! `T` from it. §793.
//!
//! ```ts
//! declare function f<T extends unknown[]>(t: [...T]): T;
//! f([1, 2]);   // T := [number, number]
//! ```
//!
//! This port answered `errorType`. Two independent gaps had to close together,
//! and **each one alone measured ZERO corpus transitions** — which is the whole
//! reason this is one section and not two:
//!
//! 1. **Inference.** `[...T]` is built as a §40 PRINT-ONLY variadic: a named
//!    type with no element list and no reference target, so every arm of
//!    `infer_from_types_within` missed it and `T` collected no candidate.
//! 2. **Contextual typing.** Every arm of `array_literal_tuple_context_kind`
//!    reads an ANNOTATION — an assertion, an annotated declaration, an
//!    assignment target. A CALL ARGUMENT has none of those, so the literal
//!    stayed `number[]` however the parameter was spelled, and inferring from
//!    it gave `number[]` where upstream gives `[number, number]`.
//!
//! Fixing (1) alone made the call answer `number[]` instead of a gap — a
//! confident wrong answer replacing an honest one — and the corpus said so by
//! not moving. Fixing both is +22.
//!
//! # What is admitted, and what is not
//!
//! Only a SINGLE rest over a name resolving to one of this inference's own
//! parameters. `[...T]` is upstream's idiom for *"this parameter is the whole
//! tuple"*. Anything else — `[string, ...T]`, two rests — needs the source
//! SPLIT across positions, which is tuple-splitting machinery this port does
//! not have, and those keep declining.

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

/// The head case. Both halves are needed: without the contextual half this
/// answers `number[]`, without the inference half it answers `error`.
#[test]
fn a_variadic_parameter_infers_the_argument_as_a_tuple() {
    let source = format!(
        "{LIB}declare function f<T extends unknown[]>(t: [...T]): T;\nconst a = f([1, 2]);"
    );
    assert_eq!(type_of_initialiser(&source, "a"), "[number, number]");
}

/// Mixed element types keep their positions, which is what distinguishes a
/// tuple answer from the widened array.
#[test]
fn the_inferred_tuple_keeps_element_positions() {
    let source = format!(
        "{LIB}declare function f<T extends unknown[]>(t: [...T]): T;\n\
         const a = f([1, \"x\", true]);"
    );
    assert_eq!(type_of_initialiser(&source, "a"), "[number, string, boolean]");
}

/// A concrete tuple argument worked before this and must keep working — it
/// reaches `T` through the ordinary reference road, not through either half.
#[test]
fn a_concrete_tuple_argument_still_infers() {
    let source = format!(
        "{LIB}declare const t: [number, string];\n\
         declare function f<T extends unknown[]>(x: T): T;\nconst a = f(t);"
    );
    assert_eq!(type_of_initialiser(&source, "a"), "[number, string]");
}

/// The plain array parameter is untouched — `T[]` infers the ELEMENT, and
/// nothing here may change that.
#[test]
fn a_plain_array_parameter_still_infers_the_element() {
    let source = format!("{LIB}declare function f<T>(t: T[]): T;\nconst a = f([1, 2]);");
    assert_eq!(type_of_initialiser(&source, "a"), "number");
}

/// The decline: a variadic with a LEADING element needs the source split
/// across positions, which this port cannot do, so it stays a gap rather than
/// guessing which elements belong to `T`.
#[test]
fn a_variadic_with_a_leading_element_still_declines() {
    let source = format!(
        "{LIB}declare function f<T extends unknown[]>(t: [string, ...T]): T;\n\
         const a = f([\"s\", 1]);"
    );
    assert_ne!(type_of_initialiser(&source, "a"), "[number]");
}
