//! Template/string-mapping intersection reduction from
//! `getIntersectionTypeEx` and `extractRedundantTemplateLiterals`
//! (`checker.go:26056`, `:26317`). The controls are deliberately asymmetric:
//! matching and non-matching literals must take opposite roads, while a
//! generic template hole must retain its structure.

use tsr_ast::Statement;
use tsr_checker::Checker;
use tsr_core::Arena;

fn annotation_type(source: &str, index: usize) -> String {
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
    let annotation = match parsed.source_file.statements[index] {
        Statement::VariableStatement(statement) => statement
            .declaration_list
            .and_then(|list| list.declarations.first().copied())
            .and_then(|declaration| declaration.r#type),
        Statement::FunctionDeclaration(function) => {
            function.parameters.first().and_then(|parameter| parameter.r#type)
        }
        _ => panic!("statement {index} must declare a variable or function"),
    }
    .expect("an annotation");
    let ty = checker.get_type_from_type_node(annotation);
    checker.type_to_string(ty)
}

const PRELUDE: &str = "type Lowercase<S extends string> = intrinsic;\n\
    type Capitalize<S extends string> = intrinsic;\n";

#[test]
fn string_is_redundant_beside_template_and_mapping_subtypes() {
    let source = format!(
        "{PRELUDE}\
         declare let numeric: string & `${{number}}`;\n\
         declare let mapped: string & Lowercase<string>;"
    );
    assert_eq!(annotation_type(&source, 2), "`${number}`");
    assert_eq!(annotation_type(&source, 3), "Lowercase<string>");
}

#[test]
fn matching_literals_win_and_disjoint_pattern_literals_are_never() {
    let source = format!(
        "{PRELUDE}\
         declare let template_match: \"prop\" & `p${{Lowercase<string>}}p`;\n\
         declare let mapping_match: \"Prop\" & Capitalize<string>;\n\
         declare let mapping_miss: \"PROP\" & Lowercase<string>;\n\
         declare let template_miss: \"setX\" & `get${{string}}`;"
    );
    assert_eq!(annotation_type(&source, 2), "\"prop\"");
    assert_eq!(annotation_type(&source, 3), "\"Prop\"");
    assert_eq!(annotation_type(&source, 4), "never");
    assert_eq!(annotation_type(&source, 5), "never");
}

#[test]
fn a_generic_template_hole_keeps_its_structure() {
    let source = format!(
        "{PRELUDE}\
         declare function generic<T extends string>(x: string & `${{T}}`): void;"
    );
    assert_eq!(annotation_type(&source, 2), "`${T}`");
}
