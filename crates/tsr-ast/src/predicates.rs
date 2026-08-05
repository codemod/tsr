//! The node-classification predicates the `.types` producer selects on.
//!
//! Ported from `internal/ast/utilities.go`. Upstream's baseline writer keeps a
//! node when `IsExpressionNode(n) || n.Kind == KindIdentifier ||
//! IsDeclarationName(n)`, then drops it again when `IsPartOfTypeNode(n)` or when
//! it is an identifier whose parent carries no *value* meaning
//! (`internal/testutil/tsbaseline/type_symbol_baseline.go:304`, `:345`). Getting
//! any of these wrong shifts every assertion line after the divergence, so they
//! are ported one-for-one rather than approximated. See
//! `docs/architecture/checker-oracle.md`.
//!
//! # Why these need a tree, not a node
//!
//! Every one of them asks about the *parent*: "is this the initialiser of its
//! parent", "is this its parent's name", "is this its parent's type annotation".
//! The AST has no back-edges ([ADR-0003](../../../docs/adr/0003-tree-plus-side-tables.md)),
//! so the parent id comes from [`NodeTable`] and the parent's fields from
//! [`NodeMap`] — which is what [`Tree`] bundles.

use crate::{Node, NodeId, NodeMap, NodeTable, SyntaxKind};

/// A parsed file's side tables, together.
///
/// Passed by value: two shared references, `Copy`, and every predicate here
/// needs both.
#[derive(Clone, Copy)]
pub struct Tree<'a, 'n> {
    /// Kind, span, flags and parent for every node.
    pub nodes: &'n NodeTable,
    /// The typed node behind an id.
    pub map: &'n NodeMap<'a>,
}

impl<'a> Tree<'a, '_> {
    /// The node's kind.
    #[must_use]
    pub fn kind(self, id: NodeId) -> SyntaxKind {
        self.nodes.kind(id)
    }

    /// The node's parent id, or `None` at the root.
    #[must_use]
    pub fn parent(self, id: NodeId) -> Option<NodeId> {
        self.nodes.parent(id)
    }

    /// The parent's kind, or `None` at the root.
    #[must_use]
    pub fn parent_kind(self, id: NodeId) -> Option<SyntaxKind> {
        self.parent(id).map(|p| self.kind(p))
    }

    /// The typed node behind an id.
    #[must_use]
    pub fn node(self, id: NodeId) -> Option<Node<'a>> {
        self.map.get(id)
    }

    /// Whether `id` is the `field` child of its own parent.
    ///
    /// The shape almost every predicate below is written in: upstream spells it
    /// `parent.Initializer() == node`, and because the accessors return ids, the
    /// comparison is exactly that.
    fn is_parents(self, id: NodeId, field: impl Fn(Node<'a>) -> Option<NodeId>) -> bool {
        self.parent(id).and_then(|p| self.node(p)).and_then(field) == Some(id)
    }
}

/// Whether the node is a type node, or part of one.
///
/// Ported from `IsPartOfTypeNode` (`utilities.go:1352`). The baseline writer uses
/// it to drop nodes it should not ask for a type: *"Don't try to get the type of
/// something that's already a type."*
#[must_use]
pub fn is_part_of_type_node(id: NodeId, tree: Tree<'_, '_>) -> bool {
    let kind = tree.kind(id);
    if kind >= SyntaxKind::FIRST_TYPE_NODE && kind <= SyntaxKind::LAST_TYPE_NODE {
        return true;
    }
    match kind {
        SyntaxKind::AnyKeyword
        | SyntaxKind::UnknownKeyword
        | SyntaxKind::NumberKeyword
        | SyntaxKind::BigIntKeyword
        | SyntaxKind::StringKeyword
        | SyntaxKind::BooleanKeyword
        | SyntaxKind::SymbolKeyword
        | SyntaxKind::ObjectKeyword
        | SyntaxKind::UndefinedKeyword
        | SyntaxKind::NullKeyword
        | SyntaxKind::NeverKeyword => true,
        // `void` is a type everywhere except in `void expr`.
        SyntaxKind::VoidKeyword => tree.parent_kind(id) != Some(SyntaxKind::VoidExpression),
        SyntaxKind::TypeParameter => {
            matches!(tree.parent_kind(id), Some(SyntaxKind::MappedType | SyntaxKind::InferType))
        }
        SyntaxKind::Identifier => {
            // A qualified name's right-hand side, or a property access's name,
            // asks about the *parent* instead — `A.B.C` in a type position makes
            // `C` part of a type by way of the access, not by itself.
            if let Some(parent) = tree.parent(id) {
                let parent_kind = tree.kind(parent);
                if parent_kind == SyntaxKind::QualifiedName
                    && tree.is_parents(id, |n| match n {
                        Node::QualifiedName(q) => q.right.and_then(crate::HasNodeId::node_id),
                        _ => None,
                    })
                {
                    return is_part_of_type_node_in_parent(parent, tree);
                }
                if parent_kind == SyntaxKind::PropertyAccessExpression
                    && tree.is_parents(id, |n| n.name_id())
                {
                    return is_part_of_type_node_in_parent(parent, tree);
                }
            }
            is_part_of_type_node_in_parent(id, tree)
        }
        SyntaxKind::QualifiedName
        | SyntaxKind::PropertyAccessExpression
        | SyntaxKind::ThisKeyword => is_part_of_type_node_in_parent(id, tree),
        _ => false,
    }
}

