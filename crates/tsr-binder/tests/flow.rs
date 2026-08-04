//! What the control-flow graph must get right.
//!
//! The graph is not directly observable — the checker walks it, and there is no
//! checker yet — so these tests ask the questions narrowing will ask, in the
//! only vocabulary available: *which* flow node is in effect at a given
//! identifier, and what its antecedents are.
//!
//! Two shapes of assertion, both deliberate:
//!
//! - **Structural.** Walk back from an identifier's flow node and describe the
//!   path in terms of the flow flags on it. This is what a narrowing query does,
//!   so a graph that answers these correctly answers narrowing correctly.
//! - **Negative.** A construct that should *not* produce a node — `if (foo())`
//!   branches, but nothing about the branch narrows anything — is worth an
//!   assertion of its own, because the failure mode of a filter is a graph that
//!   is quietly too big and still passes every positive test.

use tsr_ast::{Node, NodeId, NodeTable, SyntaxKind};
use tsr_binder::{BindResult, FlowFlags, FlowId, NodeFacts};
use tsr_core::Arena;
use tsr_parser::ParsedSourceFile;

struct Bound<'a> {
    source: &'a str,
    parsed: ParsedSourceFile<'a>,
    result: BindResult<'a>,
}

fn bind<'a>(arena: &'a Arena, source: &'a str) -> Bound<'a> {
    let parsed = tsr_parser::parse(arena, source);
    assert!(
        parsed.diagnostics.is_empty(),
        "the source should parse cleanly: {:?}",
        parsed.diagnostics.iter().map(tsr_diagnostics::Diagnostic::text).collect::<Vec<_>>()
    );
    let result = tsr_binder::bind(
        parsed.source_file,
        &parsed.nodes,
        tsr_binder::FileInfo { name: "test.ts", text: source },
    );
    Bound { source, parsed, result }
}

impl Bound<'_> {
    fn nodes(&self) -> &NodeTable {
        &self.parsed.nodes
    }

    /// Every node id in the file, in source order.
    fn ids(&self) -> impl Iterator<Item = NodeId> + '_ {
        (0..self.nodes().len()).map(|index| NodeId::new(u32::try_from(index).expect("fits")))
    }

    /// The id of the `occurrence`th identifier spelled `name`, counting from
    /// zero in source order.
    ///
    /// Positions rather than names alone: a test about narrowing is a test about
    /// *one* mention of a variable, and "the second `x`" is how such a claim is
    /// stated in prose.
    fn identifier(&self, name: &str, occurrence: usize) -> NodeId {
        let mut seen = 0;
        for id in self.ids() {
            if self.nodes().kind(id) != SyntaxKind::Identifier {
                continue;
            }
            let span = self.nodes().span(id);
            let text = &self.source[span.start as usize..span.end as usize];
            if text == name {
                if seen == occurrence {
                    return id;
                }
                seen += 1;
            }
        }
        panic!("no identifier {name:?} #{occurrence} in the source");
    }

    /// The first node of `kind`, in source order.
    fn first(&self, kind: SyntaxKind) -> NodeId {
        self.ids()
            .find(|id| self.nodes().kind(*id) == kind)
            .unwrap_or_else(|| panic!("no {kind:?} in the source"))
    }

    fn flow_of(&self, node: NodeId) -> Option<FlowId> {
        self.result.flow_of(node)
    }

    /// The flow flags on the chain back from `node`, nearest first, following
    /// single antecedents and stopping at the first junction or start.
    ///
    /// This is the shape a narrowing query sees walking backwards from a
    /// reference, minus the branching it would explore.
    fn chain(&self, node: NodeId) -> Vec<FlowFlags> {
        let flow = self.result.flow();
        let mut out = Vec::new();
        let mut current = self.flow_of(node);
        while let Some(id) = current {
            // Mask off the reference-counting bits: they say how the graph is
            // shared, not what it means.
            let flags = flow.flags(id) - (FlowFlags::REFERENCED | FlowFlags::SHARED);
            out.push(flags);
            if flags.intersects(FlowFlags::LABEL) || flags.contains(FlowFlags::START) {
                break;
            }
            current = flow.antecedent(id);
        }
        out
    }

    /// The antecedents of the label `node`'s flow is, if it is one.
    fn antecedent_flags(&self, node: NodeId) -> Vec<FlowFlags> {
        let flow = self.result.flow();
        let Some(id) = self.flow_of(node) else { return Vec::new() };
        flow.antecedents(id)
            .map(|antecedent| flow.flags(antecedent) - (FlowFlags::REFERENCED | FlowFlags::SHARED))
            .collect()
    }

    fn source_file(&self) -> NodeId {
        Node::SourceFile(self.parsed.source_file).node_id().expect("registered")
    }
}

