//! Native 5b1047d real-Program masks/consumers archived in object-facts-wave27.
//! Mode selection changes; category and primitive gaps stay held.
//! Bare receiver queries now share native runtime receiver flow (wave47).
use crate::Checker;
use tsr_ast::{Node, NodeId, SyntaxKind};

fn with_checker(source: &str, test: impl Fn(&mut Checker<'_, '_>, bool)) {
    for strict in [false, true] {
        for null in [false, true] {
            for precheck in [false, true] {
                let arena = tsr_core::Arena::new();
                let parsed = tsr_parser::parse(&arena, source);
                assert!(parsed.diagnostics.is_empty());
                let root = parsed.source_file.node_id.expect("registered file");
                let bound = tsr_binder::bind(
                    &arena,
                    parsed.source_file,
                    &parsed.nodes,
                    tsr_binder::FileInfo { name: "object-facts.ts", text: source },
                );
                let mut checker = Checker::new(&bound, &parsed.nodes, &parsed.node_map);
                checker.apply_compiler_options(&tsr_core::CompilerOptions {
                    strict: tsr_core::Tristate::from_bool(strict),
                    strict_null_checks: tsr_core::Tristate::from_bool(null),
                    ..Default::default()
                });
                if precheck {
                    checker.check_source_file(
                        root,
                        crate::check::FileContext { ambient: false, has_parse_errors: false },
                    );
                }
                test(&mut checker, null);
                test(&mut checker, null);
            }
        }
    }
}

fn alias_type(checker: &mut Checker<'_, '_>, name: &str) -> crate::TypeId {
    let body = (0..checker.nodes.len())
        .map(|index| NodeId::new(u32::try_from(index).expect("node count fits")))
        .find_map(|id| match checker.node_map.get(id) {
            Some(Node::TypeAliasDeclaration(alias))
                if alias.name.is_some_and(|n| n.text == name) =>
            {
                alias.r#type
            }
            _ => None,
        })
        .expect("named alias");
    checker.get_type_from_type_node(body)
}

