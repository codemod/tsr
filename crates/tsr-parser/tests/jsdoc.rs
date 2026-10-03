//! JSDoc parsing.
//!
//! The assertions here go through the parsed tree rather than the comment text:
//! a test that only checked "a tag named `param` exists" would have passed
//! throughout the period when every type expression was being parsed one token
//! late, because JSDoc diagnostics are discarded and the wrong type is still *a*
//! type. Checking the type's source span is what catches that.

use tsr_ast::{EntityName, JSDocComment, JSDocTag, ModifierLike, Node, NodeTable, SyntaxKind};
use tsr_core::Arena;
use tsr_parser::{JSDocTable, ParsedSourceFile};

/// The source text a node covers.
fn text_of(source: &str, nodes: &NodeTable, node: Node<'_>) -> String {
    let id = node.node_id().expect("node was registered");
    let span = nodes.span(id);
    source[span.start as usize..span.end as usize].to_string()
}

/// Every tag in the file, flattened, in source order.
fn tags<'a>(parsed: &'a ParsedSourceFile<'a>) -> Vec<&'a JSDocTag<'a>> {
    parsed.jsdoc.iter().flat_map(|(_, docs)| docs.iter().flat_map(|doc| doc.tags.iter())).collect()
}

fn parse<'a>(arena: &'a Arena, source: &'a str) -> ParsedSourceFile<'a> {
    let parsed = tsr_parser::parse(arena, source);
    assert!(
        parsed.diagnostics.is_empty(),
        "unexpected diagnostics: {:?}",
        parsed.diagnostics.iter().map(tsr_diagnostics::Diagnostic::text).collect::<Vec<_>>()
    );
    parsed
}

/// The `{…}` of the one tag in `source`, as written.
fn only_tag_type(source: &str) -> String {
    let arena = Arena::new();
    let parsed = parse(&arena, source);
    let tags = tags(&parsed);
    assert_eq!(tags.len(), 1, "expected exactly one tag in {source:?}");
    let type_expression = match tags[0] {
        JSDocTag::JSDocParameterOrPropertyTag(t) => t.type_expression,
        JSDocTag::JSDocReturnTag(t) => t.type_expression,
        JSDocTag::JSDocSatisfiesTag(t) => t.type_expression,
        JSDocTag::JSDocThisTag(t) => t.type_expression,
        JSDocTag::JSDocThrowsTag(t) => t.type_expression,
        other => panic!("tag has no simple type expression: {other:?}"),
    };
    text_of(source, &parsed.nodes, type_expression.expect("a type").into())
}

#[test]
fn a_file_without_jsdoc_builds_no_table() {
    let arena = Arena::new();
    let parsed = parse(&arena, "// not jsdoc\n/* nor this */\nlet x = 1;");
    assert!(parsed.jsdoc.is_empty());
}

#[test]
fn the_degenerate_comment_is_not_jsdoc() {
    let arena = Arena::new();
    let parsed = parse(&arena, "/**/\nlet x = 1;");
    assert!(parsed.jsdoc.is_empty());
}

#[test]
fn comments_attach_to_the_construct_that_follows() {
    let arena = Arena::new();
    let source = "/** one */\nlet a;\n/** two */\nlet b;";
    let parsed = parse(&arena, source);
    let attached: Vec<String> = parsed
        .jsdoc
        .iter()
        .map(|(id, _)| {
            let span = parsed.nodes.span(id);
            source[span.start as usize..span.end as usize].to_string()
        })
        .collect();
    assert_eq!(attached, ["let a;", "let b;"]);
}

#[test]
fn several_comments_before_one_construct_all_attach() {
    let arena = Arena::new();
    let parsed = parse(&arena, "/** one */\n/** two */\nlet a;");
    assert_eq!(parsed.jsdoc.len(), 1, "one node");
    let (_, docs) = parsed.jsdoc.iter().next().unwrap();
    assert_eq!(docs.len(), 2, "two comments");
}

#[test]
fn jsdoc_attaches_to_members_and_parameters_too() {
    let arena = Arena::new();
    let source = "class C {\n  /** field */\n  x = 1;\n  /** method */\n  m(/** param */ a) {}\n}\n\
                  interface I {\n  /** member */\n  y: number;\n}";
    let parsed = parse(&arena, source);
    // Neither `class C` nor `interface I` is itself documented here.
    assert_eq!(parsed.jsdoc.len(), 4, "field, method, parameter, interface member");
}

