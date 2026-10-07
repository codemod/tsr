//! A type alias whose FUNCTION body does not compute still prints its name.
//! §811.
//!
//! ```ts
//! type F6 = ({ a: string }) => typeof string;
//! //  ^ records `>F6 : F6` — the declaration line prints the alias name
//! ```
//!
//! A type alias's own declaration line prints its name whatever the body is:
//! upstream attaches the alias symbol to the declared type and the node builder
//! writes the name. This port returned the body's `errorType`, so an
//! uncomputable body took the declaration down with it —
//! `type F1 = { a: string }` answers `F1` and `type F2 = (a: string) => void`
//! answers `F2`, but `F6` gapped.
//!
//! Minted as the §31/§35 pattern: a named type carrying the alias's name,
//! registered in `unresolved_types`, so every consumer that asks *"is this a
//! gap?"* still gets yes and only the PRINTED line changes.
//!
//! # Why FUNCTION and CONSTRUCTOR bodies only
//!
//! The ungated version — every uncomputable body — measured **856 lines moved
//! in the two directions combined**: 194 GAP→RIGHT and 249 WRONG→RIGHT against
//! **349 GAP→WRONG and 64 RIGHT→WRONG**. The premise that
//! `unresolved_types` would contain the mint was wrong: the alias's USE SITES
//! print the name too, so a type this port cannot compute gets a confident
//! spelling wherever the alias is referenced.
//!
//! Restricting to function and constructor bodies keeps **+44 with zero adverse
//! of any kind**. That is not a principled boundary — it is the measured one,
//! and STATUS §5's §811 entry carries both numbers so the next attempt starts
//! from them rather than from the ungated idea.

use tsr_ast::Statement;
use tsr_checker::Checker;
use tsr_core::Arena;

/// The printed declared type of the named type alias.
fn declared_alias(source: &str, name: &str) -> String {
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
        let Statement::TypeAliasDeclaration(alias) = statement else { continue };
        let Some(written) = alias.name else { continue };
        if written.text != name {
            continue;
        }
        let symbol = bound.symbol_of(alias.node_id.expect("registered")).expect("a symbol");
        let id = checker.get_declared_type_of_symbol(symbol);
        return checker.type_to_string(id);
    }
    panic!("no alias named {name}");
}

/// The head case — a parameter that looks like an annotated pattern but is a
/// RENAME, and a return that refers to the renamed binding.
#[test]
fn a_function_alias_with_an_uncomputable_body_keeps_its_name() {
    assert_eq!(declared_alias("type F6 = ({ a: string }) => typeof string;", "F6"), "F6");
}

/// The controls: a computable body was already right, and must stay so.
#[test]
fn computable_bodies_are_unchanged() {
    assert_eq!(declared_alias("type F1 = { a: string };", "F1"), "F1");
    assert_eq!(declared_alias("type F2 = (a: string) => void;", "F2"), "F2");
}

/// The restriction: a NON-function uncomputable body still gaps. Naming those
/// measured 64 RIGHT→WRONG, because the alias's use sites print the name too.
#[test]
fn a_non_function_uncomputable_body_still_gaps() {
    let answer = declared_alias("type F7 = typeof nothingDeclaredAnywhere;", "F7");
    assert_ne!(answer, "F7", "a non-function body must not be named");
}
