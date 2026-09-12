//! A tuple spread inside `as const` SPLICES. §792.
//!
//! `[1, ...t] as const` with `t: [boolean]` is `readonly [1, boolean]`
//! upstream. This port answered `errorType`: `check_const_assertion`
//! (`assertions.rs`) declined any spread outright.
//!
//! §105's comment gave the reason as *"readonly MEMBERS are slice 2, behind
//! the value-spelling carriage"*. That reason is real and does not cover this
//! case — splicing a tuple needs no member machinery, only its element list.
//! A refusal's stated reason has to be re-read against the case in hand, not
//! inherited.
//!
//! # Where this came from
//!
//! STATUS §5's §790 refused variadic tuples as a subsystem and priced them at
//! ~780 lines. §791 built the ANNOTATION half for +2 and corrected the estimate:
//! those four cases share a language feature, not a fix. Re-deriving per
//! POSITION — which is what §791's correction asked for — put the expression
//! half here, in `as const`, and it is worth **+80**.
//!
//! Restricted to operands that HAVE an element list: a spread of an array, or
//! of §40's print-only variadic, still declines, because there is nothing to
//! splice and inventing a length would be a confident wrong answer.

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

/// The head shape.
#[test]
fn a_tuple_spread_splices_under_as_const() {
    let source = format!("{LIB}declare const t: [boolean];\nconst a = [1, ...t] as const;");
    assert_eq!(type_of_initialiser(&source, "a"), "readonly [1, boolean]");
}

/// `variadicTuples1`'s `tup2` shape — spreads on both sides of plain elements.
#[test]
fn spreads_splice_in_position() {
    let source = format!(
        "{LIB}declare const t: [boolean];\ndeclare const u: [string, number];\n\
         const a = [1, ...t, 2, ...u, 3] as const;"
    );
    assert_eq!(type_of_initialiser(&source, "a"), "readonly [1, boolean, 2, string, number, 3]");
}

/// The control: no spread still works, through the same road.
#[test]
fn a_plain_const_assertion_is_unchanged() {
    let source = format!("{LIB}const a = [1, 2] as const;");
    assert_eq!(type_of_initialiser(&source, "a"), "readonly [1, 2]");
}

/// The decline. An ARRAY spread has no element list, so there is no length to
/// splice and the operand keeps its gap rather than inventing one.
#[test]
fn an_array_spread_still_declines() {
    let source = format!("{LIB}declare const t: number[];\nconst a = [1, ...t] as const;");
    assert_eq!(type_of_initialiser(&source, "a"), "error");
}