#[test]
fn object_and_function_facts_select_only_the_null_mode() {
    // Expectations are native numeric aggregates, not computed from Rust facts.
    for (prefix, body, strict, loose) in [
        ("", r#"{p: "left"; q: 17}"#, 7_888_800, 16_736_160),
        ("", "object", 7_888_800, 16_736_160),
        ("interface Empty {}", "Empty", 7_888_800, 16_736_160),
        ("", r#"(p: "right") => 23"#, 7_880_640, 16_728_000),
        (r#"interface F {(p: "left"): 17; q: "right"}"#, "F", 7_880_640, 16_728_000),
        (r#"interface C {new(p: "left"): {q: 17}}"#, "C", 7_880_640, 16_728_000),
        (r#"class C {p: "left" = "left"; static q: 17 = 17;}"#, "C", 7_888_800, 16_736_160),
        (r#"class C {p: "left" = "left"; static q: 17 = 17;}"#, "typeof C", 7_880_640, 16_728_000),
    ] {
        let source = format!("{prefix} type Current = {body};");
        with_checker(&source, |checker, null| {
            let ty = alias_type(checker, "Current");
            let bits = checker.get_type_facts(ty).bits();
            assert_eq!(bits, if null { strict } else { loose }, "{body}");
            assert_eq!(bits & (3 << 24), 0, "never genuine null/undefined");
            assert_eq!(bits & (7 << 19), 7 << 19, "NE facts retained");
            assert_eq!(bits & (7 << 16), if null { 0 } else { 7 << 16 });
            assert_eq!(bits & (3 << 22), if null { 1 << 22 } else { 3 << 22 });
        });
    }
}

#[test]
fn empty_primitive_composite_and_inherited_category_boundaries_stay_held() {
    for (prefix, body, strict, loose) in [
        ("", "{}", 16_318_463, 16_777_215),
        ("", r#""left""#, 7_929_345, 16_776_705),
        ("", r#"string & {p: "left"}"#, 16_317_953, 16_776_705),
        ("", r#"{p: "left"} | {q: 17}"#, 7_888_800, 16_736_160),
        ("", r#"{p: "left"} & {q: 17}"#, 7_888_800, 16_736_160),
        (r#"type F = (p: "left") => 17;"#, r#"F & {q: "right"}"#, 7_880_640, 16_728_000),
        (
            r#"interface Base {(p: "left"): 17} interface Child extends Base {q: 23}"#,
            "Child",
            7_888_800,
            16_736_160,
        ),
        (
            r#"interface Base {new(p: "left"): {q: 17}} interface Child extends Base {p: "right"}"#,
            "Child",
            7_888_800,
            16_736_160,
        ),
    ] {
        let source = format!("{prefix} type Current = {body};");
        with_checker(&source, |checker, null| {
            let ty = alias_type(checker, "Current");
            assert_eq!(
                checker.get_type_facts(ty).bits(),
                if null { strict } else { loose },
                "{body}"
            );
        });
    }
}

#[test]
fn ordinary_and_runtime_this_false_branches_and_negation_read_native_facts() {
    let source = r#"function f(this: {p: "left"; q: 17}, x: {p: "right"; q: 23}) {
        if (!x) {type X = typeof x;}
        if (!this) {type Held = typeof this; this;}
        const nx = !x; const nt = !this;
    }"#;
    with_checker(source, |checker, null| {
        let query = alias_type(checker, "X");
        assert_eq!(
            checker.type_to_string(query),
            if null { "never" } else { r#"{ p: "right"; q: 23; }"# }
        );
        let receiver_query = alias_type(checker, "Held");
        assert_eq!(
            checker.type_to_string(receiver_query),
            if null { "never" } else { r#"{ p: "left"; q: 17; }"# }
        );
        for index in 0..checker.nodes.len() {
            let id = NodeId::new(u32::try_from(index).expect("node count fits"));
            let kind = checker.nodes.kind(id);
            if kind == SyntaxKind::PrefixUnaryExpression {
                let ty = checker.check_expression_at_node(id);
                // Loose ObjectFacts include Falsy, so `!object` is boolean.
                assert_eq!(checker.type_to_string(ty), if null { "false" } else { "boolean" });
            } else if kind == SyntaxKind::ThisKeyword
                && checker
                    .nodes
                    .parent(id)
                    .is_some_and(|p| checker.nodes.kind(p) == SyntaxKind::ExpressionStatement)
            {
                let ty = checker.check_expression_at_node(id);
                assert_eq!(
                    checker.type_to_string(ty),
                    if null { "never" } else { r#"{ p: "left"; q: 17; }"# }
                );
            }
        }
    });
}

#[test]
fn object_fact_mode_selection_preserves_wrong_assignment_anchors() {
    let source = r#"function f(this: {p: "left"; q: 17}, x: {p: "right"; q: 23}) {type T = typeof this; type X = typeof x; const bad: number = x.p; const badThis: number = this.p;}"#;
    with_checker(source, |checker, _| {
        let root = (0..checker.nodes.len())
            .map(|index| NodeId::new(u32::try_from(index).expect("node count fits")))
            .find(|&id| checker.nodes.kind(id) == SyntaxKind::SourceFile)
            .expect("source file");
        // Precheck and repeated observations must not walk reporters again.
        if checker.diagnostics().is_empty() {
            checker.check_source_file(
                root,
                crate::check::FileContext { ambient: false, has_parse_errors: false },
            );
        }
        let diagnostics: Vec<_> = checker
            .diagnostics()
            .iter()
            .map(|(_, d)| (d.message.code(), d.span.start, d.span.end - d.span.start))
            .collect();
        assert_eq!(diagnostics, [(2322, 109, 3), (2322, 134, 7)]);
        let query = alias_type(checker, "T");
        assert_eq!(checker.type_to_string(query), r#"{ p: "left"; q: 17; }"#);
        assert_ne!(query, checker.intrinsics.any);
    });
}
