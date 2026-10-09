//! `typeof x` in type position — `bd tsr-4sc.10`,
//! `docs/architecture/checker-notes-tquery.md`.
//!
//! Every expectation here was taken from
//! `submodule/conformance/typeofANonExportedType.types` **before** it was
//! written down (the convention `docs/conventions.md` records being stated in
//! a header and then not followed — three intuition-written expectations in one
//! file were all wrong). The baseline's own lines are quoted at each test.

use tsr_ast::Statement;
use tsr_checker::Checker;
use tsr_core::Arena;

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

// `typeofANonExportedType.types`:
//   var x = 1;            >x : number
//   export var r1: typeof x;   >r1 : number
#[test]
fn typeof_a_widened_var_is_its_widened_type() {
    assert_eq!(type_of_declaration("var x = 1;\nvar r1: typeof x;", "r1"), "number");
}

// `typeofANonExportedType.types`:
//   class C { foo: string; }
//   export var r3: typeof C;   >r3 : typeof C
#[test]
fn typeof_a_class_is_the_constructor_side() {
    assert_eq!(
        type_of_declaration("class C { foo: string; }\nvar r3: typeof C;", "r3"),
        "typeof C"
    );
}

// `typeofANonExportedType.types`:
//   var c2: C;            >c2 : C
//   export var r4b: typeof c2;  >r4b : C
#[test]
fn typeof_an_instance_variable_is_the_instance_type() {
    assert_eq!(
        type_of_declaration("class C { foo: string; }\nvar c2: C;\nvar r4b: typeof c2;", "r4b"),
        "C"
    );
}

// `typeofANonExportedType.types`:
//   interface I { foo: string; }
//   var i2: I;            >i2 : I
//   export var r5: typeof i2;   >r5 : I
#[test]
fn typeof_an_interface_typed_variable_is_the_interface() {
    assert_eq!(
        type_of_declaration("interface I { foo: string; }\nvar i2: I;\nvar r5: typeof i2;", "r5"),
        "I"
    );
}

// `typeofANonExportedType.types`:
//   namespace M { export var foo = ''; }   >M : typeof M
//   export var r6: typeof M;   >r6 : typeof M
//   export var r7: typeof M.foo;   >r7 : string
#[test]
fn typeof_a_namespace_and_a_qualified_export() {
    let source = "namespace M { export var foo = ''; }\nvar r6: typeof M;\nvar r7: typeof M.foo;";
    assert_eq!(type_of_declaration(source, "r6"), "typeof M");
    assert_eq!(type_of_declaration(source, "r7"), "string");
}

// Native 5b1047d: `typeof this` routes through `checkThisExpression`
// (`checker.go:10652`). queryThisWave47(strict=true).types confirms a class
// instance method's receiver is the polymorphic `this`, not `typeof C`.
#[test]
fn typeof_this_reads_the_instance_receiver_while_typeof_the_class_reads_its_value() {
    let source = "class C { m() { var v: typeof this; var w: typeof C; } }";
    let arena = Arena::new();
    let parsed = tsr_parser::parse(&arena, source);
    assert!(parsed.diagnostics.is_empty());
    let bound = tsr_binder::bind(
        &arena,
        parsed.source_file,
        &parsed.nodes,
        tsr_binder::FileInfo { name: "test.ts", text: source },
    );
    let mut checker = Checker::new(&bound, &parsed.nodes, &parsed.node_map);
    let mut answers = Vec::new();
    for index in 0..u32::try_from(parsed.node_map.len()).expect("node count fits u32") {
        let Some(node) = parsed.node_map.get(tsr_ast::NodeId::new(index)) else { continue };
        if let Ok(type_node) = tsr_ast::TypeNode::try_from(node)
            && matches!(node, tsr_ast::Node::TypeQueryNode(_))
        {
            let type_id = checker.get_type_from_type_node(type_node);
            answers.push(checker.type_to_string(type_id));
        }
    }
    answers.sort();
    assert_eq!(answers, ["this", "typeof C"]);
}