#[test]
fn a_file_with_no_branches_is_one_straight_chain() {
    let arena = Arena::new();
    let bound = bind(&arena, "let x = 1;\nx;\nx;");
    // Two assignments would make two nodes; one declaration with an initializer
    // makes one, and both reads see it.
    let first_read = bound.identifier("x", 1);
    let second_read = bound.identifier("x", 2);
    assert_eq!(bound.chain(first_read), vec![FlowFlags::ASSIGNMENT, FlowFlags::START]);
    assert_eq!(bound.flow_of(first_read), bound.flow_of(second_read));
}

#[test]
fn an_if_condition_narrows_only_inside_the_branch() {
    let arena = Arena::new();
    let bound = bind(&arena, "declare let x: string | undefined;\nif (x) { x; }\nx;");
    let inside = bound.identifier("x", 2);
    let after = bound.identifier("x", 3);
    assert_eq!(
        bound.chain(inside),
        vec![FlowFlags::TRUE_CONDITION, FlowFlags::START],
        "the reference inside the branch sits behind the true condition"
    );
    // After the `if`, control arrives either way, so the flow is the junction of
    // both branches rather than either condition.
    assert_eq!(
        bound.antecedent_flags(after),
        vec![FlowFlags::TRUE_CONDITION, FlowFlags::FALSE_CONDITION],
        "after the `if`, both outcomes merge"
    );
}

#[test]
fn an_else_branch_sits_behind_the_false_condition() {
    let arena = Arena::new();
    let bound = bind(&arena, "declare let x: string | undefined;\nif (x) { 1; } else { x; }");
    let in_else = bound.identifier("x", 2);
    assert_eq!(bound.chain(in_else), vec![FlowFlags::FALSE_CONDITION, FlowFlags::START]);
}

#[test]
fn a_condition_that_narrows_nothing_produces_no_condition_node() {
    let arena = Arena::new();
    let bound =
        bind(&arena, "declare function f(): boolean;\ndeclare let x: string;\nif (f()) { x; }");
    // `f()` takes no narrowable argument, so the branch is real but invisible to
    // narrowing and upstream records nothing for it. A graph that recorded one
    // would be bigger and no more useful.
    let inside = bound.identifier("x", 1);
    assert_eq!(bound.chain(inside), vec![FlowFlags::START]);
}

#[test]
fn logical_and_narrows_its_right_operand() {
    let arena = Arena::new();
    let bound = bind(&arena, "declare let x: string | undefined;\nif (x && x.length) { 1; }");
    // The second `x` is evaluated only when the first was truthy.
    let right = bound.identifier("x", 2);
    assert_eq!(bound.chain(right), vec![FlowFlags::TRUE_CONDITION, FlowFlags::START]);
}

#[test]
fn logical_or_narrows_its_right_operand_the_other_way() {
    let arena = Arena::new();
    let bound = bind(&arena, "declare let x: string | undefined;\nif (!x || x.length) { 1; }");
    let right = bound.identifier("x", 2);
    assert_eq!(
        bound.chain(right),
        vec![FlowFlags::FALSE_CONDITION, FlowFlags::START],
        "`!x || …` reaches the right operand when `!x` was false"
    );
}

