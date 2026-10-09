//! r6-declared2's ports (`docs/parity/notes/r6-declared2.md`). Every
//! expectation was checked against a native tsgo built from the pinned
//! submodule (`scripts/offline-cargo/build-tsgo.sh`).

use tsr_ast::Statement;
use tsr_checker::Checker;
use tsr_core::Arena;

/// The printed type of the initializer of the last `const` in `source`,
/// descending into function bodies so a fixture can introduce type
/// parameters.
fn type_of_last_initializer(source: &str) -> String {
    let arena = Arena::new();
    let parsed = tsr_parser::parse(&arena, source);
    assert!(parsed.diagnostics.is_empty(), "fixture must parse");
    let bound = tsr_binder::bind(
        &arena,
        parsed.source_file,
        &parsed.nodes,
        tsr_binder::FileInfo { name: "test.ts", text: source },
    );
    let mut checker = Checker::new(&bound, &parsed.nodes, &parsed.node_map);
    let options = tsr_core::CompilerOptions {
        strict_null_checks: tsr_core::Tristate::True,
        ..Default::default()
    };
    checker.apply_compiler_options(&options);
    let last = last_initializer(parsed.source_file.statements)
        .expect("the fixture must contain a const with an initializer");
    let id = checker.check_expression(last);
    checker.type_to_string(id)
}

fn last_initializer<'a>(statements: &[Statement<'a>]) -> Option<tsr_ast::Expression<'a>> {
    let mut found = None;
    for statement in statements {
        match statement {
            Statement::VariableStatement(node) => {
                if let Some(list) = node.declaration_list {
                    for declaration in list.declarations {
                        found = declaration.initializer.or(found);
                    }
                }
            }
            Statement::FunctionDeclaration(node) => {
                if let Some(tsr_ast::FunctionBody::Block(block)) = node.body {
                    found = last_initializer(block.statements).or(found);
                }
            }
            _ => {}
        }
    }
    found
}

/// §1: a conditional alias referenced in an alias body with a generic check
/// type is deferred (getConditionalType, checker.go:24339) and keeps its
/// alias, so the member reads as `Ex<T, null>` (tsgo: `f<T>(b: Box<T>):
/// Ex<T, null>`). The base answered errorType for the member.
#[test]
fn a_deferred_conditional_alias_in_an_alias_body_keeps_its_alias() {
    let source = "type Ex<T, U> = T extends U ? never : T;\n\
         type Box<T> = { v: Ex<T, null> };\n\
         function f<T>(b: Box<T>) { const r = b.v; }\n";
    assert_eq!(type_of_last_initializer(source), "Ex<T, null>");
}

/// §1: a decided check whose evaluation this port declines keeps the gap
/// rather than printing the alias: only a generic check or extends type
/// defers.
#[test]
fn a_concrete_check_does_not_defer() {
    let source = "type Ex<T, U> = T extends U ? never : T;\n\
         type Box = { v: Ex<string, null> };\n\
         declare const b: Box;\n\
         const r = b.v;\n";
    assert_eq!(type_of_last_initializer(source), "string");
}