#[test]
fn the_type_parsed_is_the_type_written() {
    // Every one of these ends the type parse on a different token, which is what
    // makes the hand-off back to JSDoc scanning worth testing at each shape.
    for ty in [
        "{number}",
        "{number[]}",
        "{Array<number>}",
        "{{x: number}}",
        "{(a: number) => void}",
        "{string | undefined}",
        "{Record<string, Array<number>>}",
    ] {
        let source = format!("/** @param {ty} a */\nfunction f(a) {{}}");
        assert_eq!(only_tag_type(&source), ty, "for {ty}");
    }
}

#[test]
fn a_type_may_span_continuation_lines() {
    // The `*` decorating the second line is not part of the union.
    let source = "/**\n * @param {number |\n *   string} a\n */\nfunction f(a) {}";
    let arena = Arena::new();
    let parsed = parse(&arena, source);
    let tags = tags(&parsed);
    assert_eq!(tags.len(), 1);
    let JSDocTag::JSDocParameterOrPropertyTag(tag) = tags[0] else { panic!("expected @param") };
    assert!(tag.type_expression.is_some(), "the type survived the line break");
}

#[test]
fn a_parameter_name_may_come_before_or_after_its_type() {
    let arena = Arena::new();
    let source = "/**\n * @param {number} a\n * @param b\n */\nfunction f(a, b) {}";
    let parsed = parse(&arena, source);
    let tags = tags(&parsed);
    let names: Vec<(String, bool)> = tags
        .iter()
        .map(|tag| {
            let JSDocTag::JSDocParameterOrPropertyTag(t) = tag else { panic!("expected @param") };
            let EntityName::Identifier(name) = t.name.expect("a name") else { panic!("simple") };
            (name.text.to_string(), t.is_name_first)
        })
        .collect();
    assert_eq!(names, [("a".to_string(), false), ("b".to_string(), true)]);
}

#[test]
fn brackets_mark_a_parameter_optional() {
    let arena = Arena::new();
    let source = "/**\n * @param [a]\n * @param [b=1]\n * @param c\n */\nfunction f(a, b, c) {}";
    let parsed = parse(&arena, source);
    let bracketed: Vec<bool> = tags(&parsed)
        .iter()
        .map(|tag| {
            let JSDocTag::JSDocParameterOrPropertyTag(t) = tag else { panic!("expected @param") };
            t.is_bracketed
        })
        .collect();
    assert_eq!(bracketed, [true, true, false]);
}

#[test]
fn optional_property_suffix_wraps_the_whole_type_and_preserves_the_name() {
    use tsr_ast::TypeNode;
    let arena = Arena::new();
    let source = "/**\n * @property {T[]=} items\n * @property {T | string=} choice\n * @property {T[]} required\n */\nlet host;";
    let parsed = parse(&arena, source);
    let tags = tags(&parsed);
    assert_eq!(tags.len(), 3);
    for (tag, expected_name, optional, expected_type) in [
        (tags[0], "items", true, "T[]="),
        (tags[1], "choice", true, "T | string="),
        (tags[2], "required", false, "T[]"),
    ] {
        let JSDocTag::JSDocParameterOrPropertyTag(property) = tag else {
            panic!("property tag");
        };
        assert!(
            matches!(property.name, Some(EntityName::Identifier(name)) if name.text == expected_name)
        );
        let Some(TypeNode::JSDocTypeExpression(expression)) = property.type_expression else {
            panic!("type expression");
        };
        let ty = expression.r#type.expect("inner type");
        assert_eq!(matches!(ty, TypeNode::JSDocOptionalType(_)), optional);
        assert_eq!(text_of(source, &parsed.nodes, ty.into()), expected_type);
    }
}

#[test]
fn tag_aliases_produce_the_same_node() {
    for (a, b) in [("@returns", "@return"), ("@augments", "@extends"), ("@throws", "@exception")] {
        let arena = Arena::new();
        let source = format!(
            "/** {a} {{number}} */\nfunction f() {{}}\n/** {b} {{number}} */\nfunction g() {{}}"
        );
        let parsed = parse(&arena, &source);
        let tags = tags(&parsed);
        assert_eq!(tags.len(), 2, "for {a}/{b}");
        assert_eq!(
            std::mem::discriminant(tags[0]),
            std::mem::discriminant(tags[1]),
            "{a} and {b} should build the same node"
        );
    }
}

