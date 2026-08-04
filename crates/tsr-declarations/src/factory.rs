//! Building nodes that the parser never saw.
//!
//! Stands in for typescript-go's `ast.NodeFactory` together with the parts of
//! `printer.EmitContext` this transform needs (`internal/ast/ast_generated.go`,
//! `internal/printer/emitcontext.go`). Upstream's factory is 4,000 lines of
//! `NewX`/`UpdateX` pairs; this is the subset the declaration transform calls,
//! and nothing else.
//!
//! # Why a synthesized node must be registered, not merely allocated
//!
//! A node is two things here: a struct in the arena, and a row in the
//! [`NodeTable`] holding its kind, span and [`NodeFlags`]
//! ([ADR-0003](../../../docs/adr/0003-tree-plus-side-tables.md)). The printer
//! reads the side table in two places, and both are load-bearing rather than
//! cosmetic:
//!
//! - `Printer::kind_of` dispatches on the table's kind, because several nodes
//!   carry a `kind` *field* that is a punctuation token — `BindingPattern.kind` is
//!   the opening bracket, not `ObjectBindingPattern`.
//! - `list_keyword` recovers `const`/`let`/`using` from the list's `NodeFlags`,
//!   because the keyword is not a token in the tree.
//!
//! So a `VariableDeclarationList` allocated without a table row prints as `var`,
//! silently, whatever it was built to mean. [`Factory::alloc`] therefore takes the
//! kind, span and flags and pushes the row before handing back the reference;
//! there is deliberately no way to allocate a node without registering it.
//!
//! # Spans on synthesized nodes
//!
//! Upstream tracks original positions through `EmitContext.SetOriginal` and
//! `setTextRange`, because its printer reproduces the author's layout and its
//! source maps need the correspondence. Neither applies here — this printer takes
//! no positions into account (`docs/architecture/printer.md`) — but a span is
//! still what a *diagnostic* is anchored to. Synthesized nodes therefore carry the
//! span of whatever they were derived from, and [`Factory::alloc`] has no
//! zero-span default to reach for by accident.

use tsr_ast::{
    HasNodeId, Identifier, KeywordTypeNode, ModifierLike, NodeFlags, NodeTable, SourceFile,
    Statement, SyntaxKind, Token, TypeNode,
};
use tsr_core::{Arena, Span};

/// Allocates and registers nodes the parser never produced.
pub struct Factory<'a, 't> {
    arena: &'a Arena,
    nodes: &'t mut NodeTable,
}

impl<'a, 't> Factory<'a, 't> {
    /// A factory allocating into `arena` and registering into `nodes`.
    pub fn new(arena: &'a Arena, nodes: &'t mut NodeTable) -> Self {
        Self { arena, nodes }
    }

    /// The side table, for reading the kind, span or flags of an existing node.
    #[must_use]
    pub fn nodes(&self) -> &NodeTable {
        self.nodes
    }

    /// The span of an existing node, or an empty one if it was never registered.
    #[must_use]
    pub fn span_of(&self, id: Option<tsr_ast::NodeId>) -> Span {
        id.map_or_else(|| Span::new(0, 0), |id| self.nodes.span(id))
    }

    /// The `NodeFlags` of an existing node.
    #[must_use]
    pub fn flags_of(&self, id: Option<tsr_ast::NodeId>) -> NodeFlags {
        id.map_or_else(NodeFlags::empty, |id| self.nodes.flags(id))
    }

    /// Allocate a node and record its row in the side table.
    ///
    /// [`NodeFlags::SYNTHESIZED`] is added unconditionally, matching upstream's
    /// `NodeFlagsSynthesized`: it is what distinguishes a node that has source
    /// text from one that does not, and reading a span off the latter is a bug
    /// worth being able to detect.
    pub fn alloc<T: HasNodeId>(
        &mut self,
        mut value: T,
        kind: SyntaxKind,
        span: Span,
        flags: NodeFlags,
    ) -> &'a T {
        let id = self.nodes.push(kind, span, flags | NodeFlags::SYNTHESIZED);
        value.set_node_id(id);
        &*self.arena.alloc(value)
    }

    /// Copy a string into the arena so a node can hold it.
    ///
    /// Needed because a synthesized literal's text is *computed* — `0x1` becomes
    /// `1` — so it does not borrow from the source the way a parsed node's does.
    #[must_use]
    pub fn alloc_str(&self, value: &str) -> &'a str {
        &*self.arena.alloc_str(value)
    }

    /// Copy a slice into the arena so it can be a node's child list.
    #[must_use]
    pub fn slice<T: Copy>(&self, values: &[T]) -> &'a [T] {
        &*self.arena.alloc_slice(values)
    }

    /// Ported from `NodeFactory.NewModifier`.
    pub fn modifier(&mut self, kind: SyntaxKind, span: Span) -> ModifierLike<'a> {
        ModifierLike::Token(self.alloc(Token::new(kind), kind, span, NodeFlags::empty()))
    }

    /// Ported from `NodeFactory.NewToken`.
    pub fn token(&mut self, kind: SyntaxKind, span: Span) -> &'a Token<'a> {
        self.alloc(Token::new(kind), kind, span, NodeFlags::empty())
    }

    /// Ported from `NodeFactory.NewIdentifier`.
    pub fn identifier(&mut self, text: &str, span: Span) -> &'a Identifier<'a> {
        let text = &*self.arena.alloc_str(text);
        self.alloc(Identifier::new(text), SyntaxKind::Identifier, span, NodeFlags::empty())
    }

    /// Ported from `NodeFactory.NewKeywordTypeNode`.
    ///
    /// The node's kind *is* the keyword, so the field and the table row must
    /// agree; passing them separately is what the one call site would get wrong.
    pub fn keyword_type(&mut self, kind: SyntaxKind, span: Span) -> TypeNode<'a> {
        TypeNode::KeywordTypeNode(self.alloc(
            KeywordTypeNode::new(kind),
            kind,
            span,
            NodeFlags::empty(),
        ))
    }

    /// Ported from `NodeFactory.UpdateSourceFile`.
    ///
    /// Upstream also sets `IsDeclarationFile`, the referenced-file lists and the
    /// lib/type reference directives on the result. Those are properties of an
    /// emitted *file* rather than of the tree, and this port carries them on
    /// [`crate::DeclarationFile`] instead of on the node, because
    /// `ast::SourceFile` here holds only syntax children
    /// ([ADR-0003](../../../docs/adr/0003-tree-plus-side-tables.md)).
    pub fn update_source_file(
        &mut self,
        original: &SourceFile<'a>,
        statements: &'a [Statement<'a>],
    ) -> &'a SourceFile<'a> {
        let span = self.span_of(original.node_id);
        self.alloc(
            SourceFile::new(statements, original.end_of_file_token),
            SyntaxKind::SourceFile,
            span,
            NodeFlags::empty(),
        )
    }
}
