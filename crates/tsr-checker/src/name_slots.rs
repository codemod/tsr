//! Which identifiers native `checkIdentifier` reaches, for the slots
//! `Checker::is_value_reference` (`check.rs`) does not list.
//!
//! TS2304 here is reported from a walk over every identifier, gated by a
//! parent-slot allow-list. Native reports it from `getResolvedSymbol`
//! (`checker.go:13890`), so a name is diagnosed exactly when some check calls
//! `checkExpression` on it. This module holds the r6-names corrections to that
//! gate, each anchored to the native call that does or does not reach the
//! identifier. `docs/parity/notes/r6-names.md` records the measurements and the
//! apply order of the hooks in `check.rs`.
//!
//! No cache, side table or traversal state: every question is a bounded
//! parent walk over the immutable node table.

use tsr_ast::{ForInitializer, JsxTagNameExpression, Node, NodeId, SyntaxKind};

use crate::checker::Checker;
use crate::jsx_intrinsic::is_intrinsic_jsx_name;

/// Value slots native checks that `is_value_reference` has no arm for.
///
/// - `checkWithStatement` (`checker.go:4162`) calls `checkExpression` on the
///   `with` expression; only the *statement* is parsed under
///   `NodeFlagsInWithStatement` (`parser.go:1386`, `parseWithStatement`).
/// - A JSX opening, self-closing or closing tag that is not
///   `isJsxIntrinsicTagName` (`checker/utilities.go:1116`) is
///   `checkExpression`ed: `resolveJsxOpeningLikeElement` (`jsx.go:562`) for
///   the opening tag and `checkJsxElementDeferred` (`jsx.go:84`) for the
///   closing one. A property-access tag already reaches the
///   `PropertyAccessExpression` arm through its receiver.
///
/// Inlined into `is_value_reference`'s fallback arm: out of line it cost
/// domain-model 0.255 M Ir (one call per identifier no other arm takes), and
/// plain `#[inline]` was not taken (`docs/parity/notes/r6-names2.md` §2).
#[allow(clippy::inline_always, reason = "measured, r6-names2.md §2")]
#[inline(always)]
pub(crate) fn value_reference_slot(node: NodeId, parent: Node<'_>) -> bool {
    let value_tag = |tag: Option<JsxTagNameExpression<'_>>| {
        matches!(tag, Some(JsxTagNameExpression::Identifier(name))
            if name.node_id == Some(node) && !is_intrinsic_jsx_name(name.text))
    };
    match parent {
        Node::WithStatement(with) => with.expression.and_then(|e| e.node_id()) == Some(node),
        Node::JsxOpeningElement(element) => value_tag(element.tag_name),
        Node::JsxSelfClosingElement(element) => value_tag(element.tag_name),
        Node::JsxClosingElement(element) => value_tag(element.tag_name),
        _ => false,
    }
}