#[test]
fn a_while_loop_body_is_reached_through_a_loop_label() {
    let arena = Arena::new();
    let bound = bind(&arena, "declare let x: string | undefined;\nwhile (x) { x; }");
    let inside = bound.identifier("x", 2);
    assert_eq!(
        bound.chain(inside),
        vec![FlowFlags::TRUE_CONDITION, FlowFlags::LOOP_LABEL],
        "the body is behind the condition, and the condition behind the loop junction"
    );
    // The loop label merges entry with the back edge: that is what makes a
    // narrowing inside the body visible on the next iteration.
    let condition = bound.identifier("x", 1);
    let flow = bound.result.flow();
    let loop_label = flow.antecedent(bound.flow_of(inside).expect("has flow")).expect("has one");
    assert_eq!(bound.flow_of(condition), Some(loop_label));
    assert_eq!(flow.antecedents(loop_label).count(), 2, "entry and the back edge");
}

#[test]
fn an_assignment_in_a_loop_reaches_the_next_iteration() {
    let arena = Arena::new();
    let bound = bind(&arena, "declare let x: string | undefined;\nwhile (true) { x; x = \"a\"; }");
    // #0 is the declaration's name; #1 is the read at the top of the body.
    let read = bound.identifier("x", 1);
    let flow = bound.result.flow();
    let loop_label = bound.flow_of(read).expect("the read has a flow node");
    assert!(flow.flags(loop_label).contains(FlowFlags::LOOP_LABEL));
    let back_edge_is_the_assignment = flow
        .antecedents(loop_label)
        .any(|antecedent| flow.flags(antecedent).contains(FlowFlags::ASSIGNMENT));
    assert!(back_edge_is_the_assignment, "the assignment is the loop's back edge");
}

#[test]
fn code_after_a_return_is_unreachable_and_gets_no_flow_node() {
    let arena = Arena::new();
    let bound = bind(&arena, "function f() {\n  return 1;\n  let x = 2;\n}");
    let statement = bound
        .ids()
        .find(|id| bound.nodes().kind(*id) == SyntaxKind::VariableStatement)
        .expect("the file has a variable statement");
    assert!(
        bound.result.facts(statement).contains(NodeFacts::UNREACHABLE),
        "a `let` after a `return` is unreachable code worth reporting"
    );
    assert_eq!(bound.flow_of(statement), None, "unreachable code has no flow node");
}

#[test]
fn a_type_alias_after_a_return_is_unreachable_but_not_reportable() {
    let arena = Arena::new();
    let bound = bind(&arena, "function f() {\n  return 1;\n  type T = string;\n}");
    let alias = bound.first(SyntaxKind::TypeAliasDeclaration);
    // Upstream's `IsPotentiallyExecutableNode` is what draws this line: a type
    // alias emits nothing, so its being unreachable is not a mistake.
    assert!(!bound.result.facts(alias).contains(NodeFacts::UNREACHABLE));
}

#[test]
fn a_function_that_returns_on_every_path_has_no_implicit_return() {
    let arena = Arena::new();
    let bound = bind(&arena, "function f(c: boolean) {\n  if (c) { return 1; }\n  return 2;\n}");
    let function = bound.first(SyntaxKind::FunctionDeclaration);
    let facts = bound.result.facts(function);
    assert!(
        facts.contains(NodeFacts::HAS_EXPLICIT_RETURN)
            || !facts.contains(NodeFacts::HAS_IMPLICIT_RETURN)
    );
    assert!(
        !facts.contains(NodeFacts::HAS_IMPLICIT_RETURN),
        "every path returns, so the end of the body is unreachable"
    );
    assert_eq!(bound.result.end_flow(function), None);
}

#[test]
fn a_function_that_can_fall_off_the_end_says_so() {
    let arena = Arena::new();
    let bound = bind(&arena, "function f(c: boolean) {\n  if (c) { return 1; }\n}");
    let function = bound.first(SyntaxKind::FunctionDeclaration);
    let facts = bound.result.facts(function);
    assert!(facts.contains(NodeFacts::HAS_IMPLICIT_RETURN));
    assert!(facts.contains(NodeFacts::HAS_EXPLICIT_RETURN), "it also has a `return`");
    assert!(bound.result.end_flow(function).is_some());
}

