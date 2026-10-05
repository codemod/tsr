//! Native 5b1047d leaf module aliases: accessibility uses the requested meaning.
use tsr_ast::{Expression, Node, NodeId, SyntaxKind};
use tsr_conformance::{TestCase, types_producer};
use tsr_core::{Arena, Idx};

fn reads(
    body: &str,
    reversed_declarations: bool,
    precheck: bool,
    reversed_queries: bool,
    modifier: &str,
) -> Vec<String> {
    let aliases = if reversed_declarations {
        format!(
            "{modifier}import first = require(\"./mod\");\n{modifier}import second = require(\"./mod\");"
        )
    } else {
        format!(
            "{modifier}import second = require(\"./mod\");\n{modifier}import first = require(\"./mod\");"
        )
    };
    let source = format!(
        "// @target: es2015\n// @module: commonjs\n// @filename: mod.ts\nexport const token = 17;\nexport class Thing {{ value = 1; }}\n// @filename: other.ts\nexport const token = 29;\n// @filename: use.ts\n{aliases}\nimport foreign = require(\"./other\");\n{body}"
    );
    let case = TestCase::parse("probe/module-alias-accessibility", "fixture.ts", &source);
    let arena = Arena::new();
    let program = types_producer::program_for_case(&arena, &case);
    let file = program.source_file("use.ts").expect("consumer");
    let file_id = file.source_file().node_id.expect("file id");
    let mut checker = types_producer::configured_checker(&program);
    checker.set_checked_files(vec![file_id]);
    let module = program
        .binder()
        .symbol_of(program.source_file("mod.ts").expect("module").source_file().node_id.unwrap())
        .expect("module symbol");
    let thing = *program.binder().symbols().get(module).exports.get("Thing").expect("class");
    let instance = checker.get_declared_type_of_symbol(thing);
    if precheck {
        checker.check_source_file(
            file_id,
            tsr_checker::check::FileContext { ambient: false, has_parse_errors: false },
        );
    }
    let mut sites = (0..program.nodes().len())
        .filter_map(|index| {
            let id = NodeId::from_usize(index);
            let Node::Identifier(identifier) = program.node_map().get(id)? else { return None };
            let parent = program.nodes().parent(id)?;
            if program.nodes().kind(parent) != SyntaxKind::ExpressionStatement {
                return None;
            }
            let mut root = id;
            while let Some(parent) = program.nodes().parent(root) {
                root = parent;
            }
            (root == file_id).then_some((id, identifier))
        })
        .collect::<Vec<_>>();
    sites.sort_by_key(|(id, _)| program.nodes().span(*id).start);
    if reversed_queries {
        sites.reverse();
    }
    let mut passes = Vec::new();
    for _ in 0..2 {
        let mut results = sites
            .iter()
            .map(|(id, identifier)| {
                // Written references have a separate display contract; exercise the
                // canonical class container at the same native-correlated sites.
                let ty = if identifier.text == "value" {
                    instance
                } else {
                    checker.check_expression(Expression::Identifier(identifier))
                };
                checker.type_to_string_at(ty, *id).expect("printable type")
            })
            .collect::<Vec<_>>();
        if reversed_queries {
            results.reverse();
        }
        passes.push(results);
    }
    assert_eq!(passes[0], passes[1], "cold/warm drift: {body}");
    if !modifier.is_empty() && precheck {
        assert!(checker.diagnostics().is_empty(), "native exported controls have no diagnostics");
    }
    passes.remove(0)
}

#[test]
fn module_values_and_type_containers_use_distinct_meanings() {
    for checked in [false, true] {
        for reversed_queries in [false, true] {
            for (body, expected) in [
                ("function f(second: number) { first; }", vec!["typeof first"]),
                ("function f() { interface second {} first; }", vec!["typeof second"]),
                (
                    "function f(second: number) { let value!: first.Thing; value; }",
                    vec!["second.Thing"],
                ),
                (
                    "function f() { interface second {} let value!: first.Thing; value; }",
                    vec!["second.Thing"],
                ),
                (
                    "namespace N { namespace second { export interface Other {} } let value!: first.Thing; value; first; }",
                    vec!["first.Thing", "typeof second"],
                ),
            ] {
                assert_eq!(reads(body, false, checked, reversed_queries, ""), expected, "{body}");
            }
        }
    }
}

#[test]
fn accessible_aliases_keep_scope_order_identity_and_fallback() {
    for checked in [false, true] {
        for reversed_queries in [false, true] {
            for reversed_declarations in [false, true] {
                let preferred = if reversed_declarations { "first" } else { "second" };
                for (body, expected) in [
                    (
                        "first; let value!: first.Thing; value;",
                        vec![format!("typeof {preferred}"), format!("{preferred}.Thing")],
                    ),
                    // These external imports are parsed and resolved inside a namespace;
                    // native also reports TS1147. Pin only the helper's raw naming contract,
                    // not diagnostic admission or the unresolved internal-alias producer.
                    (
                        "namespace N { import innerAlias = require(\"./mod\"); first; let value!: first.Thing; value; }",
                        vec!["typeof innerAlias".to_string(), "innerAlias.Thing".to_string()],
                    ),
                    // The immediate same-symbol hit stops before the outer alias of the same name.
                    (
                        "namespace N { import second = require(\"./mod\"); second; let value!: second.Thing; value; }",
                        vec!["typeof second".to_string(), "second.Thing".to_string()],
                    ),
                    (
                        "namespace N { import second = require(\"./other\"); first; let value!: first.Thing; value; }",
                        vec!["typeof first".to_string(), "first.Thing".to_string()],
                    ),
                    (
                        "const captured = first; function f(second: number, first: string) { captured; }",
                        vec!["typeof import(\"./mod\")".to_string()],
                    ),
                ] {
                    assert_eq!(
                        reads(body, reversed_declarations, checked, reversed_queries, ""),
                        expected,
                        "{body}"
                    );
                }
            }
        }
    }
}

#[test]
fn exports_only_external_aliases_follow_native_table_order_and_meaning() {
    for checked in [false, true] {
        for reverse_queries in [false, true] {
            for reverse_declarations in [false, true] {
                let preferred = if reverse_declarations { "first" } else { "second" };
                for (body, expected) in [
                    (
                        "first; let value!: first.Thing; value;",
                        vec![format!("typeof {preferred}"), format!("{preferred}.Thing")],
                    ),
                    (
                        "function f(second: number) { first; let value!: first.Thing; value; }",
                        vec!["typeof first".to_string(), format!("{preferred}.Thing")],
                    ),
                    (
                        "function f() { interface second {} first; let value!: first.Thing; value; }",
                        vec![format!("typeof {preferred}"), format!("{preferred}.Thing")],
                    ),
                    (
                        "import local = require(\"./mod\"); first; let value!: first.Thing; value;",
                        vec!["typeof local".to_string(), "local.Thing".to_string()],
                    ),
                    (
                        "export { first as hidden }; first; let value!: first.Thing; value;",
                        vec![format!("typeof {preferred}"), format!("{preferred}.Thing")],
                    ),
                    (
                        "const captured = first; function f(second: number, first: string) { captured; }",
                        vec!["typeof import(\"./mod\")".to_string()],
                    ),
                ] {
                    assert_eq!(
                        reads(body, reverse_declarations, checked, reverse_queries, "export "),
                        expected,
                        "native exported-alias control: {body}"
                    );
                }
            }
        }
    }
}
