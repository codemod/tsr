//! Native `checkExportAssignment` and `checkGrammarModuleElementContext`, pinned 5b1047d.
use tsr_ast::{NodeId, NodeTable};
use tsr_checker::{Checker, check::FileContext, resolution::ModuleHost};
use tsr_core::{Arena, Span};

struct Source<'a> {
    file: NodeId,
    nodes: &'a NodeTable,
    text: &'a str,
}

impl ModuleHost for Source<'_> {
    fn resolved_module(&self, _: NodeId, _: &str) -> Option<NodeId> {
        None
    }

    fn module_resolution_found(&self, _: NodeId, _: &str) -> bool {
        false
    }

    fn source_text(&self, file: NodeId, nodes: &NodeTable) -> Option<&str> {
        (file == self.file && std::ptr::eq(nodes, self.nodes)).then_some(self.text)
    }
}

fn diagnostics(source: &str, has_parse_errors: bool) -> Vec<(String, Span, String)> {
    let arena = Arena::new();
    let parsed = tsr_parser::parse(&arena, source);
    assert!(parsed.diagnostics.is_empty(), "control must parse");
    let bound = tsr_binder::bind(
        &arena,
        parsed.source_file,
        &parsed.nodes,
        tsr_binder::FileInfo { name: "test.ts", text: source },
    );
    let file = parsed.source_file.node_id.expect("source identity");
    let host = Source { file, nodes: &parsed.nodes, text: source };
    let mut checker =
        Checker::with_module_host(&bound, &parsed.nodes, &parsed.node_map, Some(&host));
    checker.check_source_file(file, FileContext { ambient: false, has_parse_errors });
    checker.diagnostics().iter().map(|(_, d)| (d.code(), d.span, d.text())).collect()
}

#[test]
fn misplaced_exports_report_the_first_token_and_do_not_cascade_namespace_errors() {
    for (source, code, message) in [
        (
            "declare const value: number; function f() { /* trivia */ export = value; }",
            "TS1231",
            "An export assignment must be at the top level of a file or module declaration.",
        ),
        (
            "declare const value: number; namespace N { function f() { export default value; } }",
            "TS1258",
            "A default export must be at the top level of a file or module declaration.",
        ),
    ] {
        let start = u32::try_from(source.find("export").unwrap()).unwrap();
        assert_eq!(
            diagnostics(source, false),
            [(code.to_owned(), Span::new(start, start + 6), message.to_owned())],
            "{source}",
        );
        assert_eq!(diagnostics(source, true), [], "grammar suppressed, context still rejected");
    }
}

#[test]
fn namespace_exports_report_whole_assignment_and_return_before_modifier_errors() {
    for (source, assignment, code, message) in [
        (
            "declare const value: number; namespace N { export = value; }",
            "export = value;",
            "TS1063",
            "An export assignment cannot be used in a namespace.",
        ),
        (
            "declare const value: number; namespace N.M { export default value; }",
            "export default value;",
            "TS1319",
            "A default export can only be used in an ECMAScript-style module.",
        ),
    ] {
        let start = u32::try_from(source.find(assignment).unwrap()).unwrap();
        assert_eq!(
            diagnostics(source, false),
            [(
                code.to_owned(),
                Span::new(start, start + u32::try_from(assignment.len()).unwrap()),
                message.to_owned(),
            )],
            "{source}",
        );
        assert_eq!(diagnostics(source, true), diagnostics(source, false), "not a grammar error");
    }
}

#[test]
fn source_file_and_ambient_module_exports_remain_legal() {
    for source in [
        "declare const value: number; export = value;",
        "declare const value: number; export default value;",
        "declare module \"m\" { const value: number; export = value; }",
        "declare module \"m\" { const value: number; export default value; }",
    ] {
        assert_eq!(diagnostics(source, false), [], "{source}");
    }
}

#[test]
fn export_assignment_conflicts_follow_resolved_value_and_namespace_exports() {
    // Pinned native 5b1047d: type-only and empty exports can coexist with
    // export=, unless the assigned namespace also exports types/namespaces.
    // Exported aliases and genuine ambient modules still reject mixed values.
    for (source, conflict) in [
        ("export const value = 1; export = value;", true),
        (
            "declare module \"m\" { export const other: number; const value: number; export = value; }",
            true,
        ),
        ("namespace N { export const value = 1; } export import Alias = N; export = N;", true),
        (
            "export type Other = { x: string }; namespace Value { export interface Member { y: number } } export = Value;",
            true,
        ),
        ("type Other = { x: string }; class Value {} export = Value; export { Other };", false),
        ("declare const value: number; export {}; export = value;", false),
        (
            "export type Other = { x: string }; namespace Value { export const member = 1; } export = Value;",
            false,
        ),
    ] {
        let reported = diagnostics(source, false);
        if conflict {
            let start = source.find("export =").unwrap();
            let end = start + source[start..].find(';').unwrap() + 1;
            assert_eq!(
                reported,
                [(
                    "TS2309".to_owned(),
                    Span::new(u32::try_from(start).unwrap(), u32::try_from(end).unwrap()),
                    "An export assignment cannot be used in a module with other exported elements."
                        .to_owned(),
                )],
                "{source}",
            );
        } else {
            assert_eq!(reported, [], "{source}");
        }
    }
}
