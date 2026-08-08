//! Private-name property access — `this.#x` (`bd tsr-0opd`,
//! `docs/architecture/checker-notes-privname.md`).
//!
//! Every expectation is copied from a `.types` baseline and named at the
//! assertion. The corpus's most common private-name lines are
//! `>this.#field : number` (41), `>this.#test : number` (28) and
//! `>this.#fieldFunc : () => void` (10).

use tsr_checker::Checker;
use tsr_core::Arena;

/// The printed type of the first `this.#name` access in the fixture.
fn type_of_private_access(source: &str) -> String {
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
    // Walk the tree from the source file — `push_children` is the same
    // traversal the checker's own probes use, and it avoids depending on how
    // `NodeId` is constructed.
    let mut stack = vec![tsr_ast::Node::SourceFile(parsed.source_file)];
    let mut children = Vec::new();
    while let Some(node) = stack.pop() {
        if let tsr_ast::Node::PropertyAccessExpression(access) = node
            && matches!(access.name, Some(tsr_ast::MemberName::PrivateIdentifier(_)))
        {
            let ty =
                checker.check_expression(tsr_ast::Expression::PropertyAccessExpression(access));
            return checker.type_to_string(ty);
        }
        children.clear();
        tsr_ast::push_children(node, &mut children);
        stack.extend(children.iter().copied());
    }
    panic!("the fixture must contain a private-name access");
}

/// `conformance/privateNameFieldAssignment.types` and its family record
/// `>this.#field : number` 41 times across the corpus — the most common
/// private-name line there is.
#[test]
fn a_private_field_access_answers_the_fields_type() {
    let source = "class C {
    #field: number = 1;
    m() { return this.#field; }
}";
    assert_eq!(type_of_private_access(source), "number");
}

/// `>this.#d : string` (9) — the same rule for a string field, so the arm is
/// not keyed to one primitive.
#[test]
fn the_answer_is_the_declared_member_type() {
    let source = "class C {
    #d: string = \"a\";
    m() { return this.#d; }
}";
    assert_eq!(type_of_private_access(source), "string");
}

/// `>this.#fieldFunc : () => void` (10) — a private field holding a function
/// type prints as a property, through the same lookup.
#[test]
fn a_private_field_of_function_type_prints_its_signature() {
    let source = "class C {
    #fieldFunc: () => void;
    m() { return this.#fieldFunc; }
}";
    assert_eq!(type_of_private_access(source), "() => void");
}

/// The safe failure: a private name that is **not** a member of the
/// receiver's type answers `errorType`, which is upstream's type answer too —
/// the scope rule upstream enforces
/// (`lookupSymbolForPrivateIdentifierDeclaration`) is a *diagnostic* channel
/// this port does not have (ADR-0040), and it does not change the type.
/// Asserted beside the ported form so the pair keeps discriminating.
#[test]
fn an_absent_private_name_is_a_gap() {
    let present = "class C {
    #a: number = 1;
    m() { return this.#a; }
}";
    assert_eq!(type_of_private_access(present), "number");
    let absent = "class C {
    #a: number = 1;
    m() { return this.#missing; }
}";
    assert_eq!(type_of_private_access(absent), "error");
}