/// Ported from `isPartOfTypeNodeInParent` (`utilities.go:1383`).
///
/// Upstream's comment is the reason this is a separate function and does **not**
/// recurse through [`is_part_of_type_node`]: in `let a: A.B.C`, only `C` and
/// `A.B.C` are type nodes — asking the parent recursively would wrongly make the
/// qualified name `A.B` one too.
fn is_part_of_type_node_in_parent(id: NodeId, tree: Tree<'_, '_>) -> bool {
    let Some(parent) = tree.parent(id) else { return false };
    let parent_kind = tree.kind(parent);
    if parent_kind == SyntaxKind::TypeQuery {
        return false;
    }
    if parent_kind == SyntaxKind::ImportType {
        return match tree.node(parent) {
            Some(Node::ImportTypeNode(n)) => !n.is_type_of,
            _ => false,
        };
    }
    if parent_kind >= SyntaxKind::FIRST_TYPE_NODE && parent_kind <= SyntaxKind::LAST_TYPE_NODE {
        return true;
    }
    match parent_kind {
        SyntaxKind::TypeParameter => match tree.node(parent) {
            Some(Node::TypeParameterDeclaration(n)) => {
                n.constraint.and_then(|c| c.node_id()) == Some(id)
            }
            _ => false,
        },
        SyntaxKind::VariableDeclaration
        | SyntaxKind::Parameter
        | SyntaxKind::PropertyDeclaration
        | SyntaxKind::PropertySignature
        | SyntaxKind::FunctionDeclaration
        | SyntaxKind::FunctionExpression
        | SyntaxKind::ArrowFunction
        | SyntaxKind::Constructor
        | SyntaxKind::MethodDeclaration
        | SyntaxKind::MethodSignature
        | SyntaxKind::GetAccessor
        | SyntaxKind::SetAccessor
        | SyntaxKind::CallSignature
        | SyntaxKind::ConstructSignature
        | SyntaxKind::IndexSignature
        | SyntaxKind::TypeAssertionExpression => {
            tree.node(parent).and_then(|n| n.type_id()) == Some(id)
        }
        // Upstream also treats a node inside a call's *type arguments* as part of
        // a type. Not ported: the type-argument lists are a list-valued field, so
        // there is no generated accessor to compare against, and reaching them
        // means a per-kind match. Recorded rather than silently dropped — it
        // costs assertion lines on generic calls, and `bd tsr-4sc.3` owns it.
        _ => false,
    }
}

