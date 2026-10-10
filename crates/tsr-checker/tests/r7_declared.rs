//! r7-declared's ports (`docs/parity/notes/r7-declared.md`). Every
//! expectation is native tsgo's print at the pinned submodule.

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

/// §2: getConditionalType's checkTuples (checker.go:24310): with a simple
/// tuple check and extends of the same length, a tuple whose element is
/// generic defers (isDeferredType, checker.go:24475), so the member reads
/// as the alias reference (tsgo: `q : Both<T, string>`).
#[test]
fn a_simple_tuple_check_with_a_generic_element_defers() {
    let source = "type Both<A, B> = [A, B] extends [string, string] ? 1 : 0;\n\
         type Box<T> = { v: Both<T, string> };\n\
         function f<T>(b: Box<T>) { const q = b.v; }\n";
    assert_eq!(type_of_last_initializer(source), "Both<T, string>");
}

/// §2: a generic mapped check type is generic (getGenericObjectFlags,
/// checker.go:24895), so the conditional defers with its alias (tsgo:
/// `q : IsP<T>`).
#[test]
fn a_generic_mapped_check_defers() {
    let source = "type IsP<T> = Partial<T> extends { a: 1 } ? 1 : 0;\n\
         type Box<T> = { v: IsP<T> };\n\
         function f<T>(b: Box<T>) { const q = b.v; }\n";
    assert_eq!(type_of_last_initializer(source), "IsP<T>");
}

/// §3: a check type that mentions only the type parameter its own signature
/// binds is not generic (getGenericObjectFlags), so getConditionalType relates
/// its permissive and restrictive instantiations and takes the false branch
/// (tsgo: `r : { isAny: <T>(obj: any) => obj is T; }`).
#[test]
fn a_signature_bound_parameter_does_not_defer_the_check() {
    let source = "type Ex<T, U> = T extends U ? never : T;\n\
         type Outer<P> = { v: Ex<P, null> };\n\
         declare const o: Outer<{ isAny: <T>(obj: any) => obj is T }>;\n\
         const r = o.v;\n";
    assert_eq!(type_of_last_initializer(source), "{ isAny: <T>(obj: any) => obj is T; }");
}

/// §3: a non-generic check that mentions a type parameter is decided when
/// even its permissive instantiation is unrelated (checker.go:24377): `{ a:
/// T }` against `{ b: string }` is the false branch whatever `T` is (tsgo:
/// `q : 0`).
#[test]
fn a_definitely_false_check_mentioning_a_parameter_takes_the_false_branch() {
    let source = "type Has<T> = { a: T } extends { b: string } ? 1 : 0;\n\
         type Box<T> = { v: Has<T> };\n\
         function f<T>(b: Box<T>) { const q = b.v; }\n";
    assert_eq!(type_of_last_initializer(source), "0");
}
