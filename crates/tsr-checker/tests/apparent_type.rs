//! `getApparentType`'s instantiable head — a type parameter is read through
//! its `extends` constraint (`bd tsr-rppd`,
//! `docs/architecture/checker-notes-apparent.md`).
//!
//! The evidence at scale is `examples/apparent.rs`, which compared this arm's
//! answer to the baseline string for string on **98 lines** before the code
//! existed. These fixtures are self-contained rather than copied verbatim
//! from a case, because the corpus's converting lines
//! (`compiler/protectedMembersThisParameter` records `>this.toLowerCase : ()
//! => string`) constrain to `string`, whose apparent type is the `String`
//! interface — and this harness loads no lib files, so the interesting half
//! of that line is unreachable here. The rule under test is the same one.

use tsr_ast::Node;
use tsr_checker::Checker;
use tsr_core::Arena;

/// The printed type of the first property access spelled `<receiver>.<name>`.
fn type_of_access(source: &str, receiver_text: &str, member: &str) -> String {
    let arena = Arena::new();
    let parsed = tsr_parser::parse(&arena, source);
    assert!(
        parsed.diagnostics.is_empty(),
        "fixture must parse: {:?}",
        parsed.diagnostics.iter().map(tsr_diagnostics::Diagnostic::text).collect::<Vec<_>>()
    );
    let bound = tsr_binder::bind(
        parsed.source_file,
        &parsed.nodes,
        tsr_binder::FileInfo { name: "test.ts", text: source },
    );
    let mut checker = Checker::new(&bound, &parsed.nodes, &parsed.node_map);
    let mut stack = vec![Node::SourceFile(parsed.source_file)];
    let mut children = Vec::new();
    while let Some(node) = stack.pop() {
        if let Node::PropertyAccessExpression(access) = node
            && let Some(tsr_ast::MemberName::Identifier(name)) = access.name
            && name.text == member
            && let Some(receiver) = access.expression
            && matches!(receiver, tsr_ast::Expression::Identifier(r) if r.text == receiver_text)
        {
            let ty =
                checker.check_expression(tsr_ast::Expression::PropertyAccessExpression(access));
            return checker.type_to_string(ty);
        }
        children.clear();
        tsr_ast::push_children(node, &mut children);
        stack.extend(children.iter().copied());
    }
    panic!("the fixture must contain `{receiver_text}.{member}`");
}

#[test]
fn a_constrained_type_parameter_reads_members_through_its_constraint() {
    let source = "interface Shape { name: string; size: number; }
function f<T extends Shape>(t: T) { return t.name; }";
    assert_eq!(type_of_access(source, "t", "name"), "string");
    let numeric = "interface Shape { name: string; size: number; }
function f<T extends Shape>(t: T) { return t.size; }";
    assert_eq!(type_of_access(numeric, "t", "size"), "number");
}

/// The refused halves, each beside the ported one, so the pair keeps
/// discriminating when the frontier moves.
#[test]
fn the_unconstrained_and_absent_cases_stay_gaps() {
    // An **unconstrained** parameter: upstream's head substitutes
    // `unknownType`, whose lookup finds nothing — the same gap as before the
    // arm existed, which is why this arm cannot lose a line on its own.
    let unconstrained = "interface Shape { name: string; }
function f<T>(t: T) { return t.name; }
function g<U extends Shape>(u: U) { return u.name; }";
    assert_eq!(type_of_access(unconstrained, "t", "name"), "error");
    assert_eq!(type_of_access(unconstrained, "u", "name"), "string");

    // A member the constraint does not have stays a gap, which is upstream's
    // type answer too (it reports, and reporting is a channel this port does
    // not have — ADR-0040).
    let absent = "interface Shape { name: string; }
function f<T extends Shape>(t: T) { return t.missing; }
function g<U extends Shape>(u: U) { return u.name; }";
    assert_eq!(type_of_access(absent, "t", "missing"), "error");
    assert_eq!(type_of_access(absent, "u", "name"), "string");
}

/// A constraint that itself gaps leaves the parameter alone rather than
/// reading members off `errorType` — the whole-construct refusal rule.
#[test]
fn a_gapping_constraint_leaves_the_parameter_a_gap() {
    let source = "interface Shape { name: string; }
function f<T extends Unresolved>(t: T) { return t.name; }
function g<U extends Shape>(u: U) { return u.name; }";
    assert_eq!(type_of_access(source, "t", "name"), "error");
    assert_eq!(type_of_access(source, "u", "name"), "string");
}
