//! `Array` and `ReadonlyArray` are ONE reference target for inference. §787.
//!
//! `mk<T>(values: readonly T[])` called with `[0, 1, 2]` puts an
//! `Array<number>` SOURCE against a `ReadonlyArray<T>` TARGET.
//! `infer_from_types_within` compared the two reference targets by identity,
//! found them different, and returned — so `T` collected no candidate and the
//! whole call answered `errorType`, printed `any`.
//!
//! Upstream infers argument-wise here: `inferFromObjectTypes`
//! (`inference.go`) admits two references whose targets differ when both are
//! array-like, because a mutable array IS a readonly one and their single type
//! argument occupies the same slot.
//!
//! Restricted to those two globals rather than any structurally-compatible
//! pair: inference that admits a target it cannot justify produces a
//! CANDIDATE, and a wrong candidate is a confident wrong answer rather than a
//! missing one.

use tsr_ast::Statement;
use tsr_checker::Checker;
use tsr_core::Arena;

/// The printed type of the named variable's declaration.
fn type_of(source: &str, name: &str) -> String {
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
            let symbol = bound
                .symbol_of(declaration.node_id.expect("registered"))
                .expect("the declaration must be bound");
            let id = checker.get_type_of_symbol(symbol);
            return checker.type_to_string(id);
        }
    }
    panic!("no declaration named {name}");
}

/// The harness has no lib, so both array interfaces are declared in the
/// fixture — the pair is resolved through `binder.global` either way.
const LIB: &str = "interface Array<T> { length: number }\n\
                   interface ReadonlyArray<T> { length: number }\n";

/// The control: a MUTABLE array parameter has always inferred.
#[test]
fn a_mutable_array_parameter_infers() {
    let source = format!("{LIB}declare function mk<T>(values: T[]): T;\nconst a = mk([0, 1, 2]);");
    assert_eq!(type_of(&source, "a"), "number");
}

/// The arm: a READONLY array parameter must infer the same way. Reverting
/// `Checker::is_array_like_pair` reddens this and leaves `error`.
#[test]
fn a_readonly_array_parameter_infers_the_same_way() {
    let source =
        format!("{LIB}declare function mk<T>(values: readonly T[]): T;\nconst a = mk([0, 1, 2]);");
    assert_eq!(type_of(&source, "a"), "number");
}

/// The construct spelling, which is where the lib actually puts it —
/// every collection constructor takes `readonly T[]`.
#[test]
fn a_readonly_array_construct_parameter_infers() {
    let source = format!(
        "{LIB}interface S<T> {{ size: number }}\n\
         interface SCtor {{ new <T>(values: readonly T[]): S<T>; }}\n\
         declare var SCtor: SCtor;\nconst a = new SCtor([0, 1, 2]);"
    );
    assert_eq!(type_of(&source, "a"), "S<number>");
}

/// The restriction, stated as a test: two UNRELATED generic references are
/// still not inferred across. Admitting them would produce a candidate this
/// port cannot justify.
#[test]
fn two_unrelated_references_still_do_not_infer_across() {
    let source = format!(
        "{LIB}interface Box<T> {{ v: T }}\ninterface Bag<T> {{ v: T }}\n\
         declare function mk<T>(b: Box<T>): T;\ndeclare const bag: Bag<number>;\n\
         const a = mk(bag);"
    );
    assert_ne!(type_of(&source, "a"), "number");
}