// `typeof a` over a PARAMETER computes as upstream computes it, and the
// signature that renders it **reuses the written node** — the mechanism the
// registered bar forced (`checker-notes-tquery.md` §5): the first build
// printed the resolved structure and measured 1,341 gap→wrong lines, 1,059
// with `typeof` still in the wanted text. The whole expectation is one
// baseline line, `conformance/subtypingWithCallSignatures2.types:26`:
//   >foo1 : { (a: (x: number) => number[]): typeof a; (a: any): any; }
#[test]
fn typeof_a_parameter_prints_as_written_inside_its_signature() {
    // The `interface Array<T> {}` line is the minimal lib `tests/arrays.rs`
    // and `tests/union_printing.rs` declare: the harness loads no lib files,
    // and without it `number[]` measures the missing global rather than this
    // arm.
    let source = "interface Array<T> {}\ndeclare function foo1(a: (x: number) => number[]): typeof a;\ndeclare function foo1(a: any): any;";
    let arena = Arena::new();
    let parsed = tsr_parser::parse(&arena, source);
    assert!(parsed.diagnostics.is_empty());
    let bound = tsr_binder::bind(
        &arena,
        parsed.source_file,
        &parsed.nodes,
        tsr_binder::FileInfo { name: "test.ts", text: source },
    );
    let mut checker = Checker::new(&bound, &parsed.nodes, &parsed.node_map);
    let Some(Statement::FunctionDeclaration(function)) = parsed.source_file.statements.get(1)
    else {
        panic!("the fixture's second statement is the function");
    };
    let symbol = bound
        .symbol_of(function.node_id.expect("registered"))
        .expect("the declaration must be bound");
    let id = checker.get_type_of_symbol(symbol);
    assert_eq!(
        checker.type_to_string(id),
        "{ (a: (x: number) => number[]): typeof a; (a: any): any; }"
    );
}

// The PARAMETER-position half of the same reuse rule, which the return-position
// test above cannot see (its `written_text` mutation stayed green — recorded
// per the convention that a mutation that fails to redden is a finding).
// `conformance/anyAssignabilityInInheritance.types:220-231`:
//   function f() { }
//   declare function foo15(x: typeof f): typeof f;
//   >foo15 : { (x: typeof f): typeof f; (x: any): any; }
#[test]
fn typeof_in_parameter_position_prints_as_written_too() {
    let source = "function f() { }\ndeclare function foo15(x: typeof f): typeof f;\ndeclare function foo15(x: any): any;";
    let arena = Arena::new();
    let parsed = tsr_parser::parse(&arena, source);
    assert!(parsed.diagnostics.is_empty());
    let bound = tsr_binder::bind(
        &arena,
        parsed.source_file,
        &parsed.nodes,
        tsr_binder::FileInfo { name: "test.ts", text: source },
    );
    let mut checker = Checker::new(&bound, &parsed.nodes, &parsed.node_map);
    let Some(Statement::FunctionDeclaration(function)) = parsed.source_file.statements.get(1)
    else {
        panic!("the fixture's second statement is foo15");
    };
    let symbol = bound
        .symbol_of(function.node_id.expect("registered"))
        .expect("the declaration must be bound");
    let id = checker.get_type_of_symbol(symbol);
    assert_eq!(checker.type_to_string(id), "{ (x: typeof f): typeof f; (x: any): any; }");
}

// Instantiation expressions `typeof f<string>` (10 corpus lines) reach
// `getInstantiationExpressionType` (`checker.go:10660`), which filters
// signatures by arity and instantiates each — refused whole-construct, while
// the argument-less `typeof f` on the same symbol answers.
//
// The declaration and the expected rendering are both from
// `compiler/inferentialTypingWithFunctionTypeSyntacticScenarios.types`:
//   declare function identity<V>(y: V): V;   >identity : <V>(y: V) => V
//   <typeof identity>identity                : <V>(y: V) => V
#[test]
fn an_instantiation_expression_instantiates_while_plain_typeof_answers() {
    // `getInstantiationExpressionType` (`checker.go:10660`): the one
    // applicable signature, instantiated (`docs/parity/notes/r5-instexpr.md`).
    let source = "declare function identity<V>(y: V): V;\nvar a: typeof identity<string>;\nvar b: typeof identity;";
    // The node builder then reuses the written `typeof` node for the result
    // (`nodebuilderimpl.go:2816`; `>FnAlias : typeof fn<T>` in
    // `aliasInstantiationExpressionGenericIntersectionNoCrash2`,
    // r6-typesroots §4).
    assert_eq!(type_of_declaration(source, "a"), "typeof identity<string>");
    assert_eq!(type_of_declaration(source, "b"), "<V>(y: V) => V");
}
