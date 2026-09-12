//! A deferred indexed access keeps a TOP-LEVEL alias's name and expands a
//! LOCALLY SCOPED one. §806.
//!
//! ```ts
//! type RecordMap = { n: number, s: string };
//! declare function f<K extends keyof RecordMap>(x: RecordMap[K]): void;
//! //                                               ^ prints RecordMap[K]
//!
//! function outer() {
//!     type ArgMap = { sum: [number] };
//!     declare function g<K extends keyof ArgMap>(x: ArgMap[K]): void;
//!     //                                            ^ prints { sum: [number]; }[K]
//! }
//! ```
//!
//! # §34's rule was right about its case and wrong as a rule
//!
//! `declared.rs`'s `IndexedAccessTypeNode` arm declined an ALIAS object whole,
//! on the reasoning that *"a TYPE ALIAS object EXPANDS in upstream's deferred
//! print (`ArgMap[P]` wants `{ sum: …; concat: … }[P]`)"*. That is true of
//! `ArgMap` — and `compiler/correlatedUnions` contains BOTH, in one file, both
//! spelled `type X = { … }`:
//!
//! | alias | declared | oracle prints |
//! |---|---|---|
//! | `RecordMap` | top level | `RecordMap[P]` |
//! | `ArgMap` | inside a function body | `{ sum: …; concat: … }[P]` |
//!
//! The difference is **where they are declared**, not that either is an alias.
//! Upstream prints a name it can REACH from the site and expands one it cannot
//! — `isTypeAccessible`, the node builder's symbol-table walk.
//!
//! Approximated syntactically here: an alias whose declaration has a function
//! or block ancestor is not nameable from an arbitrary site. Narrower than
//! upstream's walk, and it errs toward §34's behaviour, which was the measured
//! one.
//!
//! Lifting the decline WITHOUT the scope gate measured +60 against **6
//! GAP→WRONG**, all of them `ArgMap`. With it: **+56, zero adverse.**

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

/// A TOP-LEVEL alias keeps its name — `correlatedUnions`' `RecordMap`.
#[test]
fn a_top_level_alias_object_keeps_its_name() {
    let source = "type RecordMap = { n: number; s: string };\n\
                  declare let a: <K extends keyof RecordMap>(x: RecordMap[K]) => void;";
    assert!(
        type_of_annotation(source).contains("RecordMap[K]"),
        "expected the alias name to survive"
    );
}

/// A non-alias object was never in question and must not change.
#[test]
fn an_interface_object_keeps_its_name() {
    let source = "interface RecordMap { n: number; s: string }\n\
                  declare let a: <K extends keyof RecordMap>(x: RecordMap[K]) => void;";
    assert!(type_of_annotation(source).contains("RecordMap[K]"));
}

/// A CONCRETE index must not take the deferred road — §625's asymmetry, which
/// this arm must not disturb. It either resolves or gaps; what it must never do
/// is print `RecordMap["n"]`, which is what an ungated deferral would produce.
///
/// (In this lib-less harness it gaps, for reasons upstream of this arm — §625's
/// concrete resolution needs machinery the harness does not stand up. The
/// assertion is on the deferral, which is what §806 changed.)
#[test]
fn a_concrete_index_does_not_take_the_deferred_road() {
    let source = "type RecordMap = { n: number; s: string };\ndeclare let a: RecordMap[\"n\"];";
    let answer = type_of_annotation(source);
    assert!(!answer.contains('['), "a concrete index must not defer: {answer}");
}