impl Checker<'_, '_> {
    /// Is `node` inside a region native never checks?
    ///
    /// Three native checks skip a whole subtree, so no identifier in it is ever
    /// resolved or reported:
    ///
    /// - **An empty `for…of` declaration list.** `checkForOfStatement`
    ///   (`checker.go:4051`) checks a declaration-list initializer **only**
    ///   through `checkVariableDeclarationList`; the right-hand side is reached
    ///   from each declaration's type (`getTypeForVariableLikeDeclaration`,
    ///   `checker.go:16664`, `checkRightHandSideOfForOf`). `for (var of X)` is
    ///   TS1123 with no declaration, so nothing checks `X` or anything inside
    ///   it. `checkForInStatement` (`checker.go:3988`) checks its expression
    ///   unconditionally and is not affected.
    /// - **A decorator on a node that cannot be decorated.** A decorator's
    ///   expression is checked only by `checkDecorators` (`checker.go:6022`),
    ///   reached from the accessor, method, class-like and variable-like
    ///   checks, which returns at once unless `ast.NodeCanBeDecorated`
    ///   (`ast/utilities.go:4254`) holds. Anything else is a grammar error
    ///   (TS1206, `checkGrammarModifiers`): `var v = @decorate class C {}`
    ///   under `experimentalDecorators` reports TS1206 and nothing on
    ///   `decorate` (`classExpressionWithDecorator1`).
    /// - **An enum member's computed name.** `checkEnumMember`
    ///   (`checker.go:5121`) checks the initializer and never the name;
    ///   `computeEnumMemberValue` reports TS1164 on a non-literal one without
    ///   checking its expression (`parserComputedPropertyName16`; r7-grammar
    ///   §3).
    ///
    /// One walk answers all three, testing the kind column before building a typed
    /// node: it runs for every value identifier, and a typed node per ancestor
    /// was most of its cost (`docs/parity/notes/r6-names.md` §12).
    pub(crate) fn names_in_unchecked_region(&self, node: NodeId) -> bool {
        let mut current = node;
        while let Some(parent) = self.nodes.parent(current) {
            match self.nodes.kind(parent) {
                SyntaxKind::ForOfStatement
                    if matches!(self.node_map.get(parent), Some(Node::ForInOrOfStatement(statement))
                        if statement.expression.and_then(|e| e.node_id()) == Some(current)
                            && matches!(statement.initializer,
                                Some(ForInitializer::VariableDeclarationList(list)) if list.declarations.is_empty())) =>
                {
                    return true;
                }
                SyntaxKind::Decorator
                    if self
                        .nodes
                        .parent(parent)
                        .is_some_and(|decorated| !self.names_decorators_are_checked(decorated)) =>
                {
                    return true;
                }
                SyntaxKind::ComputedPropertyName
                    if self.nodes.parent(parent).is_some_and(|member| {
                        self.nodes.kind(member) == SyntaxKind::EnumMember
                    }) =>
                {
                    return true;
                }
                _ => {}
            }
            current = parent;
        }
        false
    }
}

impl Checker<'_, '_> {
    /// `ast.IsInJSDoc` (`ast/utilities.go`): is `node` part of a JSDoc
    /// comment rather than of the file's own syntax?
    ///
    /// The type-reference reporter declined every name in a JavaScript file.
    /// Native declines none: `checkSourceElement` reaches a TypeScript-only
    /// annotation in a `.js` file (TS8010 is a separate, syntactic
    /// diagnostic, `getJSSyntacticDiagnosticsForFile`), and
    /// `getTypeFromTypeReference` resolves it as in TypeScript — the baselines
    /// of `fillInMissingTypeArgsOnJSConstructCalls` and
    /// `parserArrowFunctionExpression10`/`17` carry TS2304 beside TS8010.
    /// JSDoc type names keep the decline: their resolution
    /// (`resolveTypeReferenceName`'s JSDoc arms, typedef scopes) is the jsdoc
    /// lane's, and lifting it measured 6 cases lost
    /// (`docs/parity/notes/r6-names.md` §6).
    ///
    /// A reparsed JSDoc root has no parent edge here (the parser's
    /// `attach_jsdoc`), so a walk that ends anywhere but a source file is in
    /// a comment.
    pub(crate) fn names_in_jsdoc(&self, node: NodeId) -> bool {
        let mut current = node;
        loop {
            let kind = self.nodes.kind(current);
            if (SyntaxKind::JSDocTypeExpression..=SyntaxKind::JSDocImportTag).contains(&kind) {
                return true;
            }
            if kind == SyntaxKind::SourceFile {
                return false;
            }
            let Some(parent) = self.nodes.parent(current) else { return true };
            current = parent;
        }
    }
}