#[test]
fn a_break_leaves_the_loop_and_makes_what_follows_unreachable() {
    let arena = Arena::new();
    let bound = bind(&arena, "while (true) {\n  break;\n  let x = 1;\n}\n");
    let statement = bound.first(SyntaxKind::VariableStatement);
    assert!(bound.result.facts(statement).contains(NodeFacts::UNREACHABLE));
}

#[test]
fn a_labelled_break_targets_the_label_not_the_inner_loop() {
    let arena = Arena::new();
    let bound =
        bind(&arena, "outer: while (true) {\n  while (true) { break outer; }\n  let x = 1;\n}\n");
    // `break outer` leaves both loops, so the statement after the inner loop is
    // reached only by the inner loop's own exit — which `while (true)` does not
    // have. Nothing reaches it.
    let statement = bound.first(SyntaxKind::VariableStatement);
    assert!(bound.result.facts(statement).contains(NodeFacts::UNREACHABLE));
}

#[test]
fn a_label_nothing_jumps_to_is_recorded_as_unused() {
    let arena = Arena::new();
    let bound = bind(&arena, "unused: while (true) { break; }");
    let label = bound.identifier("unused", 0);
    assert!(bound.result.facts(label).contains(NodeFacts::UNUSED_LABEL));

    let bound = bind(&arena, "used: while (true) { break used; }");
    let label = bound.identifier("used", 0);
    assert!(!bound.result.facts(label).contains(NodeFacts::UNUSED_LABEL));
}

#[test]
fn a_switch_clause_narrows_on_its_own_range() {
    let arena = Arena::new();
    let bound = bind(
        &arena,
        "declare let x: \"a\" | \"b\";\nswitch (x) {\n  case \"a\": x; break;\n  case \"b\": x; break;\n}",
    );
    let flow = bound.result.flow();
    // #0 is the declaration's name, #1 the switch subject, #2 and #3 the reads.
    let in_first = bound.flow_of(bound.identifier("x", 2)).expect("has flow");
    let clause = flow.switch_clause(in_first).expect("a switch clause node");
    assert_eq!((clause.clause_start, clause.clause_end), (0, 1));

    let in_second = bound.flow_of(bound.identifier("x", 3)).expect("has flow");
    let clause = flow.switch_clause(in_second).expect("a switch clause node");
    assert_eq!((clause.clause_start, clause.clause_end), (1, 2));
}

#[test]
fn a_switch_without_a_default_can_match_nothing() {
    let arena = Arena::new();
    let bound = bind(&arena, "declare let x: string;\nswitch (x) {\n  case \"a\": break;\n}\nx;");
    let flow = bound.result.flow();
    let after = bound.flow_of(bound.identifier("x", 2)).expect("has flow");
    // One antecedent for `break`, one for the empty clause range that says the
    // subject matched nothing. Without the second, `switch` would look
    // exhaustive when it is not.
    let empty_range = flow.antecedents(after).any(|antecedent| {
        flow.switch_clause(antecedent).is_some_and(tsr_binder::SwitchClause::is_empty)
    });
    assert!(empty_range, "the no-match path is on the graph");
}

#[test]
fn a_case_that_falls_through_records_where_it_falls_to() {
    let arena = Arena::new();
    let bound =
        bind(&arena, "declare let x: number;\nswitch (x) {\n  case 1: x; \n  case 2: break;\n}");
    let first_clause = bound.first(SyntaxKind::CaseClause);
    assert!(
        bound.result.fallthrough_flow(first_clause).is_some(),
        "a clause with statements and no `break` falls through"
    );

    let bound =
        bind(&arena, "declare let x: number;\nswitch (x) {\n  case 1: break;\n  case 2: break;\n}");
    let first_clause = bound.first(SyntaxKind::CaseClause);
    assert_eq!(bound.result.fallthrough_flow(first_clause), None, "a `break` ends the clause");
}

