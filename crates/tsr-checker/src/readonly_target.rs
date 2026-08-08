//! TS2540 — `Cannot assign to '{0}' because it is a read-only property.`
//!
//! `checkPropertyAccessExpressionOrQualifiedName` (`checker.go:11376`), whose
//! test is `isAssignmentToReadonlyEntity` (`checker.go:27279`) over
//! `isReadonlySymbol` (`checker.go:13849`).
//!
//! # What `isReadonlySymbol` is, and the two rows this port cannot read
//!
//! Upstream's predicate is five facts about a symbol and its declarations.
//! Four are readable off the declarations here: a `readonly` modifier on a
//! property, a `const` variable, an accessor with no setter, and an enum
//! member. The other two — `CheckFlagsReadonly` (a computed flag on
//! synthesised union and intersection properties) and
//! `isReadonlyAssignmentDeclaration` (`Object.defineProperty`) — are unported,
//! and both omissions can only cost a *missing* diagnostic.
//!
//! # The constructor exception is the rule's only real decision
//!
//! `this.x = …` is permitted inside the constructor of the class that declares
//! `x`. Getting that wrong reports on ordinary, correct code, so the whole
//! disjunction is reproduced rather than approximated.
//!
//! `docs/architecture/checker-notes-diag2.md` §53.

use tsr_ast::{Node, NodeId, SyntaxKind};
use tsr_binder::SymbolId;
use tsr_diagnostics::{Diagnostic, messages};

use crate::{checker::Checker, expressions::AssignmentTargetKind, flags::TypeFlags};

impl Checker<'_, '_> {
    /// The read-only check for one `x.y = …` target.
    pub(crate) fn check_readonly_assignment_target(&mut self, node: NodeId, ambient: bool) {
        if ambient || self.file_has_parse_errors {
            return;
        }
        if self.assignment_target_kind(node) == AssignmentTargetKind::None {
            return;
        }
        let Some(Node::PropertyAccessExpression(access)) = self.node_map.get(node) else { return };
        let (Some(receiver), Some(member)) = (access.expression, access.name) else { return };
        let tsr_ast::MemberName::Identifier(name) = member else { return };
        let Some(name_id) = name.node_id else { return };
        if access.question_dot_token.is_some() {
            return;
        }
        let receiver_type = self.check_expression(receiver);
        // The same gate `crate::nonexistent_property` uses: a receiver whose
        // members this port did not finish resolving cannot be asked whether
        // one of them is read-only either.
        if self.is_error(receiver_type)
            || self.type_of(receiver_type).flags.intersects(TypeFlags::ANY_OR_UNKNOWN)
            || !self.declared_members_are_complete(receiver_type)
        {
            return;
        }
        let Some(property) = self.get_property_of_type(receiver_type, name.text) else { return };
        if !self.is_readonly_symbol(property) && !self.property_signature_is_readonly(property) {
            return;
        }
        if self.assignment_is_inside_the_declaring_constructor(node, property) {
            return;
        }
        let Some(file) = self.source_file_of_for_diagnostics(name_id) else { return };
        let span = self.error_span(name_id);
        self.report(
            file,
            Diagnostic::with_args(
                &messages::CANNOT_ASSIGN_TO_0_BECAUSE_IT_IS_A_READ_ONLY_PROPERTY,
                span,
                [name.text.to_string()],
            ),
        );
    }

    /// `isReadonlySymbol`'s **property-signature** row, which
    /// [`Checker::is_readonly_symbol`] does not cover.
    ///
    /// That function is `crate::flow`'s, built for `checker-notes-narrow.md`
    /// §27, and it reads the `readonly` modifier off a `PropertyDeclaration`
    /// only. An interface member — `interface I { readonly x: number }` — is a
    /// `PropertySignature`, and `readonlyPropertySubtypeRelationDirected` and
    /// `externalModuleImmutableBindings` are that shape. Supplemented here
    /// rather than widened there, because widening a function the query road
    /// reads moves `checker_types`.
    fn property_signature_is_readonly(&self, symbol: SymbolId) -> bool {
        self.binder.symbols().get(symbol).declarations.iter().any(|declaration| {
            matches!(
                self.node_map.get(*declaration),
                Some(Node::PropertySignatureDeclaration(signature))
                    if signature.modifiers.iter().any(|modifier| {
                        matches!(modifier, tsr_ast::ModifierLike::Token(token)
                            if token.kind == SyntaxKind::ReadonlyKeyword)
                    })
            )
        })
    }

