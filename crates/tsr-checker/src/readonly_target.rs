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
use tsr_binder::SymbolFlags;

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

    /// Assigning to an identifier that names something other than a variable.
    ///
    /// `checkIdentifier`'s assignment-target arm (`checker.go:11076`), a
    /// six-way switch on **symbol flags alone**:
    ///
    /// | flag | message | code |
    /// |---|---|---|
    /// | `Enum` | `…because it is an enum` | TS2628 |
    /// | `Class` | `…because it is a class` | TS2629 |
    /// | `Module` | `…because it is a namespace` | TS2631 |
    /// | `Function` | `…because it is a function` | TS2630 |
    /// | `Alias` | `…because it is an import` | TS2632 |
    /// | — | `…because it is not a variable` | TS2539 |
    ///
    /// The order is upstream's and is load-bearing: a symbol merged from a
    /// `namespace` and a `function` is a namespace here, because that case
    /// comes first.
    ///
    /// Upstream's `isInJSFile && ValueModule` exemption is unreachable — §197
    /// excludes `allowJs` cases from this suite.
    pub(crate) fn check_identifier_assignment_target(&mut self, node: NodeId, ambient: bool) {
        if ambient || self.file_has_parse_errors {
            return;
        }
        if self.assignment_target_kind(node) == AssignmentTargetKind::None {
            return;
        }
        let Some(Node::Identifier(identifier)) = self.node_map.get(node) else { return };
        let Some(flags) = self.assignment_target_meaning(node, identifier.text) else { return };
        let message = if flags.intersects(SymbolFlags::ENUM) {
            &messages::CANNOT_ASSIGN_TO_0_BECAUSE_IT_IS_AN_ENUM
        } else if flags.intersects(SymbolFlags::CLASS) {
            &messages::CANNOT_ASSIGN_TO_0_BECAUSE_IT_IS_A_CLASS
        } else if flags.intersects(SymbolFlags::MODULE) {
            &messages::CANNOT_ASSIGN_TO_0_BECAUSE_IT_IS_A_NAMESPACE
        } else if flags.intersects(SymbolFlags::FUNCTION) {
            &messages::CANNOT_ASSIGN_TO_0_BECAUSE_IT_IS_A_FUNCTION
        } else if flags.intersects(SymbolFlags::ALIAS) {
            &messages::CANNOT_ASSIGN_TO_0_BECAUSE_IT_IS_AN_IMPORT
        } else {
            &messages::CANNOT_ASSIGN_TO_0_BECAUSE_IT_IS_NOT_A_VARIABLE
        };
        let Some(file) = self.source_file_of_for_diagnostics(node) else { return };
        let span = self.error_span(node);
        self.report(file, Diagnostic::with_args(message, span, [identifier.text.to_string()]));
    }

    /// The flags of what an assignment target *names*, when it is not a
    /// variable — the condition `checkIdentifier`'s arm fires on
    /// (`checker.go:11077`). `None` when the rule does not apply.
    ///
    /// Shared because upstream's arm ends `return c.errorType`, and that
    /// return is load-bearing for a **second** rule: an operand whose type is
    /// the error type is not asked whether it is arithmetic. `arithAssignTyping`
    /// wants twelve TS2629 and no TS2362, and this port emitted both. §253.
    pub(crate) fn assignment_target_meaning(
        &mut self,
        node: NodeId,
        text: &str,
    ) -> Option<SymbolFlags> {
        if self.assignment_target_kind(node) == AssignmentTargetKind::None {
            return None;
        }
        let symbol =
            self.binder.resolve_name(self.nodes, self.node_map, node, text, SymbolFlags::VALUE)?;
        // `getExportSymbolOfValueSymbolIfExported` — an exported declaration's
        // local carries `EXPORT_VALUE` and none of the real flags, so reading
        // the local would answer "not a variable" for every exported `var`.
        let symbol = self.binder.symbols().get(symbol).export_symbol.unwrap_or(symbol);
        let symbol = self.binder.merged_symbol(symbol);
        let flags = self.binder.symbols().get(symbol).flags;
        (!flags.intersects(SymbolFlags::VARIABLE)).then_some(flags)
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
        self.check_private_identifier_access(node);
        let Some((message, name, class_name, at)) = self.inaccessible_property(node) else {
            return;
        };
        let Some(file) = self.source_file_of_for_diagnostics(at) else { return };
        let span = self.error_span(at);
        self.report(file, Diagnostic::with_args(message, span, [name, class_name]));
    }

    /// TS18013 — `Property '{0}' is not accessible outside class '{1}' because
    /// it has a private identifier.`
    ///
    /// `checkPropertyAccessExpressionOrQualifiedName`'s private-name arm
    /// (`checker.go`), which `inaccessible_property` declines with *"`#x` is
    /// TS18013, a different code with its own row"*. This is that row.
    ///
    /// A `#name` is **lexically scoped to the class that declares it**, so the
    /// test is syntactic and needs no type: walk out from the access and report
    /// unless some enclosing class declares the name. §138.
    fn check_private_identifier_access(&mut self, node: NodeId) {
        let Some(Node::PropertyAccessExpression(access)) = self.node_map.get(node) else { return };
        let Some(tsr_ast::MemberName::PrivateIdentifier(name)) = access.name else { return };
        if self.enclosing_class_declares_private_name(node, name.text) {
            return;
        }
        // Upstream runs this against the receiver's **type**, and `any`
        // permits the access — `privateNameAndAny` was 3 of §138's 10 wrong
        // lines, an index signature the fourth. The syntactic test cannot see
        // either, so the receiver's type is asked here and only here. §139.
        let Some(receiver) = access.expression else { return };
        let receiver_type = self.check_expression(receiver);
        if receiver_type == self.intrinsics.any {
            return;
        }
        let Some(at) = name.node_id else { return };
        let Some(file) = self.source_file_of_for_diagnostics(at) else { return };
        let span = self.error_span(at);
        self.report(
            file,
            Diagnostic::with_args(
                &messages::PROPERTY_0_IS_NOT_ACCESSIBLE_OUTSIDE_CLASS_1_BECAUSE_IT_HAS_A_PRIVATE_IDENTIFIER,
                span,
                [name.text.to_string(), String::new()],
            ),
        );
    }

    /// Does any enclosing class declare this `#name`?
    fn enclosing_class_declares_private_name(&self, node: NodeId, text: &str) -> bool {
        self.nodes.ancestors(node).any(|ancestor| {
            let members: &[tsr_ast::ClassElement<'_>] = match self.node_map.get(ancestor) {
                Some(Node::ClassDeclaration(class)) => class.members,
                Some(Node::ClassExpression(class)) => class.members,
                _ => return false,
            };
            members.iter().any(|member| {
                let name = match member {
                    tsr_ast::ClassElement::PropertyDeclaration(n) => Some(n.name),
                    tsr_ast::ClassElement::MethodDeclaration(n) => Some(n.name),
                    tsr_ast::ClassElement::GetAccessorDeclaration(n) => Some(n.name),
                    tsr_ast::ClassElement::SetAccessorDeclaration(n) => Some(n.name),
                    _ => None,
                };
                matches!(name, Some(tsr_ast::PropertyName::PrivateIdentifier(p)) if p.text == text)
            })
        })
    }

    /// Is this property access inaccessible, and with which message?
    ///
    /// Split out because an inaccessible property's access answers `errorType`
    /// upstream (`checkPropertyAccessExpression` returns after reporting), so
    /// **no assignment check follows it**: `c.y = 1` on a private accessor is
    /// TS2341 alone and this port was adding a TS2322 beside it
    /// (`classPropertyAsPrivate`, `classPropertyAsProtected` —
    /// `checker-notes-diag2.md` §70).
    pub(crate) fn inaccessible_property(
        &mut self,
        node: NodeId,
    ) -> Option<(&'static tsr_diagnostics::Message, String, String, NodeId)> {
        let Some(Node::PropertyAccessExpression(access)) = self.node_map.get(node) else {
            return None;
        };
        let (Some(receiver), Some(member)) = (access.expression, access.name) else { return None };
        // `#x` is TS18013, a different code with its own row.
        let tsr_ast::MemberName::Identifier(name) = member else { return None };
        let name_id = name.node_id?;
        let receiver_type = self.check_expression(receiver);
        if self.is_error(receiver_type)
            || self.type_of(receiver_type).flags.intersects(TypeFlags::ANY_OR_UNKNOWN)
            || !self.declared_members_are_complete(receiver_type)
        {
            return None;
        }
        let property = self.get_property_of_type(receiver_type, name.text)?;
        // **Every** declaration must carry the modifier, not just the value one.
        // A `get`/`set` pair may diverge — `get PublicPrivate()` beside
        // `private set PublicPrivate(v)` — and upstream decides accessibility
        // from the accessor the *access kind* selects, a read from the getter
        // and a write from the setter. This port has no access-kind-selected
        // declaration, so a divergent pair is declined whole:
        // `divergentAccessorsVisibility1` was 12 wrong lines and
        // `accessorDeclarationOrder` was this rule's only loss (§67).
        let declarations = self.binder.symbols().get(property).declarations.clone();
        if declarations.is_empty() {
            return None;
        }
        let all_carry = |checker: &Self, keyword: SyntaxKind| {
            declarations
                .iter()
                .all(|&declaration| checker.member_declaration_has(declaration, keyword))
        };
        let is_private = all_carry(self, SyntaxKind::PrivateKeyword);
        let message = if is_private {
            &messages::PROPERTY_0_IS_PRIVATE_AND_ONLY_ACCESSIBLE_WITHIN_CLASS_1
        } else if all_carry(self, SyntaxKind::ProtectedKeyword) {
            &messages::PROPERTY_0_IS_PROTECTED_AND_ONLY_ACCESSIBLE_WITHIN_CLASS_1_AND_ITS_SUBCLASSES
        } else {
            return None;
        };
        let declaring = self.nodes.parent(declarations[0])?;
        // **Every** enclosing class, not the nearest one. `isNodeWithinClass`
        // and `forEachEnclosingClass` walk the whole chain, so a reference in a
        // class nested inside a subclass is still inside it —
        // `protectedClassPropertyAccessibleWithinNestedSubclass1` was 21 wrong
        // lines before this (§68).
        // A function with a **`this` parameter** carries the class through its
        // type rather than lexically, and upstream's accessibility check reads
        // the `this` type (`getThisTypeOfDeclaration`). This port has no such
        // reading, so the whole shape is declined —
        // `protectedMembersThisParameter`, `thisTypeAccessibility` and
        // `protectedAccessThroughContextualThis` (§68).
        if self.reference_is_inside_a_this_parameter_function(node) {
            return None;
        }
        let enclosing: Vec<NodeId> = self.enclosing_classes_of(node);
        let permitted = if is_private {
            enclosing.contains(&declaring)
        } else {
            // `protected`: an enclosing class must **derive from** the
            // declaring one (§68). The walk declines the moment it cannot
            // follow a link, which is `base_symbols_of`'s contract at a
            // different question.
            enclosing.iter().any(|&class| self.class_derives_from(class, declaring))
        };
        if permitted {
            return None;
        }
        let class_name = self.declaration_name_of_class(declaring)?;
        Some((message, name.text.to_string(), class_name, name_id))
    }

    /// Does `class` reach `base` through its `extends` chain, or **is** it
    /// `base`? A link this port cannot follow answers `false`, which is the
    /// reporting direction — see §68's falsifier.
    fn class_derives_from(&self, class: NodeId, base: NodeId) -> bool {
        let mut current = class;
        for _ in 0..16 {
            if current == base {
                return true;
            }
            let Some(next) = self.extends_class_declaration(current) else { return false };
            current = next;
        }
        false
    }

    /// The class declaration a class `extends`, when the heritage names one
    /// non-generic class this port can resolve — §56's shape.
    fn extends_class_declaration(&self, class: NodeId) -> Option<NodeId> {
        let heritage = match self.node_map.get(class)? {
            Node::ClassDeclaration(declaration) => declaration.heritage_clauses,
            Node::ClassExpression(declaration) => declaration.heritage_clauses,
            _ => return None,
        };
        let clause =
            heritage.iter().find(|clause| clause.token.kind == SyntaxKind::ExtendsKeyword)?;
        let [base] = clause.types else { return None };
        let expression = base.expression?.node_id()?;
        let Some(Node::Identifier(written)) = self.node_map.get(expression) else { return None };
        let symbol = self.binder.resolve_name(
            self.nodes,
            self.node_map,
            expression,
            written.text,
            tsr_binder::SymbolFlags::VALUE,
        )?;
        let symbol = self.binder.merged_symbol(symbol);
        let entry = self.binder.symbols().get(symbol);
        if !entry.flags.intersects(tsr_binder::SymbolFlags::CLASS) || entry.declarations.len() != 1
        {
            return None;
        }
        let candidate = entry.declarations[0];
        matches!(
            self.nodes.kind(candidate),
            SyntaxKind::ClassDeclaration | SyntaxKind::ClassExpression
        )
        .then_some(candidate)
    }

    /// Does this class member carry the given accessibility modifier?
    fn member_declaration_has(&self, declaration: NodeId, keyword: SyntaxKind) -> bool {
        let modifiers = match self.node_map.get(declaration) {
            Some(Node::PropertyDeclaration(property)) => property.modifiers,
            Some(Node::MethodDeclaration(method)) => method.modifiers,
            Some(Node::GetAccessorDeclaration(accessor)) => accessor.modifiers,
            Some(Node::SetAccessorDeclaration(accessor)) => accessor.modifiers,
            Some(Node::ParameterDeclaration(parameter)) => parameter.modifiers,
            _ => return false,
        };
        modifiers.iter().any(|modifier| {
            matches!(modifier, tsr_ast::ModifierLike::Token(token) if token.kind == keyword)
        })
    }

    /// Is the reference inside a function-like declaration that names a `this`
    /// parameter? See §68.
    fn reference_is_inside_a_this_parameter_function(&self, node: NodeId) -> bool {
        self.nodes.ancestors(node).any(|ancestor| {
            let parameters: &[&tsr_ast::ParameterDeclaration<'_>] =
                match self.node_map.get(ancestor) {
                    Some(Node::FunctionDeclaration(n)) => n.parameters,
                    Some(Node::FunctionExpression(n)) => n.parameters,
                    Some(Node::ArrowFunction(n)) => n.parameters,
                    Some(Node::MethodDeclaration(n)) => n.parameters,
                    _ => return false,
                };
            parameters.iter().any(|parameter| {
                matches!(parameter.name, Some(tsr_ast::BindingName::Identifier(name))
                    if name.text == "this")
            })
        })
    }

    /// Every enclosing class declaration or expression, innermost first —
    /// `isNodeWithinClass` / `forEachEnclosingClass`.
    fn enclosing_classes_of(&self, node: NodeId) -> Vec<NodeId> {
        self.nodes
            .ancestors(node)
            .filter(|&ancestor| {
                matches!(
                    self.nodes.kind(ancestor),
                    SyntaxKind::ClassDeclaration | SyntaxKind::ClassExpression
                )
            })
            .collect()
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
