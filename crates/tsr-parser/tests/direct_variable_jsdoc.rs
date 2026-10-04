//! Native-pinned direct variable JSDoc ownership, without synthetic annotations.
use tsr_ast::{BindingName, JSDocTag, Node, VariableDeclaration};
use tsr_core::Arena;
use tsr_parser::{ParseOptions, ParsedSourceFile};

const DIRECT: &str = r"/** @type {string} statement */
let /** @type {number} first */ /** @type {boolean} second */ first = 17,
    plain = 23, /** @type {Map<string, Array<number>>} later */ later = 31;
for (var /** @type {number} head */ head = 0; head < 2; head++) {}
const tail = 'after';
";

fn variables<'a>(parsed: &ParsedSourceFile<'a>) -> Vec<&'a VariableDeclaration<'a>> {
    let mut pending = vec![Node::SourceFile(parsed.source_file)];
    let mut result = Vec::new();
    while let Some(node) = pending.pop() {
        tsr_ast::push_children(node, &mut pending);
        if let Node::VariableDeclaration(variable) = node {
            result.push(variable);
        }
    }
    result.sort_by_key(|variable| parsed.nodes.span(variable.node_id.unwrap()).start);
    result
}

#[test]
fn direct_first_later_and_for_head_documents_have_exact_owners_and_spans() {
    let arena = Arena::new();
    let parsed = tsr_parser::parse(&arena, DIRECT);
    assert!(parsed.diagnostics.is_empty());
    assert_eq!(parsed.source_file.statements.len(), 3, "scanner restored through the tail");
    assert_eq!(parsed.jsdoc.len(), 4, "statement, first, later, for-head only");
    let variables = variables(&parsed);
    let expected = [
        (
            "first",
            "17",
            vec![
                ("/** @type {number} first */", "number"),
                ("/** @type {boolean} second */", "boolean"),
            ],
        ),
        ("plain", "23", vec![]),
        (
            "later",
            "31",
            vec![("/** @type {Map<string, Array<number>>} later */", "Map<string, Array<number>>")],
        ),
        ("head", "0", vec![("/** @type {number} head */", "number")]),
        ("tail", "'after'", vec![]),
    ];
    assert_eq!(variables.len(), expected.len());
    for (variable, (name, initializer, expected_docs)) in variables.iter().zip(expected) {
        assert!(matches!(variable.name, Some(BindingName::Identifier(id)) if id.text == name));
        assert!(variable.r#type.is_none(), "no synthesized annotation on {name}");
        let span = parsed.nodes.span(variable.initializer.unwrap().node_id().unwrap());
        assert_eq!(&DIRECT[span.start as usize..span.end as usize], initializer);
        let docs = parsed.jsdoc.get(variable.node_id.unwrap());
        assert_eq!(docs.len(), expected_docs.len(), "host {name}");
        for (doc, (comment, ty)) in docs.iter().zip(expected_docs) {
            let doc_id = doc.node_id.unwrap();
            let span = parsed.nodes.span(doc_id);
            assert_eq!(span.start as usize, DIRECT.find(comment).unwrap());
            assert_eq!(&DIRECT[span.start as usize..span.end as usize], comment);
            assert_eq!(parsed.nodes.parent(doc_id), None, "doc root must not enter host scope");
            let [JSDocTag::JSDocTypeTag(tag)] = doc.tags else { panic!("one type tag") };
            let Some(Node::JSDocTypeExpression(expression)) = tag.type_expression else {
                panic!("type expression")
            };
            let type_span = parsed.nodes.span(expression.r#type.unwrap().node_id().unwrap());
            let expected_start =
                DIRECT.find(comment).unwrap() + comment.find(&format!("{{{ty}}}")).unwrap() + 1;
            assert_eq!(type_span.start as usize, expected_start);
            assert_eq!(&DIRECT[type_span.start as usize..type_span.end as usize], ty);
        }
    }
    let statement = Node::from(parsed.source_file.statements[0]);
    let docs = parsed.jsdoc.get(statement.node_id().unwrap());
    assert_eq!(docs.len(), 1);
    let span = parsed.nodes.span(docs[0].node_id.unwrap());
    assert_eq!(&DIRECT[span.start as usize..span.end as usize], "/** @type {string} statement */");
    assert_eq!(parsed.nodes.parent(docs[0].node_id.unwrap()), None);
    for (host, _) in parsed.jsdoc.iter() {
        assert!(matches!(
            parsed.node_map.get(host),
            Some(Node::VariableDeclaration(_) | Node::VariableStatement(_))
        ));
    }
}

#[test]
fn written_annotations_and_disabled_or_absent_jsdoc_preserve_program_syntax() {
    let source = "let /** @type {string} annotation */ typed: number = 17, plain = 23;\nconst tail = 'after';";
    for jsdoc in [true, false] {
        let arena = Arena::new();
        let parsed = tsr_parser::parse_with_options(
            &arena,
            source,
            ParseOptions { jsdoc, ..Default::default() },
        );
        assert!(parsed.diagnostics.is_empty());
        assert_eq!(parsed.source_file.statements.len(), 2);
        let variables = variables(&parsed);
        assert_eq!(variables.len(), 3);
        let type_span = parsed.nodes.span(variables[0].r#type.unwrap().node_id().unwrap());
        assert_eq!(&source[type_span.start as usize..type_span.end as usize], "number");
        for (variable, expected) in variables.iter().zip(["17", "23", "'after'"]) {
            let span = parsed.nodes.span(variable.initializer.unwrap().node_id().unwrap());
            assert_eq!(&source[span.start as usize..span.end as usize], expected);
        }
        assert!(variables[1].r#type.is_none());
        assert!(variables[2].r#type.is_none());
        assert_eq!(parsed.jsdoc.len(), usize::from(jsdoc));
        assert_eq!(parsed.jsdoc.get(variables[0].node_id.unwrap()).len(), usize::from(jsdoc));
    }
    let arena = Arena::new();
    let plain = tsr_parser::parse(&arena, "let /* ordinary */ x = 17, y = 23;");
    assert!(plain.diagnostics.is_empty());
    assert!(plain.jsdoc.is_empty());
}