#[test]
fn an_unrecognised_tag_is_kept_rather_than_dropped() {
    let arena = Arena::new();
    let parsed = parse(&arena, "/** @my-custom-tag some text */\nlet a;");
    let tags = tags(&parsed);
    assert_eq!(tags.len(), 1);
    let JSDocTag::JSDocUnknownTag(tag) = tags[0] else { panic!("expected an unknown tag") };
    // The hyphens belong to the name, not to a subtraction.
    assert_eq!(tag.tag_name.text, "my-custom-tag");
}

#[test]
fn links_are_separated_from_the_prose_around_them() {
    let arena = Arena::new();
    let source = "/** See {@link add} and {@linkcode a.b}. */\nlet v;";
    let parsed = parse(&arena, source);
    let (_, docs) = parsed.jsdoc.iter().next().expect("one documented node");
    let parts: Vec<String> = docs[0]
        .comment
        .iter()
        .map(|part| match part {
            JSDocComment::JSDocText(t) => format!("text({:?})", t.text.concat()),
            JSDocComment::JSDocLink(l) => format!("link({})", entity(l.name)),
            JSDocComment::JSDocLinkCode(l) => format!("linkcode({})", entity(l.name)),
            JSDocComment::JSDocLinkPlain(l) => format!("linkplain({})", entity(l.name)),
        })
        .collect();
    assert_eq!(
        parts,
        [
            "text(\"See \")".to_string(),
            "link(add)".to_string(),
            // The space after `}` belongs to the text that follows the link.
            "text(\" and \")".to_string(),
            "linkcode(a.b)".to_string(),
            "text(\".\")".to_string(),
        ]
    );
}

fn entity(name: Option<EntityName<'_>>) -> String {
    match name {
        None => "-".into(),
        Some(EntityName::Identifier(i)) => i.text.to_string(),
        Some(EntityName::QualifiedName(q)) => {
            format!("{}.{}", entity(q.left), q.right.map_or("", |r| r.text))
        }
    }
}

#[test]
fn an_at_sign_in_prose_does_not_start_a_tag() {
    let arena = Arena::new();
    // No whitespace before the `@`, so it is part of the address.
    let parsed = parse(&arena, "/** Mail foo@bar.com about it. */\nlet a;");
    assert!(tags(&parsed).is_empty(), "an email address is not a tag");
}

#[test]
fn a_second_asterisk_on_a_line_is_text() {
    let arena = Arena::new();
    // The opening `/**` already supplies the line's one leading asterisk, so a
    // second one is prose rather than decoration.
    //
    // Upstream's comment on this says the effect is that `/** * @type */` "does
    // not parse" — but its code reaches the `@` in the ordinary way and builds
    // the tag, and this follows the code. Asserting the comment's claim instead
    // would be asserting documentation, not behaviour; see ADR-0006.
    let parsed = parse(&arena, "/** * @type {number} */\nlet a;");
    let (_, docs) = parsed.jsdoc.iter().next().expect("one documented node");
    let text: String = docs[0]
        .comment
        .iter()
        .map(|part| match part {
            JSDocComment::JSDocText(t) => t.text.concat(),
            _ => String::new(),
        })
        .collect();
    assert!(text.contains('*'), "the second asterisk is text, not decoration: {text:?}");
    assert_eq!(tags(&parsed).len(), 1, "the tag is still built");
}

#[test]
fn tags_inside_a_fenced_code_block_are_text() {
    let arena = Arena::new();
    let source =
        "/**\n * ```\n * @param not a tag\n * ```\n * @param real\n */\nfunction f(real) {}";
    let parsed = parse(&arena, source);
    let tags = tags(&parsed);
    assert_eq!(tags.len(), 1, "only the tag outside the fence counts");
}

#[test]
fn template_parameters_share_one_constraint() {
    let arena = Arena::new();
    let source = "/** @template {object} T, U */\nfunction f() {}";
    let parsed = parse(&arena, source);
    let tags = tags(&parsed);
    let JSDocTag::JSDocTemplateTag(tag) = tags[0] else { panic!("expected @template") };
    let names: Vec<&str> =
        tag.type_parameters.iter().map(|p| p.name.map_or("-", |n| n.text)).collect();
    assert_eq!(names, ["T", "U"]);
    assert!(tag.constraint.is_some(), "the `{{object}}` constrains both");
}

