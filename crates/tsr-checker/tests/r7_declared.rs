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

/// §5: a mapped template whose conditional has a parenthesized nested
/// conditional branch is a deferred conditional native prints from its
/// parts; the member reads the instantiated template (tsgo: `r : 0`). The
/// mint had no written text, so the template was `error`.
#[test]
fn a_parenthesized_conditional_branch_in_a_mapped_template_instantiates() {
    let source = "type M<T> = { [K in keyof T]: T[K] extends number ? (T[K] extends string ? 2 : 1) : 0 };\n\
         declare const m: M<{ a: boolean }>;\n\
         const r = m.a;\n";
    assert_eq!(type_of_last_initializer(source), "0");
}

/// §7: a type-literal alias mentioned inside its own body through a union
/// alias (`N = F | B` inside `F`) is `F` itself in `N`'s constituents
/// (getTypeFromTypeLiteralOrFunctionOrConstructorTypeNode creates it once,
/// members lazy), so relating `N` reads `F`'s members (tsgo: `r : 1`). The
/// placeholder had no members table and the relation was undecidable.
#[test]
fn a_self_mentioned_type_literal_alias_has_its_members_in_the_union() {
    let source = "type F = { kind: 'foo'; children: N };\n\
         type B = { kind: 'bar' };\n\
         type N = F | B;\n\
         type R = N extends { kind: 'foo' | 'bar' } ? 1 : 0;\n\
         declare const x: R;\n\
         const r = x;\n";
    assert_eq!(type_of_last_initializer(source), "1");
}

/// §8: a union of an alias-named union and an object keeps the alias in its
/// origin (getUnionTypeWorker, checker.go:25705) instead of failing (tsgo:
/// `z : C3 | CC`).
#[test]
fn an_alias_named_union_beside_an_object_keeps_its_origin() {
    let source = "interface C1 { a: 1 } interface C2 { b: 2 } interface C3 { c: 3 }\n\
         type CC = C1 | C2;\n\
         declare const y: CC | C3;\n\
         const z = y;\n";
    assert_eq!(type_of_last_initializer(source), "C3 | CC");
}
