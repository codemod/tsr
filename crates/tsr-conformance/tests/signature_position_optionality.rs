//! Positional parameter optionality, pinned against native 5b1047d1.
use tsr_conformance::{TestCase, types_producer};

const SOURCE: &str = r#"function optional(p?: number) { p; p = undefined; p = "bad"; }
function defaulted(d: number = 1) { d; d = undefined; d = "bad"; }
function before(this: void, b: number = 1, r: string) { b; b = undefined; r = undefined; }
function required(r: number) { r; r = undefined; }
function rest(...values: number[]) {}
function tuple(...values: [number, string?]) {}
declare let requiredFn: (value: number) => void;
declare let optionalFn: (value?: number) => void;
declare let wideFn: (value: number | undefined) => void;
optionalFn = requiredFn;
requiredFn = optionalFn;
optionalFn = wideFn;
wideFn = optionalFn;
declare let requiredPair: (value: number, next: string) => void;
declare let widePair: (value: number | undefined, next: string) => void;
requiredPair = before;
widePair = before;
let defaultPair: typeof before = requiredPair;
defaultPair = widePair;
optional(); optional(undefined); defaulted(); defaulted(undefined);
before(undefined, "next"); before(1, "next"); before();
required(undefined); required(1); required();
"#;

#[test]
fn positional_relations_distinguish_optional_defaults_and_body_writes() {
    for strict in [false, true] {
        let source =
            format!("// @strictNullChecks: {strict}\n// @strictFunctionTypes: true\n{SOURCE}");
        let case = TestCase::parse("probe/signature-positions", "positions.ts", &source);
        let arena = tsr_core::Arena::new();
        let program = types_producer::program_for_case(&arena, &case);
        let mut checker = types_producer::configured_checker(&program);
        let file = program.source_file("positions.ts").unwrap();
        let mut stack = vec![tsr_ast::Node::SourceFile(file.source_file())];
        let mut assignments = Vec::new();
        while let Some(node) = stack.pop() {
            tsr_ast::push_children(node, &mut stack);
            if let tsr_ast::Node::BinaryExpression(binary) = node
                && binary.operator_token.is_some_and(|t| t.kind == tsr_ast::SyntaxKind::EqualsToken)
            {
                let target = checker.check_expression(binary.left.unwrap());
                let value = checker.check_expression(binary.right.unwrap());
                assignments.push((
                    program.nodes().span(binary.node_id.unwrap()).start,
                    checker.is_type_assignable_to(value, target),
                ));
            } else if let tsr_ast::Node::VariableDeclaration(variable) = node
                && let (Some(annotation), Some(initializer)) =
                    (variable.r#type, variable.initializer)
            {
                let target = checker.get_type_from_type_node(annotation);
                let value = checker.check_expression(initializer);
                assignments.push((
                    program.nodes().span(variable.node_id.unwrap()).start,
                    checker.is_type_assignable_to(value, target),
                ));
            }
        }
        assignments.sort_unstable_by_key(|(position, _)| *position);
        let got: Vec<_> = assignments.into_iter().map(|(_, related)| related).collect();
        assert_eq!(
            got,
            [
                true, false, !strict, false, !strict, !strict, !strict, !strict, true, true, true,
                true, true, !strict, true
            ],
            "body writes and both function-assignment directions, strict={strict}"
        );
    }
}

#[test]
fn calls_and_parameter_body_writes_keep_their_native_boundaries() {
    for strict in [false, true] {
        let source =
            format!("// @strictNullChecks: {strict}\n// @strictFunctionTypes: true\n{SOURCE}");
        let case = TestCase::parse("probe/signature-position-errors", "positions.ts", &source);
        let mut got: Vec<_> = tsr_conformance::diagnostics_suite::reported_for(&case)
            .iter()
            .map(|d| (d.line, d.column, d.code))
            .collect();
        got.sort_unstable();
        let expected = if strict {
            vec![
                (1, 51, 2322),
                (2, 40, 2322),
                (2, 55, 2322),
                (3, 60, 2322),
                (3, 75, 2322),
                (4, 35, 2322),
                (10, 1, 2322),
                (18, 5, 2322),
                (21, 47, 2554),
                (22, 10, 2345),
                (22, 35, 2554),
            ]
        } else {
            vec![(1, 51, 2322), (2, 55, 2322), (21, 47, 2554), (22, 35, 2554)]
        };
        assert_eq!(got, expected, "strictNullChecks={strict}");
    }
}