#[test]
fn template_variance_modifiers_precede_the_actual_name() {
    let arena = Arena::new();
    let source = "/** @template in A, out B, in out C, D */\nfunction f() {}";
    let parsed = parse(&arena, source);
    let tags = tags(&parsed);
    let JSDocTag::JSDocTemplateTag(tag) = tags[0] else { panic!("expected @template") };
    let parameters: Vec<_> = tag
        .type_parameters
        .iter()
        .map(|parameter| {
            let modifiers: Vec<_> = parameter
                .modifiers
                .iter()
                .map(|modifier| match modifier {
                    ModifierLike::Token(token) => token.kind,
                    ModifierLike::Decorator(_) => panic!("variance is a token modifier"),
                })
                .collect();
            (parameter.name.expect("name").text, modifiers)
        })
        .collect();
    assert_eq!(
        parameters,
        [
            ("A", vec![SyntaxKind::InKeyword]),
            ("B", vec![SyntaxKind::OutKeyword]),
            ("C", vec![SyntaxKind::InKeyword, SyntaxKind::OutKeyword]),
            ("D", vec![]),
        ]
    );
}

#[test]
fn callback_remains_outside_the_typedef_parser_path() {
    let arena = Arena::new();
    let parsed = parse(&arena, "/** @callback C */\nlet value;");
    assert!(matches!(
        tags(&parsed).as_slice(),
        [JSDocTag::JSDocUnknownTag(tag)] if tag.tag_name.text == "callback"
    ));
}

#[test]
fn typedef_keeps_both_its_type_and_its_name() {
    let source = "/** @typedef {{x: number}} Point */\nlet a;";
    let arena = Arena::new();
    let parsed = parse(&arena, source);
    let tags = tags(&parsed);
    let JSDocTag::JSDocTypedefTag(tag) = tags[0] else { panic!("expected @typedef") };
    assert_eq!(text_of(source, &parsed.nodes, tag.name.expect("a name").into()), "Point");
    assert_eq!(
        text_of(source, &parsed.nodes, tag.type_expression.expect("a type")),
        "{{x: number}}"
    );
}

#[test]
fn malformed_jsdoc_reports_nothing_and_terminates() {
    // JSDoc is not part of a `.ts` file's syntax, so a broken comment must not
    // become a compile error — and must not hang the parser either.
    let arena = Arena::new();
    for source in [
        "/** @param {unclosed */\nlet a;",
        "/** @ */\nlet a;",
        "/** @param */\nlet a;",
        "/** {@link */\nlet a;",
        "/** @template */\nlet a;",
        "/** @typedef */\nlet a;",
        "/** {{{{ */\nlet a;",
        "/** @param {} x */\nlet a;",
    ] {
        let parsed = tsr_parser::parse(&arena, source);
        assert!(
            parsed.diagnostics.is_empty(),
            "{source:?} produced diagnostics: {:?}",
            parsed.diagnostics.iter().map(tsr_diagnostics::Diagnostic::text).collect::<Vec<_>>()
        );
    }
}

#[test]
fn parsing_a_comment_leaves_the_cursor_where_it_was() {
    // The scanner is retargeted into the comment and must come back exactly: if
    // it did not, the statement after a documented one would be misparsed.
    let arena = Arena::new();
    let parsed = parse(
        &arena,
        "/** @param {number} a */\nfunction f(a) { return a; }\nconst x: string = 'ok';",
    );
    assert_eq!(parsed.source_file.statements.len(), 2);
}

#[test]
fn a_documented_node_can_be_found_from_its_id() {
    let arena = Arena::new();
    let parsed = parse(&arena, "/** doc */\nlet a;");
    let (id, docs) = parsed.jsdoc.iter().next().expect("one entry");
    assert_eq!(parsed.jsdoc.get(id).len(), docs.len());
}

#[test]
fn an_undocumented_node_looks_up_empty() {
    let arena = Arena::new();
    let parsed = parse(&arena, "/** doc */\nlet a;\nlet b;");
    let table: &JSDocTable = &parsed.jsdoc;
    let documented = table.iter().next().expect("one entry").0;
    // The `let b;` statement is registered but undocumented.
    let undocumented = parsed
        .source_file
        .statements
        .iter()
        .map(|s| tsr_ast::Node::from(*s).node_id().expect("registered"))
        .find(|id| *id != documented)
        .expect("a second statement");
    assert!(table.get(undocumented).is_empty());
}