/// Whether the node sits where an expression is expected.
///
/// Ported from `IsInExpressionContext` (`utilities.go:1244`).
#[must_use]
pub fn is_in_expression_context(id: NodeId, tree: Tree<'_, '_>) -> bool {
    let Some(parent) = tree.parent(id) else { return false };
    match tree.kind(parent) {
        SyntaxKind::VariableDeclaration
        | SyntaxKind::Parameter
        | SyntaxKind::PropertyDeclaration
        | SyntaxKind::PropertySignature
        | SyntaxKind::EnumMember
        | SyntaxKind::PropertyAssignment
        | SyntaxKind::BindingElement => tree.is_parents(id, |n| n.initializer_id()),
        SyntaxKind::ExpressionStatement
        | SyntaxKind::IfStatement
        | SyntaxKind::DoStatement
        | SyntaxKind::WhileStatement
        | SyntaxKind::ReturnStatement
        | SyntaxKind::WithStatement
        | SyntaxKind::SwitchStatement
        | SyntaxKind::CaseClause
        | SyntaxKind::DefaultClause
        | SyntaxKind::ThrowStatement
        | SyntaxKind::TypeAssertionExpression
        | SyntaxKind::AsExpression
        | SyntaxKind::TemplateSpan
        | SyntaxKind::ComputedPropertyName
        | SyntaxKind::SatisfiesExpression => tree.is_parents(id, |n| n.expression_id()),
        SyntaxKind::ForStatement => match tree.node(parent) {
            Some(Node::ForStatement(n)) => {
                let initializer = n.initializer.and_then(|c| c.node_id());
                (initializer == Some(id) && tree.kind(id) != SyntaxKind::VariableDeclarationList)
                    || n.condition.and_then(|c| c.node_id()) == Some(id)
                    || n.incrementor.and_then(|c| c.node_id()) == Some(id)
            }
            _ => false,
        },
        SyntaxKind::ForInStatement | SyntaxKind::ForOfStatement => {
            let initializer_is_us = tree.is_parents(id, |n| n.initializer_id())
                && tree.kind(id) != SyntaxKind::VariableDeclarationList;
            initializer_is_us || tree.is_parents(id, |n| n.expression_id())
        }
        SyntaxKind::Decorator
        | SyntaxKind::JsxExpression
        | SyntaxKind::JsxSpreadAttribute
        | SyntaxKind::SpreadAssignment => true,
        SyntaxKind::ExpressionWithTypeArguments => {
            tree.is_parents(id, |n| n.expression_id()) && !is_part_of_type_node(parent, tree)
        }
        SyntaxKind::ShorthandPropertyAssignment => match tree.node(parent) {
            Some(Node::ShorthandPropertyAssignment(n)) => {
                n.object_assignment_initializer.and_then(|c| c.node_id()) == Some(id)
            }
            _ => false,
        },
        _ => is_expression_node(parent, tree),
    }
}

