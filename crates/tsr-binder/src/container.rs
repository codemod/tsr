//! Which nodes introduce a scope, and what kind.
//!
//! A direct port of `GetContainerFlags` (`internal/binder/binder.go:2551`). The
//! classification is subtle and entirely upstream's — every arm here corresponds
//! to one there, and the ordering matters because several kinds fall through.

use tsr_ast::{Node, NodeTable, SyntaxKind};

bitflags::bitflags! {
    /// What kind of scope a node introduces.
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
    pub struct ContainerFlags: u16 {
        /// Owns a symbol table: declarations inside land in it.
        const IS_CONTAINER = 1 << 0;
        /// Owns a block scope, for `let`/`const` and class declarations.
        const IS_BLOCK_SCOPED_CONTAINER = 1 << 1;
        /// Root of a control-flow graph. Recorded but unused until the flow
        /// graph is built — see the note in `lib.rs`.
        const IS_CONTROL_FLOW_CONTAINER = 1 << 2;
        /// Has a `locals` table, as opposed to only members or exports.
        const HAS_LOCALS = 1 << 3;
        /// A function, method, constructor, or signature.
        const IS_FUNCTION_LIKE = 1 << 4;
        /// A function expression or arrow, whose name is not visible outside it.
        const IS_FUNCTION_EXPRESSION = 1 << 5;
        /// An interface: members go in the symbol's `members`, and there are no
        /// locals.
        const IS_INTERFACE = 1 << 6;
        /// Binds its own `this`.
        const IS_THIS_CONTAINER = 1 << 7;
    }
}

/// How `node` scopes its children.
///
/// `nodes` is needed for the two arms that depend on a node's *parent*, which the
/// tree does not carry — it is in the side table
/// ([ADR-0003](../../../docs/adr/0003-tree-plus-side-tables.md)).
#[must_use]
#[allow(clippy::too_many_lines)] // One arm per upstream arm; splitting it would obscure the correspondence.
pub fn container_flags(node: Node<'_>, nodes: &NodeTable) -> ContainerFlags {
    use ContainerFlags as F;
    match node {
        Node::ClassDeclaration(_)
        | Node::ClassExpression(_)
        | Node::EnumDeclaration(_)
        | Node::ObjectLiteralExpression(_)
        | Node::TypeLiteralNode(_)
        | Node::JsxAttributes(_) => F::IS_CONTAINER,

        Node::InterfaceDeclaration(_) => F::IS_CONTAINER | F::IS_INTERFACE,

        Node::ModuleDeclaration(_)
        | Node::TypeAliasDeclaration(_)
        | Node::MappedTypeNode(_)
        | Node::IndexSignatureDeclaration(_) => F::IS_CONTAINER | F::HAS_LOCALS,

        Node::SourceFile(_) => F::IS_CONTAINER | F::IS_CONTROL_FLOW_CONTAINER | F::HAS_LOCALS,

        // A method or accessor on an object literal or class *expression* also
        // binds `this` differently; upstream distinguishes the two and so does
        // this, though nothing consumes the distinction until the checker.
        Node::GetAccessorDeclaration(_)
        | Node::SetAccessorDeclaration(_)
        | Node::MethodDeclaration(_)
        | Node::ConstructorDeclaration(_)
        | Node::FunctionDeclaration(_)
        | Node::ClassStaticBlockDeclaration(_) => {
            F::IS_CONTAINER
                | F::IS_CONTROL_FLOW_CONTAINER
                | F::HAS_LOCALS
                | F::IS_FUNCTION_LIKE
                | F::IS_THIS_CONTAINER
        }

        Node::MethodSignatureDeclaration(_)
        | Node::CallSignatureDeclaration(_)
        | Node::FunctionTypeNode(_)
        | Node::ConstructSignatureDeclaration(_)
        | Node::ConstructorTypeNode(_) => {
            F::IS_CONTAINER | F::IS_CONTROL_FLOW_CONTAINER | F::HAS_LOCALS | F::IS_FUNCTION_LIKE
        }

        Node::FunctionExpression(_) | Node::ArrowFunction(_) => {
            F::IS_CONTAINER
                | F::IS_CONTROL_FLOW_CONTAINER
                | F::HAS_LOCALS
                | F::IS_FUNCTION_LIKE
                | F::IS_FUNCTION_EXPRESSION
        }

        Node::ModuleBlock(_) => F::IS_CONTROL_FLOW_CONTAINER,

        // Only an initialised property introduces a flow container: the
        // initialiser is code, an uninitialised declaration is not.
        Node::PropertyDeclaration(property) => {
            if property.initializer.is_some() {
                F::IS_CONTROL_FLOW_CONTAINER | F::IS_THIS_CONTAINER
            } else {
                F::empty()
            }
        }

        Node::CatchClause(_)
        | Node::ForStatement(_)
        | Node::ForInOrOfStatement(_)
        | Node::CaseBlock(_) => F::IS_BLOCK_SCOPED_CONTAINER | F::HAS_LOCALS,

        // A function's body block is not its own scope — the parameters and the
        // body share one — so this depends on the parent.
        Node::Block(block) => {
            use tsr_ast::HasNodeId as _;
            let parent_is_function_like = block
                .node_id()
                .and_then(|id| nodes.parent(id))
                .is_some_and(|parent| is_function_like_kind(nodes.kind(parent)));
            if parent_is_function_like {
                F::empty()
            } else {
                F::IS_BLOCK_SCOPED_CONTAINER | F::HAS_LOCALS
            }
        }

        _ => F::empty(),
    }
}

/// Whether a kind is function-like, for the `Block` parent test above.
///
/// By kind rather than by node, because the parent is reached through the side
/// table as an id and resolving it back to a `Node` would need the tree.
#[must_use]
pub fn is_function_like_kind(kind: SyntaxKind) -> bool {
    matches!(
        kind,
        SyntaxKind::FunctionDeclaration
            | SyntaxKind::FunctionExpression
            | SyntaxKind::ArrowFunction
            | SyntaxKind::MethodDeclaration
            | SyntaxKind::MethodSignature
            | SyntaxKind::Constructor
            | SyntaxKind::GetAccessor
            | SyntaxKind::SetAccessor
            | SyntaxKind::CallSignature
            | SyntaxKind::ConstructSignature
            | SyntaxKind::IndexSignature
            | SyntaxKind::FunctionType
            | SyntaxKind::ConstructorType
            | SyntaxKind::ClassStaticBlockDeclaration
    )
}
