//! Native nil/allocated-empty type argument lists and raw list extents.
use tsr_ast::{Node, NodeId, NodeMap, NodeTable};
use tsr_core::{Arena, Span};
use tsr_parser::{ParseOptions, ParsedFile, parse_into};

#[test]
fn every_type_argument_host_preserves_raw_delimiter_exclusive_ranges() {
    for (source, expected_kind, expected) in [
        ("type Q = typeof f;", "query", None),
        ("type Q = typeof f<>;", "query", Some("")),
        ("type Q = typeof f< /*a*/ T, /*b*/ >;", "query", Some(" /*a*/ T,")),
        ("f<>;", "instantiation", Some("")),
        ("f</*c*/>;", "instantiation", Some("")),
        ("f</*c*/>();", "call", Some("")),
        ("f</*c*/>`x`;", "tagged", Some("")),
        ("new f</*c*/>();", "new", Some("")),
        ("f< /*a*/ T, /*b*/ >;", "instantiation", Some(" /*a*/ T,")),
        ("f< /*a*/ T, /*b*/ >();", "call", Some(" /*a*/ T,")),
        ("f< /*a*/ T, /*b*/ >`x`;", "tagged", Some(" /*a*/ T,")),
        ("new f< /*a*/ T, /*b*/ >();", "new", Some(" /*a*/ T,")),
        ("class C extends f< /*a*/ T, /*b*/ > {}", "instantiation", Some(" /*a*/ T,")),
        ("/** @extends {f<T>} */ class C {}", "instantiation", Some("T")),
        ("type Q = typeof import('m').f< /*a*/ T, /*b*/ >;", "import", Some(" /*a*/ T,")),
    ] {
        let arena = Arena::new();
        let mut nodes = NodeTable::new();
        let mut map = NodeMap::new();
        // Repeated parse_into and private publication must use the same table,
        // with a nonzero publication base and unchanged file-relative offsets.
        parse_into(&arena, "const prefix = 0;", ParseOptions::default(), &mut nodes, &mut map);
        let private = ParsedFile::parse(source.to_owned());
        let published = private.publish(&arena, source, &mut nodes, &mut map);
        drop(private);
        let mut hosts = Vec::new();
        for index in published.node_range.clone() {
            let id = NodeId::new(index);
            let kind = match map.get(id).unwrap() {
                Node::TypeQueryNode(_) => "query",
                Node::ExpressionWithTypeArguments(_) => "instantiation",
                Node::CallExpression(_) => "call",
                Node::TaggedTemplateExpression(_) => "tagged",
                Node::NewExpression(_) => "new",
                Node::ImportTypeNode(_) => "import",
                _ => continue,
            };
            if kind == expected_kind {
                let span = nodes.type_argument_list_span(id);
                if kind != "import" || span.is_some() {
                    hosts.push(span);
                }
            } else if kind == "instantiation" || kind == "import" {
                assert_eq!(nodes.type_argument_list_span(id), None, "absorbed host: {source}");
            }
        }
        let expected = expected.map(|raw| {
            let start = u32::try_from(source.find('<').unwrap() + 1).unwrap();
            Span::new(start, start + u32::try_from(raw.len()).unwrap())
        });
        assert_eq!(hosts, [expected], "{source}");
    }
}