#[test]
fn a_mutation_in_a_try_block_reaches_the_catch_block() {
    let arena = Arena::new();
    let bound = bind(&arena, "declare let x: string | undefined;\ntry { x = \"a\"; } catch { x; }");
    let flow = bound.result.flow();
    let in_catch = bound.flow_of(bound.identifier("x", 2)).expect("has flow");
    // Either the exception label survived as a junction of "before the
    // assignment" and "after it", or it collapsed — but the assignment must be
    // reachable from the catch either way, because that is the whole point of
    // routing mutations to the exception label.
    let reaches_assignment = flow.flags(in_catch).contains(FlowFlags::ASSIGNMENT)
        || flow
            .antecedents(in_catch)
            .any(|antecedent| flow.flags(antecedent).contains(FlowFlags::ASSIGNMENT));
    assert!(reaches_assignment, "the catch block sees the try block's assignment");
}

#[test]
fn a_finally_block_is_reached_from_the_try_and_from_the_catch() {
    let arena = Arena::new();
    let bound =
        bind(&arena, "declare let x: number;\ntry { x = 1; } catch { x = 2; } finally { x; }");
    let flow = bound.result.flow();
    let in_finally = bound.flow_of(bound.identifier("x", 3)).expect("has flow");
    let antecedents: Vec<_> = flow.antecedents(in_finally).collect();
    assert!(
        antecedents.len() >= 2,
        "the finally block merges at least the try and catch paths, got {antecedents:?}"
    );
}

#[test]
fn a_try_that_always_returns_makes_the_code_after_it_unreachable() {
    let arena = Arena::new();
    let bound = bind(&arena, "function f() {\n  try { return 1; } finally { 2; }\n  let x = 3;\n}");
    let statement = bound.first(SyntaxKind::VariableStatement);
    assert!(
        bound.result.facts(statement).contains(NodeFacts::UNREACHABLE),
        "the finally completes, but nothing follows the try statement"
    );
}

#[test]
fn a_conditional_expression_narrows_each_arm() {
    let arena = Arena::new();
    let bound = bind(&arena, "declare let x: string | undefined;\nconst y = x ? x : \"\";");
    // #0 is the declaration's own name, #1 the condition, #2 the true arm.
    let when_true = bound.identifier("x", 2);
    assert_eq!(bound.chain(when_true), vec![FlowFlags::TRUE_CONDITION, FlowFlags::START]);
}

#[test]
fn a_dotted_call_statement_is_a_candidate_assertion() {
    let arena = Arena::new();
    let bound = bind(&arena, "declare const a: { f(): void };\ndeclare let x: number;\na.f();\nx;");
    let flow = bound.result.flow();
    let read = bound.identifier("x", 1);
    assert_eq!(
        bound.chain(read),
        vec![FlowFlags::CALL, FlowFlags::START],
        "the call sits on the flow so the checker can ask whether it asserts"
    );
    let call = bound.flow_of(read).expect("the read has a flow node");
    assert_eq!(flow.node(call), Some(bound.first(SyntaxKind::CallExpression)));
}

#[test]
fn an_indexed_call_statement_is_not() {
    let arena = Arena::new();
    let bound =
        bind(&arena, "declare const a: { f(): void }[];\ndeclare let x: number;\na[0].f();\nx;");
    // `a[0].f` is not a dotted name, so it cannot be an assertion signature and
    // nothing is recorded for it.
    let read = bound.identifier("x", 1);
    assert_eq!(bound.chain(read), vec![FlowFlags::START]);
}

#[test]
fn a_push_call_is_recorded_as_an_array_mutation() {
    let arena = Arena::new();
    let bound = bind(&arena, "declare const a: number[];\na.push(1);\nlet x = 1;");
    let chain = bound.chain(bound.identifier("x", 0));
    assert!(
        chain.contains(&FlowFlags::ARRAY_MUTATION),
        "`a.push(…)` may change what `a` holds: {chain:?}"
    );
}