/// Whether the node *is* an expression.
///
/// Ported from `IsExpressionNode` (`utilities.go:1209`).
#[must_use]
pub fn is_expression_node(id: NodeId, tree: Tree<'_, '_>) -> bool {
    match tree.kind(id) {
        SyntaxKind::SuperKeyword
        | SyntaxKind::NullKeyword
        | SyntaxKind::TrueKeyword
        | SyntaxKind::FalseKeyword
        | SyntaxKind::RegularExpressionLiteral
        | SyntaxKind::ArrayLiteralExpression
        | SyntaxKind::ObjectLiteralExpression
        | SyntaxKind::PropertyAccessExpression
        | SyntaxKind::ElementAccessExpression
        | SyntaxKind::CallExpression
        | SyntaxKind::NewExpression
        | SyntaxKind::TaggedTemplateExpression
        | SyntaxKind::AsExpression
        | SyntaxKind::TypeAssertionExpression
        | SyntaxKind::SatisfiesExpression
        | SyntaxKind::NonNullExpression
        | SyntaxKind::ParenthesizedExpression
        | SyntaxKind::FunctionExpression
        | SyntaxKind::ClassExpression
        | SyntaxKind::ArrowFunction
        | SyntaxKind::VoidExpression
        | SyntaxKind::DeleteExpression
        | SyntaxKind::TypeOfExpression
        | SyntaxKind::PrefixUnaryExpression
        | SyntaxKind::PostfixUnaryExpression
        | SyntaxKind::BinaryExpression
        | SyntaxKind::ConditionalExpression
        | SyntaxKind::SpreadElement
        | SyntaxKind::TemplateExpression
        | SyntaxKind::OmittedExpression
        | SyntaxKind::JsxElement
        | SyntaxKind::JsxSelfClosingElement
        | SyntaxKind::JsxFragment
        | SyntaxKind::YieldExpression
        | SyntaxKind::AwaitExpression
        // Upstream qualifies this one: `import.defer` in `import.defer(...)` is
        // not an expression (`utilities.go:1222`). Not ported — `IsImportCall`
        // needs the call's callee, and the form is `import.defer`, which this
        // parser does not yet produce. Folded in as plain `true` and recorded
        // rather than left as a silently identical arm.
        | SyntaxKind::MetaProperty => true,
        SyntaxKind::ExpressionWithTypeArguments => {
            tree.parent_kind(id) != Some(SyntaxKind::HeritageClause)
        }
        SyntaxKind::QualifiedName => {
            // Walk to the outermost qualified name, then ask what encloses it.
            let mut current = id;
            while tree.parent_kind(current) == Some(SyntaxKind::QualifiedName) {
                current = tree.parent(current).expect("checked by parent_kind");
            }
            tree.parent_kind(current) == Some(SyntaxKind::TypeQuery)
                || is_jsx_tag_name(current, tree)
        }
        SyntaxKind::PrivateIdentifier => match tree.parent(id).and_then(|p| tree.node(p)) {
            Some(Node::BinaryExpression(n)) => {
                n.left.and_then(|c| c.node_id()) == Some(id)
                    && n.operator_token.is_some_and(|t| t.kind == SyntaxKind::InKeyword)
            }
            _ => false,
        },
        SyntaxKind::Identifier => {
            if tree.parent_kind(id) == Some(SyntaxKind::TypeQuery) || is_jsx_tag_name(id, tree) {
                return true;
            }
            is_in_expression_context(id, tree)
        }
        SyntaxKind::NumericLiteral
        | SyntaxKind::BigIntLiteral
        | SyntaxKind::StringLiteral
        | SyntaxKind::NoSubstitutionTemplateLiteral
        | SyntaxKind::ThisKeyword => is_in_expression_context(id, tree),
        _ => false,
    }
}

/// Ported from `IsJsxTagName` (`utilities.go`).
fn is_jsx_tag_name(id: NodeId, tree: Tree<'_, '_>) -> bool {
    let Some(parent) = tree.parent(id) else { return false };
    let tag = match tree.node(parent) {
        Some(Node::JsxOpeningElement(n)) => n.tag_name.and_then(|c| c.node_id()),
        Some(Node::JsxClosingElement(n)) => n.tag_name.and_then(|c| c.node_id()),
        Some(Node::JsxSelfClosingElement(n)) => n.tag_name.and_then(|c| c.node_id()),
        _ => None,
    };
    tag == Some(id)
}

/// Whether the node is the name of a declaration.
///
/// Ported from `IsDeclarationName` (`utilities.go:1306`):
///
/// ```go
/// !IsSourceFile(name) && !IsBindingPattern(name) && IsDeclaration(name.Parent) && name.Parent.Name() == name
/// ```
///
/// The `IsDeclaration(parent)` clause is load-bearing and cannot be replaced by
/// "the parent has a `name` field": `PropertyAccessExpression` has one — `b` in
/// `a.b` — and is not a declaration. See `Node::is_declaration_node`.
#[must_use]
pub fn is_declaration_name(id: NodeId, tree: Tree<'_, '_>) -> bool {
    let kind = tree.kind(id);
    if kind == SyntaxKind::SourceFile
        || matches!(kind, SyntaxKind::ObjectBindingPattern | SyntaxKind::ArrayBindingPattern)
    {
        return false;
    }
    let Some(parent) = tree.parent(id) else { return false };
    let Some(parent_node) = tree.node(parent) else { return false };
    // `IsDeclaration` adds one special case over `IsDeclarationNode`: a type
    // parameter counts only when it has a parent (`utilities.go:1298`).
    let is_declaration = if tree.kind(parent) == SyntaxKind::TypeParameter {
        tree.parent(parent).is_some()
    } else {
        parent_node.is_declaration_node()
    };
    is_declaration && parent_node.name_id() == Some(id)
}

