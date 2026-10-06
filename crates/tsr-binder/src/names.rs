//! Pure constructed symbol names, prepared before binding workers start.

use rustc_hash::FxHashMap;
use tsr_ast::{Node, NodeId, NodeMap, SyntaxKind};
use tsr_core::Arena;

/// Canonical arena strings for the binder's three allocating name operations.
/// This table contains lexical spellings, not semantic results or checker state.
pub struct PreparedNames<'a>(FxHashMap<String, &'a str>);

impl<'a> PreparedNames<'a> {
    /// Prepare numeric canonicalization, signed computed names and quoted
    /// ambient module names. Source-borrowed names need no entry.
    #[must_use]
    pub fn new(arena: &'a Arena, nodes: &NodeMap<'a>) -> Self {
        let mut names = FxHashMap::default();
        let mut insert = |name: String| {
            names.entry(name).or_insert_with_key(|name| &*arena.alloc_str(name));
        };
        for index in 0..nodes.len() {
            match nodes.get(NodeId::new(u32::try_from(index).expect("node count exceeds u32"))) {
                Some(Node::NumericLiteral(literal)) => {
                    let canonical = tsr_core::jsnum::canonical_numeric_text(literal.text);
                    if canonical != literal.text {
                        insert(canonical);
                    }
                }
                Some(Node::PrefixUnaryExpression(unary))
                    if matches!(
                        unary.operator.kind,
                        SyntaxKind::MinusToken | SyntaxKind::PlusToken
                    ) =>
                {
                    if let Some(tsr_ast::Expression::NumericLiteral(literal)) = unary.operand {
                        let canonical = tsr_core::jsnum::canonical_numeric_text(literal.text);
                        let sign =
                            if unary.operator.kind == SyntaxKind::MinusToken { "-" } else { "+" };
                        insert(format!("{sign}{canonical}"));
                    }
                }
                Some(Node::ModuleDeclaration(module)) => {
                    if let Some(tsr_ast::ModuleName::StringLiteral(literal)) = module.name {
                        insert(format!("\"{}\"", literal.text));
                    }
                }
                _ => {}
            }
        }
        Self(names)
    }
}

#[derive(Clone, Copy)]
pub(crate) enum Names<'a, 'n> {
    Arena(&'a Arena),
    Prepared(&'n PreparedNames<'a>),
}

impl<'a> Names<'a, '_> {
    pub(crate) fn alloc_str(self, text: &str) -> &'a str {
        match self {
            Self::Arena(arena) => arena.alloc_str(text),
            Self::Prepared(names) => {
                names.0.get(text).copied().expect("constructed binder name was prepared")
            }
        }
    }
}