    /// `isAssignmentToReadonlyEntity`'s constructor permission
    /// (`checker.go:27296`): `this.x = …` inside the constructor of the class
    /// that declares `x`, or whose parameter declares it.
    fn assignment_is_inside_the_declaring_constructor(
        &mut self,
        access: NodeId,
        property: SymbolId,
    ) -> bool {
        let Some(Node::PropertyAccessExpression(expression)) = self.node_map.get(access) else {
            return false;
        };
        if expression
            .expression
            .and_then(|receiver| receiver.node_id())
            .is_none_or(|receiver| self.nodes.kind(receiver) != SyntaxKind::ThisKeyword)
        {
            return false;
        }
        let Some(constructor) = self.control_flow_container(access) else { return true };
        if self.nodes.kind(constructor) != SyntaxKind::Constructor {
            // Upstream returns `true` from `isAssignmentToReadonlyEntity` — the
            // assignment IS an error — when the container is not a
            // constructor, so this is the reporting direction.
            return false;
        }
        let Some(class) = self.nodes.parent(constructor) else { return false };
        let Some(declaration) = self.binder.symbols().get(property).value_declaration else {
            return false;
        };
        // `isLocalPropertyDeclaration` (the class is the declaration's parent)
        // or `isLocalParameterProperty` (the constructor is).
        self.nodes.parent(declaration) == Some(class)
            || self.nodes.parent(declaration) == Some(constructor)
    }

    /// TS2341 — `Property '{0}' is private and only accessible within class
    /// '{1}'.`
    ///
    /// `checkPropertyAccessibility`'s `private` half. Entirely syntactic once
    /// the property symbol is in hand: the declaration carries a `private`
    /// modifier and the reference is not inside the class that declares it.
    /// `protected` (TS2445) needs the `extends` chain and is not built here —
    /// `docs/architecture/checker-notes-diag2.md` §67.
    pub(crate) fn check_private_property_access(&mut self, node: NodeId, ambient: bool) {
        if ambient || self.file_has_parse_errors || self.in_js_file(node) {
            return;
        }
        let Some(Node::PropertyAccessExpression(access)) = self.node_map.get(node) else { return };
        let (Some(receiver), Some(member)) = (access.expression, access.name) else { return };
        // `#x` is TS18013, a different code with its own row.
        let tsr_ast::MemberName::Identifier(name) = member else { return };
        let Some(name_id) = name.node_id else { return };
        let receiver_type = self.check_expression(receiver);
        if self.is_error(receiver_type)
            || self.type_of(receiver_type).flags.intersects(TypeFlags::ANY_OR_UNKNOWN)
            || !self.declared_members_are_complete(receiver_type)
        {
            return;
        }
        let Some(property) = self.get_property_of_type(receiver_type, name.text) else { return };
        // **Every** declaration must carry `private`, not just the value one.
        // A `get`/`set` pair may diverge — `get PublicPrivate()` beside
        // `private set PublicPrivate(v)` — and upstream decides accessibility
        // from the accessor the *access kind* selects, a read from the getter
        // and a write from the setter. This port has no access-kind-selected
        // declaration, so a divergent pair is declined whole:
        // `divergentAccessorsVisibility1` was 12 wrong lines and
        // `accessorDeclarationOrder` was this rule's only loss (§67).
        let declarations = self.binder.symbols().get(property).declarations.clone();
        if declarations.is_empty()
            || !declarations
                .iter()
                .all(|&declaration| self.member_declaration_is_private(declaration))
        {
            return;
        }
        let Some(declaring) = self.nodes.parent(declarations[0]) else { return };
        if self.enclosing_class_of(node) == Some(declaring) {
            return;
        }
        let Some(class_name) = self.declaration_name_of_class(declaring) else { return };
        let Some(file) = self.source_file_of_for_diagnostics(name_id) else { return };
        let span = self.error_span(name_id);
        self.report(
            file,
            Diagnostic::with_args(
                &messages::PROPERTY_0_IS_PRIVATE_AND_ONLY_ACCESSIBLE_WITHIN_CLASS_1,
                span,
                [name.text.to_string(), class_name],
            ),
        );
    }

    /// Does this class member carry a `private` modifier?
    fn member_declaration_is_private(&self, declaration: NodeId) -> bool {
        let modifiers = match self.node_map.get(declaration) {
            Some(Node::PropertyDeclaration(property)) => property.modifiers,
            Some(Node::MethodDeclaration(method)) => method.modifiers,
            Some(Node::GetAccessorDeclaration(accessor)) => accessor.modifiers,
            Some(Node::SetAccessorDeclaration(accessor)) => accessor.modifiers,
            Some(Node::ParameterDeclaration(parameter)) => parameter.modifiers,
            _ => return false,
        };
        modifiers.iter().any(|modifier| {
            matches!(modifier, tsr_ast::ModifierLike::Token(token)
                if token.kind == SyntaxKind::PrivateKeyword)
        })
    }

    /// The nearest enclosing class declaration or expression —
    /// `getContainingClass`.
    fn enclosing_class_of(&self, node: NodeId) -> Option<NodeId> {
        self.nodes.ancestors(node).find(|&ancestor| {
            matches!(
                self.nodes.kind(ancestor),
                SyntaxKind::ClassDeclaration | SyntaxKind::ClassExpression
            )
        })
    }

    /// A class's written name, for the message's second argument.
    fn declaration_name_of_class(&self, class: NodeId) -> Option<String> {
        let name = match self.node_map.get(class)? {
            Node::ClassDeclaration(declaration) => declaration.name?,
            Node::ClassExpression(declaration) => declaration.name?,
            _ => return None,
        };
        Some(name.text.to_string())
    }
}