bitflags::bitflags! {
    /// What a declaration contributes to a name.
    ///
    /// Ported from `SemanticMeaning` (`utilities.go:2194`), bit values included.
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub struct SemanticMeaning: u8 {
        /// The name denotes a value.
        const VALUE = 1 << 0;
        /// The name denotes a type.
        const TYPE = 1 << 1;
        /// The name denotes a namespace.
        const NAMESPACE = 1 << 2;
    }
}

impl SemanticMeaning {
    /// `SemanticMeaningAll`.
    pub const ALL: Self = Self::VALUE.union(Self::TYPE).union(Self::NAMESPACE);
}

/// What the declaration at `id` means.
///
/// Ported from `GetMeaningFromDeclaration` (`utilities.go:2200`). The `.types`
/// baseline writer uses it to drop an identifier whose parent declares no
/// **value** — which is what keeps `interface I` and an import's names out of a
/// `.types` baseline while `const x` stays in.
///
/// # The module case is approximated
///
/// Upstream answers `Namespace | Value` for an ambient module and for one whose
/// `GetModuleInstanceState` is `Instantiated`, and bare `Namespace` otherwise —
/// i.e. a namespace containing only types has no value side and its name is
/// dropped. `GetModuleInstanceState` is a recursive analysis this port does not
/// have, so `ModuleDeclaration` answers `Namespace | Value` unconditionally.
///
/// **The error is one-directional**: a type-only namespace keeps a line upstream
/// would drop, so we over-emit rather than under-emit. Recorded in
/// `docs/architecture/checker-oracle.md` and owned by `bd tsr-4sc.3`.
#[must_use]
#[allow(
    clippy::match_same_arms,
    reason = "the arms mirror upstream's explicit list in GetMeaningFromDeclaration;               collapsing the ones that happen to share an answer into the fallback               would lose which kinds upstream names and make the next drift               invisible"
)]
pub fn meaning_from_declaration(id: NodeId, tree: Tree<'_, '_>) -> SemanticMeaning {
    match tree.kind(id) {
        SyntaxKind::VariableDeclaration
        | SyntaxKind::Parameter
        | SyntaxKind::BindingElement
        | SyntaxKind::PropertyDeclaration
        | SyntaxKind::PropertySignature
        | SyntaxKind::PropertyAssignment
        | SyntaxKind::ShorthandPropertyAssignment
        | SyntaxKind::MethodDeclaration
        | SyntaxKind::MethodSignature
        | SyntaxKind::Constructor
        | SyntaxKind::GetAccessor
        | SyntaxKind::SetAccessor
        | SyntaxKind::FunctionDeclaration
        | SyntaxKind::FunctionExpression
        | SyntaxKind::ArrowFunction
        | SyntaxKind::CatchClause
        | SyntaxKind::JsxAttribute => SemanticMeaning::VALUE,

        SyntaxKind::TypeParameter
        | SyntaxKind::InterfaceDeclaration
        | SyntaxKind::TypeAliasDeclaration
        | SyntaxKind::TypeLiteral => SemanticMeaning::TYPE,

        SyntaxKind::EnumMember | SyntaxKind::ClassDeclaration => {
            SemanticMeaning::VALUE | SemanticMeaning::TYPE
        }

        // Two upstream arms with the same answer, merged because clippy rejects
        // identical bodies — but they are there for different reasons.
        //
        // `SourceFile` is `Namespace | Value` outright: an external module can be
        // a value.
        //
        // `ModuleDeclaration` is `Namespace | Value` only when the module is
        // ambient or *instantiated*; a namespace holding nothing but types is
        // bare `Namespace` upstream, and its name gets no line. That distinction
        // needs `GetModuleInstanceState`, which is not ported — see this
        // function's doc comment. The error over-emits rather than under-emits.
        SyntaxKind::ModuleDeclaration | SyntaxKind::SourceFile => {
            SemanticMeaning::NAMESPACE | SemanticMeaning::VALUE
        }

        SyntaxKind::EnumDeclaration
        | SyntaxKind::NamedImports
        | SyntaxKind::ImportSpecifier
        | SyntaxKind::ImportEqualsDeclaration
        | SyntaxKind::ImportDeclaration
        | SyntaxKind::ExportAssignment
        | SyntaxKind::ExportDeclaration => SemanticMeaning::ALL,

        _ => SemanticMeaning::ALL,
    }
}
