//! A PATTERN-named parameter takes its type from the CONTEXTUAL road before
//! declining. §808.
//!
//! `parameter_of` (`signatures.rs`) builds a signature's parameters from
//! **syntax**: an annotation, or the implicit `any` that §561's gate admits
//! where it can SHOW no contextual type exists. It never asked
//! `get_contextually_typed_parameter_type`, which lives on the SYMBOL path
//! (`symbols.rs:4291`) and is where §768's IIFE arm and every other contextual
//! parameter source hang.
//!
//! So a destructured parameter of a contextually typed arrow declined: §561's
//! gate correctly refuses the implicit `any` (there IS a contextual type), and
//! the road that would have supplied the real one was never consulted.
//!
//! # How it was found
//!
//! By asking §807's question — *which road builds an inline callee's signature,
//! and why does it not go through `get_type_of_symbol` for its parameters?* —
//! and reading `parameter_of`. The answer is that the PATTERN-named road never
//! reaches a symbol at all (§429: "a PATTERN-named parameter has no symbol of
//! its own"), so the contextual road was structurally unreachable from it.
//!
//! The IDENTIFIER-named road below it does call `get_type_of_symbol` and does
//! reach the contextual road — verified by probe, `((j) => {})("build")` types
//! `j` as `string` correctly. That case still gaps for a different reason,
//! recorded in STATUS §4.-5 under §807.

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

/// A destructured parameter of an arrow under a contextual signature.
#[test]
fn a_destructured_parameter_takes_its_contextual_type() {
    let source = "const a: (arg: { x: number; y: string }) => void = ({ x, y }) => { };";
    assert_eq!(type_of_initialiser(source, "a"), "({ x, y }: { x: number; y: string; }) => void");
}

/// The control: with no contextual type, §561's gate supplies the implicit
/// `any` and this arm never runs.
#[test]
fn no_contextual_type_still_gives_the_implied_shape() {
    let source = "const a = ([x, y]) => { };";
    let answer = type_of_initialiser(source, "a");
    assert!(answer.contains("[x, y]"), "expected the implied pattern shape: {answer}");
}

/// An ANNOTATED destructured parameter is unchanged — the annotation road runs
/// first and this arm is behind it.
#[test]
fn an_annotated_pattern_parameter_is_unchanged() {
    let source = "const a = ({ x }: { x: number }) => { };";
    assert_eq!(type_of_initialiser(source, "a"), "({ x }: { x: number; }) => void");
}
