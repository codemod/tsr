//! `typeof import("m")…`, the value-meaning arm of
//! `getTypeFromImportTypeNode` (`checker.go:24575`). An ambient
//! `declare module "m"` resolves without a module host, so one bound file
//! exercises the qualifier walk and the instantiation. Every expectation was
//! read from the pinned `tsgo` first (assign the variable to `never`).
//! `docs/parity/notes/r6-typesroots2.md` §3.

use tsr_ast::Statement;
use tsr_checker::Checker;
use tsr_core::Arena;

const MODULE: &str = "declare module \"m\" {
    export class A { static foo(): void; }
    export function f<T>(x: T): T;
    export const n: number;
    export interface I { a: string }
}
";

/// The printed type of the named variable declaration.
fn type_of_declaration(source: &str, name: &str) -> String {
    let arena = Arena::new();
    let parsed = tsr_parser::parse(&arena, source);
    assert!(
        parsed.diagnostics.is_empty(),
        "fixture must parse: {:?}",
        parsed.diagnostics.iter().map(tsr_diagnostics::Diagnostic::text).collect::<Vec<_>>()
    );
    let bound = tsr_binder::bind(
        &arena,
        parsed.source_file,
        &parsed.nodes,
        tsr_binder::FileInfo { name: "test.ts", text: source },
    );
    let mut checker = Checker::new(&bound, &parsed.nodes, &parsed.node_map);
    for statement in parsed.source_file.statements {
        let Statement::VariableStatement(node) = statement else { continue };
        for declaration in node.declaration_list.map(|l| l.declarations).unwrap_or_default() {
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
    panic!("the fixture must declare `{name}`");
}

fn probe(declaration: &str, name: &str) -> String {
    type_of_declaration(&format!("{MODULE}{declaration}"), name)
}

// tsgo: `Type '() => void' is not assignable to type 'never'`.
#[test]
fn a_qualifier_is_read_as_a_property_of_the_value() {
    assert_eq!(probe("var r1: typeof import(\"m\").A.foo;", "r1"), "() => void");
}

// tsgo: `Type 'number' is not assignable to type 'never'`.
#[test]
fn a_qualified_value_answers_its_type() {
    assert_eq!(probe("var r3: typeof import(\"m\").n;", "r3"), "number");
}

// tsgo: `Type '(x: string) => string' is not assignable to type 'never'`.
#[test]
fn written_type_arguments_instantiate_the_value() {
    assert_eq!(probe("var r2: typeof import(\"m\").f<string>;", "r2"), "(x: string) => string");
}

// tsgo: TS2694 `Namespace '"m"' has no exported member 'I'`, and `errorType`.
#[test]
fn a_type_only_member_is_not_a_property_of_the_value() {
    assert_eq!(probe("var r4: typeof import(\"m\").I;", "r4"), "error");
}
