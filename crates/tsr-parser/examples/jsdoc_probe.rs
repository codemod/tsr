//! Dump the JSDoc parsed from a file. Diagnostic tool.

use tsr_ast::{EntityName, JSDocComment, JSDocTag, Node, NodeTable};
use tsr_core::Arena;

struct Dumper<'s> {
    source: &'s str,
    nodes: &'s NodeTable,
}

impl Dumper<'_> {
    /// The source text a node covers, which is how we check that a parsed type
    /// is the type that was written rather than merely *a* type.
    fn text_of(&self, node: Node<'_>) -> String {
        node.node_id().map_or_else(
            || "<unregistered>".to_string(),
            |id| {
                let span = self.nodes.span(id);
                self.source[span.start as usize..span.end as usize].to_string()
            },
        )
    }

    fn opt<'n>(&self, node: Option<impl Into<Node<'n>>>) -> String {
        node.map_or_else(|| "-".to_string(), |n| self.text_of(n.into()))
    }

    fn tag(&self, tag: &JSDocTag<'_>) -> String {
        match tag {
            JSDocTag::JSDocParameterOrPropertyTag(t) => format!(
                "@{} name={} bracketed={} name_first={} type={}",
                t.tag_name.text,
                t.name.map_or_else(|| "-".into(), entity_text),
                t.is_bracketed,
                t.is_name_first,
                self.opt(t.type_expression),
            ),
            JSDocTag::JSDocReturnTag(t) => {
                format!("@{} type={}", t.tag_name.text, self.opt(t.type_expression))
            }
            JSDocTag::JSDocTypeTag(t) => {
                format!("@{} type={}", t.tag_name.text, self.opt(t.type_expression))
            }
            JSDocTag::JSDocSatisfiesTag(t) => {
                format!("@{} type={}", t.tag_name.text, self.opt(t.type_expression))
            }
            JSDocTag::JSDocThisTag(t) => {
                format!("@{} type={}", t.tag_name.text, self.opt(t.type_expression))
            }
            JSDocTag::JSDocThrowsTag(t) => {
                format!("@{} type={}", t.tag_name.text, self.opt(t.type_expression))
            }
            JSDocTag::JSDocOverloadTag(t) => {
                format!("@{} type={}", t.tag_name.text, self.opt(t.type_expression))
            }
            JSDocTag::JSDocTemplateTag(t) => format!(
                "@{} params=[{}] constraint={}",
                t.tag_name.text,
                t.type_parameters
                    .iter()
                    .map(|p| p.name.map_or("-", |n| n.text))
                    .collect::<Vec<_>>()
                    .join(", "),
                self.opt(t.constraint),
            ),
            JSDocTag::JSDocTypedefTag(t) => format!(
                "@{} name={} type={}",
                t.tag_name.text,
                self.opt(t.name),
                self.opt(t.type_expression),
            ),
            JSDocTag::JSDocAugmentsTag(t) => {
                format!(
                    "@{} class={}",
                    t.tag_name.text,
                    self.opt(t.class_name.map(Node::ExpressionWithTypeArguments))
                )
            }
            JSDocTag::JSDocImplementsTag(t) => {
                format!(
                    "@{} class={}",
                    t.tag_name.text,
                    self.opt(t.class_name.map(Node::ExpressionWithTypeArguments))
                )
            }
            JSDocTag::JSDocSeeTag(t) => {
                format!("@{} name={}", t.tag_name.text, self.opt(t.name_expression))
            }
            JSDocTag::JSDocDeprecatedTag(t) => format!("@{}", t.tag_name.text),
            JSDocTag::JSDocPublicTag(t) => format!("@{}", t.tag_name.text),
            JSDocTag::JSDocPrivateTag(t) => format!("@{}", t.tag_name.text),
            JSDocTag::JSDocProtectedTag(t) => format!("@{}", t.tag_name.text),
            JSDocTag::JSDocReadonlyTag(t) => format!("@{}", t.tag_name.text),
            JSDocTag::JSDocOverrideTag(t) => format!("@{}", t.tag_name.text),
            JSDocTag::JSDocUnknownTag(t) => format!("@{} (unknown)", t.tag_name.text),
            other => format!("{other:?}"),
        }
    }
}

fn comment(parts: &[JSDocComment<'_>]) -> String {
    parts
        .iter()
        .map(|part| match part {
            JSDocComment::JSDocText(t) => format!("{:?}", t.text.concat()),
            JSDocComment::JSDocLink(l) => {
                format!("link({}, {:?})", opt_entity(l.name), l.text.concat())
            }
            JSDocComment::JSDocLinkCode(l) => {
                format!("linkcode({}, {:?})", opt_entity(l.name), l.text.concat())
            }
            JSDocComment::JSDocLinkPlain(l) => {
                format!("linkplain({}, {:?})", opt_entity(l.name), l.text.concat())
            }
        })
        .collect::<Vec<_>>()
        .join(" + ")
}

fn opt_entity(name: Option<EntityName<'_>>) -> String {
    name.map_or_else(|| "-".into(), entity_text)
}

fn entity_text(name: EntityName<'_>) -> String {
    match name {
        EntityName::Identifier(i) => i.text.to_string(),
        EntityName::QualifiedName(q) => format!(
            "{}.{}",
            q.left.map_or_else(String::new, entity_text),
            q.right.map_or("", |r| r.text)
        ),
    }
}

fn main() {
    let path = std::env::args().nth(1).expect("usage: jsdoc_probe <file>");
    let source = std::fs::read_to_string(&path).expect("read");
    let arena = Arena::new();
    let parsed = tsr_parser::parse(&arena, &source);

    println!("diagnostics: {}", parsed.diagnostics.len());
    for d in parsed.diagnostics.iter().take(5) {
        println!("  TS{} {:?} {}", d.message.code(), d.span, d.text());
    }

    let dumper = Dumper { source: &source, nodes: &parsed.nodes };
    println!("documented nodes: {}", parsed.jsdoc.len());
    for (id, docs) in parsed.jsdoc.iter() {
        let span = parsed.nodes.span(id);
        let head: String =
            source[span.start as usize..span.end as usize].chars().take(44).collect();
        println!("\n== {:?}", head.replace('\n', "\\n"));
        for doc in docs {
            if !doc.comment.is_empty() {
                println!("   text: {}", comment(doc.comment));
            }
            for tag in doc.tags {
                println!("   tag:  {}", dumper.tag(tag));
                if let Some(parts) = tag_comment(tag)
                    && !parts.is_empty()
                {
                    println!("         comment: {}", comment(parts));
                }
            }
        }
    }
}

fn tag_comment<'a>(tag: &JSDocTag<'a>) -> Option<&'a [JSDocComment<'a>]> {
    Some(match tag {
        JSDocTag::JSDocParameterOrPropertyTag(t) => t.comment,
        JSDocTag::JSDocReturnTag(t) => t.comment,
        JSDocTag::JSDocTypeTag(t) => t.comment,
        JSDocTag::JSDocDeprecatedTag(t) => t.comment,
        JSDocTag::JSDocTemplateTag(t) => t.comment,
        JSDocTag::JSDocTypedefTag(t) => t.comment,
        JSDocTag::JSDocUnknownTag(t) => t.comment,
        JSDocTag::JSDocSeeTag(t) => t.comment,
        _ => return None,
    })
}
