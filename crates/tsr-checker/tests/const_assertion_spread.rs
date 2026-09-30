//! Array-like spreads in const assertions retain tuple arguments until
//! normalization expands concrete tuples or combines unbounded rest elements.
//! Corresponds to `checkArrayLiteral` and `TupleNormalizer.normalize` in tsgo.

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
fn an_array_spread_retains_an_unbounded_rest() {
    let source = format!("{LIB}declare const t: number[];\nconst a = [1, ...t] as const;");
    assert_eq!(type_of_initialiser(&source, "a"), "readonly [1, ...number[]]");
}

#[test]
fn a_generic_spread_is_substituted_in_a_const_return() {
    let source = format!(
        "{LIB}function f<T extends unknown[]>(t: [...T]) {{ return [1, ...t, 2] as const; }}\n\
         const a = f(['hello', true]);"
    );
    assert_eq!(type_of_initialiser(&source, "a"), "readonly [1, string, boolean, 2]");
}

#[test]
fn multiple_array_spreads_normalize_to_a_single_rest() {
    let source = format!(
        "{LIB}declare const t: number[]; declare const u: string[];\n\
         const a = [1, ...t, true, ...u, 2] as const;"
    );
    assert_eq!(type_of_initialiser(&source, "a"), "readonly [1, ...(string | number | true)[], 2]");
}

#[test]
fn an_any_spread_still_produces_an_array() {
    let source = format!("{LIB}declare const t: any; const a = [...t];");
    assert_eq!(type_of_initialiser(&source, "a"), "any[]");
}