impl Checker<'_, '_> {
    /// Is `node` the leftmost name of a `typeof` entity name?
    ///
    /// `check_value_identifier` declines a reference spelled `null` because
    /// this parser makes an identifier of `class C extends null`, where
    /// native's makes a `NullKeyword`. A type query is the place both parsers
    /// make one: `parseTypeQuery` (`parser.go:3114`) parses its entity name
    /// with `allowReservedWords`, so `typeof null` is an identifier natively
    /// too, and `checkIdentifier` reports TS2304 *Cannot find name 'null'*
    /// (`invalidTypeOfTarget`). The decline must not fire there.
    pub(crate) fn names_in_type_query_entity_name(&self, node: NodeId) -> bool {
        let mut current = node;
        while let Some(parent) = self.nodes.parent(current) {
            match self.node_map.get(parent) {
                Some(Node::TypeQueryNode(query)) => {
                    return query.expr_name.and_then(|name| name.node_id()) == Some(current);
                }
                Some(Node::QualifiedName(name))
                    if name.left.and_then(|left| left.node_id()) == Some(current) =>
                {
                    current = parent;
                }
                _ => return false,
            }
        }
        false
    }
}

impl Checker<'_, '_> {
    /// `ast.NodeCanBeDecorated` (`ast/utilities.go:4254`) of `decorated`,
    /// under this program's decorator mode; see
    /// [`Checker::names_in_unchecked_region`].
    fn names_decorators_are_checked(&self, decorated: NodeId) -> bool {
        use tsr_ast::{BindingName, PropertyName, has_syntactic_modifier};
        let legacy = self.legacy_decorators;
        let parent = self.nodes.parent(decorated);
        let parent_kind = parent.map(|parent| self.nodes.kind(parent));
        let parent_is_class_declaration = parent_kind == Some(SyntaxKind::ClassDeclaration);
        let parent_is_class_like =
            matches!(parent_kind, Some(SyntaxKind::ClassDeclaration | SyntaxKind::ClassExpression));
        let private = |name: PropertyName<'_>| matches!(name, PropertyName::PrivateIdentifier(_));
        let member = |name: PropertyName<'_>, has_body: bool| {
            !(legacy && private(name))
                && has_body
                && if legacy { parent_is_class_declaration } else { parent_is_class_like }
        };
        match self.node_map.get(decorated) {
            Some(Node::ClassDeclaration(_)) => true,
            Some(Node::ClassExpression(_)) => !legacy,
            Some(Node::PropertyDeclaration(property)) => {
                !(legacy && private(property.name))
                    && if legacy {
                        parent_is_class_declaration
                    } else {
                        parent_is_class_like
                            && !has_syntactic_modifier(
                                property.modifiers,
                                SyntaxKind::AbstractKeyword,
                            )
                            && !has_syntactic_modifier(
                                property.modifiers,
                                SyntaxKind::DeclareKeyword,
                            )
                    }
            }
            Some(Node::MethodDeclaration(method)) => member(method.name, method.body.is_some()),
            Some(Node::GetAccessorDeclaration(accessor)) => {
                member(accessor.name, accessor.body.is_some())
            }
            Some(Node::SetAccessorDeclaration(accessor)) => {
                member(accessor.name, accessor.body.is_some())
            }
            Some(Node::ParameterDeclaration(_)) => {
                if !legacy {
                    return false;
                }
                let Some(function) = parent else { return false };
                let (has_body, parameters) = match self.node_map.get(function) {
                    Some(Node::ConstructorDeclaration(n)) => (n.body.is_some(), n.parameters),
                    Some(Node::MethodDeclaration(n)) => (n.body.is_some(), n.parameters),
                    Some(Node::SetAccessorDeclaration(n)) => (n.body.is_some(), n.parameters),
                    _ => return false,
                };
                // `GetThisParameter(parent) != node`.
                let this_parameter = parameters.first().filter(|first| {
                    matches!(first.name, Some(BindingName::Identifier(name)) if name.text == "this")
                });
                has_body
                    && this_parameter.and_then(|p| p.node_id) != Some(decorated)
                    && self.nodes.parent(function).map(|g| self.nodes.kind(g))
                        == Some(SyntaxKind::ClassDeclaration)
            }
            _ => false,
        }
    }
}
