//! Relation reports carry native related information located in its own file:
//! `reportUnmatchedProperty`'s TS2728 shared by every chain link, and
//! `elaborateElement`'s TS6500/TS6501 on the direct element report only.
use tsr_ast::{NodeId, NodeTable};
use tsr_checker::{Checker, check::FileContext, resolution::ModuleHost};
use tsr_core::Arena;
use tsr_diagnostics::Diagnostic;

struct OneFile<'s> {
    root: NodeId,
    text: &'s str,
}

impl ModuleHost for OneFile<'_> {
    fn resolved_module(&self, _importing_file: NodeId, _specifier: &str) -> Option<NodeId> {
        None
    }
    fn module_resolution_found(&self, _importing_file: NodeId, _specifier: &str) -> bool {
        false
    }
    fn source_text(&self, file: NodeId, _nodes: &NodeTable) -> Option<&str> {
        (file == self.root).then_some(self.text)
    }
    fn file_path(&self, file: NodeId) -> Option<String> {
        (file == self.root).then(|| "/a.ts".to_string())
    }
}

/// One reported diagnostic: its code, its related `(code, start, file)`
/// records, and the related codes seen on each link of its chain.
type Reported = (u32, Vec<(u32, u32, String)>, Vec<Vec<u32>>);

fn links(d: &Diagnostic, out: &mut Vec<Vec<u32>>) {
    for child in d.message_chain() {
        out.push(child.related_information().iter().map(|r| r.message.code()).collect());
        links(child, out);
    }
}

fn report(source: &str) -> Vec<Reported> {
    let arena = Arena::new();
    let parsed = tsr_parser::parse(&arena, source);
    assert!(parsed.diagnostics.is_empty());
    let root = parsed.source_file.node_id.unwrap();
    let bound = tsr_binder::bind(
        &arena,
        parsed.source_file,
        &parsed.nodes,
        tsr_binder::FileInfo { name: "/a.ts", text: source },
    );
    let host = OneFile { root, text: source };
    let mut checker =
        Checker::with_module_host(&bound, &parsed.nodes, &parsed.node_map, Some(&host));
    checker.check_source_file(root, FileContext { ambient: false, has_parse_errors: false });
    checker
        .diagnostics()
        .iter()
        .map(|(_, d)| {
            let related = d
                .related_information()
                .iter()
                .map(|r| {
                    (r.message.code(), r.span.start, r.file().unwrap().file_name().to_string())
                })
                .collect();
            let mut chain = Vec::new();
            links(d, &mut chain);
            (d.message.code(), related, chain)
        })
        .collect()
}

#[test]
fn single_missing_property_points_at_its_declaration_on_every_chain_link() {
    let source = "interface I { a: number; b: string }\n\
                  declare let y: { a: number };\nlet z: I = y;\n\
                  declare let q: { p: { a: number } };\nlet r: { p: I } = q;\n\
                  declare let e: {};\nlet t: I = e;";
    let b = u32::try_from(source.find("b: string").unwrap()).unwrap();
    assert_eq!(
        report(source),
        [
            (2741, vec![(2728, b, "/a.ts".into())], vec![]),
            (2322, vec![(2728, b, "/a.ts".into())], vec![vec![2728], vec![2728]]),
            // Two missing properties (TS2739) name no declaration.
            (2739, vec![], vec![]),
        ]
    );
}

#[test]
fn elaborated_member_names_expected_property_or_index_signature() {
    let source = "interface I { a: number; b: string }\n\
                  const x: I = { a: 1, b: 2 };\n\
                  const w: { [k: string]: number } = { r: \"s\" };\n\
                  const n: { o: I } = { o: { a: \"x\", b: 1 } };";
    let at = |text: &str| u32::try_from(source.find(text).unwrap()).unwrap();
    let file = || "/a.ts".to_string();
    assert_eq!(
        report(source),
        [
            (2322, vec![(6500, at("b: string"), file())], vec![]),
            (2322, vec![(6501, at("[k: string]: number"), file())], vec![]),
            // The innermost report names `I`'s member; the elaborated outer
            // member `o` adds nothing.
            (2322, vec![(6500, at("a: number"), file())], vec![]),
            (2322, vec![(6500, at("b: string"), file())], vec![]),
        ]
    );
}
