//! `new C(…)` where `C` is typed by a CONSTRUCTOR TYPE NODE. §805.
//!
//! ```ts
//! declare var C: new (tag: string) => L;
//! new C("x");   // L
//! ```
//!
//! answered `errorType`. The `new` road reaches its callee's symbol and
//! requires `SymbolFlags::CLASS`; a variable annotated with a constructor type
//! carries the construct signature on the **type** — in `signature_types`,
//! where `function_types.rs` puts it — and has no class symbol at all, so the
//! gate declined it whole.
//!
//! # The third spelling
//!
//! Two of the three ways to write a constructible value were already ported:
//!
//! - a **class** (`class C {}`) — the road below this arm;
//! - a **constructor interface** (`DateConstructor`, whose construct signatures
//!   live in its members) — the road above it, `bd tsr-4sa`.
//!
//! The third — a **variable annotated with a constructor type** — had no arm.
//! `conformance/localesObjectArgument` is 75 GAP lines of exactly that:
//! `new Intl.Locale("en-US")`, where `Intl.Locale` is a variable typed
//! `new (tag: …) => Intl.Locale`.
//!
//! **Ranking the GAP lines rather than the wrong ones is what surfaced it.**
//! Every board in STATUS §4.-5 ranks wrong lines; these were honest gaps, so
//! none of them showed this at all.
//!
//! The shared constructor resolver now selects overloads and infers generic
//! constructor types as well; the former generic refusal below is reopened.

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

/// The head shape.
#[test]
fn a_constructor_typed_variable_constructs() {
    let source = "interface L { x: number }\n\
                  declare var C: new (tag: string) => L;\n\
                  const a = new C(\"x\");";
    assert_eq!(type_of_initialiser(source, "a"), "L");
}

/// `localesObjectArgument`'s own shape — the constructor variable inside a
/// namespace, reached as `N.Locale`.
#[test]
fn a_namespaced_constructor_variable_constructs() {
    let source = "declare namespace N {\n\
                    interface L { x: number }\n\
                    var Locale: new (tag: string) => L;\n\
                  }\n\
                  const a = new N.Locale(\"x\");";
    assert_eq!(type_of_initialiser(source, "a"), "L");
}

/// Optional and union parameters do not change the answer — the return is what
/// this road supplies, and the arguments are upstream's diagnostics lane.
#[test]
fn parameter_shape_does_not_affect_the_answer() {
    let source = "declare namespace N {\n\
                    interface L { x: number }\n\
                    var Locale: new (tag: string | L, opts?: number) => L;\n\
                  }\n\
                  const a = new N.Locale(\"x\");";
    assert_eq!(type_of_initialiser(source, "a"), "L");
}

/// Native resolveNewExpression uses generic inference for constructor type nodes.
#[test]
fn a_generic_constructor_type_infers_its_return() {
    let source = "interface L<T> { x: T }\n\
                  declare var C: new <T>(tag: T) => L<T>;\n\
                  const a = new C(\"x\");";
    assert_eq!(type_of_initialiser(source, "a"), "L<string>");
}