#[test]
fn each_function_gets_its_own_graph() {
    let arena = Arena::new();
    let bound = bind(&arena, "declare let x: string | undefined;\nif (x) { function f() { x; } }");
    let inside_function = bound.identifier("x", 2);
    // The narrowing from the enclosing `if` does not carry into the function,
    // because the function may be called later — that is what a separate start
    // node encodes.
    assert_eq!(bound.chain(inside_function), vec![FlowFlags::START]);
}

#[test]
fn an_immediately_invoked_function_stays_in_the_containing_flow() {
    let arena = Arena::new();
    let bound =
        bind(&arena, "declare let x: string | undefined;\nif (x) { (function () { x; })(); }");
    let inside = bound.identifier("x", 2);
    assert_eq!(
        bound.chain(inside),
        vec![FlowFlags::TRUE_CONDITION, FlowFlags::START],
        "an IIFE runs where it is written, so it keeps the narrowing around it"
    );
}

#[test]
fn a_destructuring_declaration_assigns_to_each_name() {
    let arena = Arena::new();
    let bound = bind(&arena, "declare const o: { a: number; b: number };\nconst { a, b } = o;\na;");
    let flow = bound.result.flow();
    let read = bound.identifier("a", 2);
    let mut assignments = 0;
    let mut current = bound.flow_of(read);
    while let Some(id) = current {
        if flow.flags(id).contains(FlowFlags::ASSIGNMENT) {
            assignments += 1;
        }
        current = flow.antecedent(id);
    }
    assert_eq!(assignments, 2, "one assignment node per name the pattern introduces");
}

#[test]
fn a_parameter_default_may_or_may_not_run() {
    let arena = Arena::new();
    let bound = bind(&arena, "declare let x: number;\nfunction f(a = (x = 1)) { a; }");
    let flow = bound.result.flow();
    let body = bound.identifier("a", 1);
    let id = bound.flow_of(body).expect("has flow");
    // The default's assignment is on one branch of a junction, not on the only
    // path: calling `f(2)` never runs it.
    let merged = flow.flags(id).intersects(FlowFlags::LABEL)
        || flow.antecedent(id).is_some_and(|a| flow.flags(a).intersects(FlowFlags::LABEL));
    assert!(merged, "the parameter list merges 'the default ran' with 'it did not'");
}

#[test]
fn the_flow_graph_can_be_read_from_several_threads() {
    let arena = Arena::new();
    let bound = bind(&arena, "declare let x: string | undefined;\nif (x) { x; } else { x; }");
    let result = &bound.result;
    let target = bound.identifier("x", 2);

    let seen: Vec<Option<FlowId>> = std::thread::scope(|scope| {
        let handles: Vec<_> = (0..4).map(|_| scope.spawn(move || result.flow_of(target))).collect();
        handles.into_iter().map(|h| h.join().expect("thread panicked")).collect()
    });
    assert!(seen.iter().all(|f| *f == seen[0]), "threads disagreed: {seen:?}");
}

#[test]
fn every_statement_in_reachable_code_has_a_flow_node() {
    let arena = Arena::new();
    let bound = bind(
        &arena,
        "let a = 1;\nif (a) { a = 2; } else { a = 3; }\nfor (let i = 0; i < 3; i++) { a += i; }\n\
         while (a) { a--; }\ndo { a++; } while (a);\ntry { a = 4; } catch (e) { a = 5; } finally { a = 6; }\n\
         switch (a) { case 1: break; default: break; }\nlabel: { break label; }",
    );
    let mut missing = Vec::new();
    for id in bound.ids() {
        let kind = bound.nodes().kind(id);
        let is_statement = (SyntaxKind::FIRST_STATEMENT as u16) <= (kind as u16)
            && (kind as u16) <= (SyntaxKind::LAST_STATEMENT as u16);
        if is_statement
            && !bound.result.facts(id).contains(NodeFacts::UNREACHABLE)
            && bound.flow_of(id).is_none()
        {
            missing.push((id, kind));
        }
    }
    assert!(missing.is_empty(), "reachable statements without a flow node: {missing:?}");
    assert_ne!(bound.source_file(), NodeId::new(u32::MAX - 1), "the file was bound");
}
