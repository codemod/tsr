//! Complete identity and ownership checks for worker AST publication.
use std::fmt::Write as _;
use tsr_ast::{Node, NodeId, NodeMap, NodeTable};
use tsr_core::Arena;
use tsr_parser::{ParseOptions, ParsedFile, ScriptKind, parse_into};

#[test]
fn reversed_completion_publishes_every_identity_like_serial_parsing() {
    let mut large = String::new();
    for i in 0..80 {
        writeln!(large, "const x{i} = {{ ['\\u0061']: {i}, [-(0x10)]: 'a\\nb' }};").unwrap();
    }
    let inputs = [
        ("/** @typedef {{value: string}} Value */\n/** @param {Value} v */ function f(v) { return v.value; }".to_owned(), ScriptKind::TypeScript),
        (large, ScriptKind::TypeScript),
        ("const x = <div data-name='a'><span>{1}</span></div>;".to_owned(), ScriptKind::Tsx),
        ("{\"a\": [1, true, null], \"b\": \"x\\ny\"}".to_owned(), ScriptKind::Json),
        ("function broken( { return ; const x =".to_owned(), ScriptKind::TypeScript),
        (String::new(), ScriptKind::TypeScript),
    ];
    let serial_arena = Arena::new();
    let mut serial_nodes = NodeTable::new();
    let mut serial_map = NodeMap::new();
    let mut serial_files = Vec::new();
    for (source, kind) in &inputs {
        let source = serial_arena.alloc_str(source);
        serial_files.push(parse_into(
            &serial_arena,
            source,
            ParseOptions { script_kind: *kind, ..Default::default() },
            &mut serial_nodes,
            &mut serial_map,
        ));
    }

    // Completion order is deliberately reversed, including the largest file,
    // syntax recovery, empty input and JSDoc outside the main syntax walk.
    let mut completed = Vec::new();
    for (source, kind) in inputs.iter().rev() {
        completed.push(
            std::thread::spawn({
                let source = source.clone();
                let kind = *kind;
                move || ParsedFile::parse_with_script_kind(source, kind)
            })
            .join()
            .unwrap(),
        );
    }
    let arena = Arena::new();
    let mut nodes = NodeTable::new();
    let mut map = NodeMap::new();
    let mut published = Vec::new();
    for (source, _) in &inputs {
        let private = completed.pop().unwrap();
        let source = arena.alloc_str(source);
        published.push(private.publish(&arena, source, &mut nodes, &mut map));
        // The private arena is gone before any published consumer reads it.
    }
    assert_eq!(nodes.len(), serial_nodes.len());
    assert_eq!(map.len(), serial_map.len());
    for index in 0..nodes.len() {
        let id = NodeId::new(u32::try_from(index).unwrap());
        assert_eq!(nodes.kind(id), serial_nodes.kind(id));
        assert_eq!(nodes.span(id), serial_nodes.span(id));
        assert_eq!(nodes.parent(id), serial_nodes.parent(id));
        assert_eq!(nodes.flags(id), serial_nodes.flags(id));
        assert_eq!(map.get(id).unwrap().node_id(), Some(id));
        // Generated fields include every typed child, token, list and decoded
        // string, not only children visited by the ordinary visitor.
        assert_eq!(
            format!("{:?}", map.get(id)),
            format!("{:?}", serial_map.get(id)),
            "node {index}"
        );
    }
    for (actual, expected) in published.iter().zip(serial_files) {
        assert_eq!(actual.node_range, expected.node_range);
        assert_eq!(format!("{:?}", actual.diagnostics), format!("{:?}", expected.diagnostics));
        assert_eq!(format!("{:?}", actual.jsdoc), format!("{:?}", expected.jsdoc));
        assert_eq!(
            Node::SourceFile(actual.source_file).node_id(),
            Node::SourceFile(expected.source_file).node_id()
        );
        assert_eq!(
            format!("{:?}", actual.file_references),
            format!("{:?}", expected.file_references)
        );
    }
}
