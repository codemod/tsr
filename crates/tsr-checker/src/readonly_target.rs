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
        self.check_readonly_index_signature_write(node);
        if self.assignment_target_kind(node) == AssignmentTargetKind::None {
            return;
        }
        // §326 — upstream's `checkReferenceExpression` reaches an
        // `ElementAccessExpression` too, and `M["x"]` names a property exactly
        // as `M.x` does. A **computed** key names no particular property and
        // declines, the same bound `nonexistent_property` takes.
        let (receiver, name_id, name_text, optional) = match self.node_map.get(node) {
            Some(Node::PropertyAccessExpression(access)) => {
                let Some(receiver) = access.expression else { return };
                // A **private name** is a separate `MemberName` variant, and
                // `this.#roProp = ""` on a getter-only accessor is TS2540
                // exactly as `this.roProp = ""` is. §385.
                let (id, text) = match access.name {
                    Some(tsr_ast::MemberName::Identifier(name)) => {
                        let Some(id) = name.node_id else { return };
                        (id, name.text)
                    }
                    Some(tsr_ast::MemberName::PrivateIdentifier(name)) => {
                        let Some(id) = name.node_id else { return };
                        (id, name.text)
                    }
                    None => return,
                };
                (receiver, id, text, access.question_dot_token.is_some())
            }
            Some(Node::ElementAccessExpression(access)) => {
                let (Some(receiver), Some(argument)) =
                    (access.expression, access.argument_expression)
                else {
                    return;
                };
                let Some(tsr_ast::Expression::StringLiteral(literal)) = Some(argument) else {
                    return;
                };
                let Some(id) = literal.node_id else { return };
                (receiver, id, literal.text, access.question_dot_token.is_some())
            }
            _ => return,
        };
        let name = name_text;
        if optional {
            return;
        }
        let receiver_type = self.check_expression(receiver);
        // A found readonly property is a positive answer; unlike an absence
        // it needs no member-completeness certificate (§944.1's measurement
        // for the type road applies to this one too).
        if self.is_error(receiver_type)
            || self.type_of(receiver_type).flags.intersects(TypeFlags::ANY_OR_UNKNOWN)
        {
            return;
        }
        let receiver_type = self.apparent_type(receiver_type);
        // A union property one constituent lacks (with no applicable index
        // signature) is partial: `getPropertyOfType` answers nil and the
        // access reports TS2339, not this.
        if let crate::types::TypeData::Union { types, .. } =
            self.store.get(receiver_type).data.clone()
        {
            let key = self.store.intern_literal(
                TypeFlags::STRING_LITERAL,
                crate::types::TypeData::StringLiteral(name.to_string()),
                false,
            );
            for part in types {
                let apparent = self.apparent_type(part);
                if self.get_property_of_type(apparent, name).is_none()
                    && self.get_applicable_index_info(apparent, key).is_none()
                {
                    return;
                }
            }
        }
        // isAssignmentToReadonlyEntity's last arm: a property found through a
        // namespace import is readonly whatever its own declaration says.
        if !self.is_assignment_to_readonly_property(node, receiver_type, name)
            && (self.receiver_alias_is_namespace_import(receiver) != Some(true)
                || self.get_property_of_type(receiver_type, name).is_none())
        {
            return;
        }
        let Some(file) = self.source_file_of_for_diagnostics(name_id) else { return };
        let span = self.error_span(name_id);
        self.report(
            file,
            Diagnostic::with_args(
                &messages::CANNOT_ASSIGN_TO_0_BECAUSE_IT_IS_A_READ_ONLY_PROPERTY,
                span,
                [name.to_string()],
            ),
        );
    }

    /// TS2542 — `Index signature in type '{0}' only permits reading.`
    ///
    /// An access answered by a readonly index signature, written or deleted:
    /// `checkPropertyAccessExpressionOrQualifiedName`'s index arm
    /// (`checker.go:11354`) at the whole access, and
    /// `getPropertyTypeForIndexType`'s `errorIfWritingToReadonlyIndex`
    /// (`checker.go:27277`) at the element access. Both print the apparent
    /// receiver. A union receiver declines: its per-constituent property
    /// lookup (`createUnionOrIntersectionProperty`) is not this one.
    fn check_readonly_index_signature_write(&mut self, node: NodeId) {
        if self.assignment_target_kind(node) == AssignmentTargetKind::None
            && !self.is_delete_target(node)
        {
            return;
        }
        let (receiver, written_name, argument) = match self.node_map.get(node) {
            Some(Node::PropertyAccessExpression(access)) => {
                if access.question_dot_token.is_some() {
                    return;
                }
                let (Some(receiver), Some(tsr_ast::MemberName::Identifier(name))) =
                    (access.expression, access.name)
                else {
                    return;
                };
                (receiver, Some(name.text), None)
            }
            Some(Node::ElementAccessExpression(access)) => {
                if access.question_dot_token.is_some() {
                    return;
                }
                let (Some(receiver), Some(argument)) =
                    (access.expression, access.argument_expression)
                else {
                    return;
                };
                (receiver, None, Some(argument))
            }
            _ => return,
        };
        let receiver_type = self.check_expression(receiver);
        if self.is_error(receiver_type)
            || self
                .type_of(receiver_type)
                .flags
                .intersects(TypeFlags::ANY_OR_UNKNOWN | TypeFlags::NEVER)
        {
            return;
        }
        let apparent = self.apparent_type(receiver_type);
        if self.type_of(apparent).flags.intersects(TypeFlags::INTERSECTION)
            || written_name.is_some() && self.type_of(apparent).flags.intersects(TypeFlags::UNION)
        {
            return;
        }
        // getPropertyNameFromIndex: a usable literal key names a property,
        // and a found property is the 2540 road, not an index read.
        let key = if let Some(name) = written_name {
            if self.get_property_of_type(apparent, name).is_some() {
                return;
            }
            None
        } else {
            let Some(argument) = argument else { return };
            let key = self.check_expression(argument);
            if self.is_error(key) {
                return;
            }
            let property_name = match &self.type_of(key).data {
                crate::types::TypeData::StringLiteral(text)
                | crate::types::TypeData::NumberLiteral(text) => Some(text.clone()),
                _ => None,
            };
            if let Some(name) = property_name {
                if self.get_property_of_type(apparent, &name).is_some() {
                    return;
                }
            } else if !self.type_of(key).flags.intersects(
                TypeFlags::STRING_LIKE | TypeFlags::NUMBER_LIKE | TypeFlags::ES_SYMBOL_LIKE,
            ) {
                return;
            }
            Some(key)
        };
        // Only a readonly signature can answer; skip the applicability
        // relation for the common mutable `array[i] = v`.
        if !self
            .get_index_infos_of_type(apparent)
            .is_some_and(|infos| infos.iter().any(|info| info.readonly))
        {
            return;
        }
        let key = match (key, written_name) {
            (Some(key), _) => key,
            (None, Some(name)) => self.store.intern_literal(
                TypeFlags::STRING_LITERAL,
                crate::types::TypeData::StringLiteral(name.to_string()),
                false,
            ),
            (None, None) => return,
        };
        let Some(info) = self.get_applicable_index_info(apparent, key) else { return };
        if !info.readonly {
            return;
        }
        let Some(file) = self.source_file_of_for_diagnostics(node) else { return };
        let span = self.error_span(node);
        let printed = self.type_to_string(apparent);
        self.report(
            file,
            Diagnostic::with_args(
                &messages::INDEX_SIGNATURE_IN_TYPE_0_ONLY_PERMITS_READING,
                span,
                [printed],
            ),
        );
    }

    /// `isDeleteTarget` (`utilities.go:2059`): the operand of `delete`, through
    /// parentheses.
    fn is_delete_target(&self, node: NodeId) -> bool {
        let mut current = node;
        while let Some(parent) = self.nodes.parent(current) {
            match self.nodes.kind(parent) {
                SyntaxKind::ParenthesizedExpression => current = parent,
                SyntaxKind::DeleteExpression => return true,
                _ => return false,
            }
        }
        false
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
    ///
    /// Runs in files with parse errors, as upstream's `checkIdentifier` does:
    /// the rule reads only the identifier and what it resolves to, which
    /// recovery preserves (`docs/parity/notes/misc-checks.md` §12).
    pub(crate) fn check_identifier_assignment_target(&mut self, node: NodeId, ambient: bool) {
        if ambient {
            return;
        }
        if self.assignment_target_kind(node) == AssignmentTargetKind::None {
            return;
        }
        let Some(Node::Identifier(identifier)) = self.node_map.get(node) else { return };
        // `getResolvedSymbol` resolves nothing for a missing identifier
        // (`!ast.NodeIsMissing(node)`, `checker.go:13894`): parser recovery's
        // empty name must not find a declaration whose name was also lost.
        if identifier.text.is_empty() {
            return;
        }
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
    pub(crate) fn assignment_target_symbol(
        &mut self,
        node: NodeId,
        text: &str,
    ) -> Option<(SymbolId, SymbolFlags)> {
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
        Some((symbol, flags))
    }

    /// The §243 arm's condition: the target names something that is **not** a
    /// variable.
    pub(crate) fn assignment_target_meaning(
        &mut self,
        node: NodeId,
        text: &str,
    ) -> Option<SymbolFlags> {
        let (_, flags) = self.assignment_target_symbol(node, text)?;
        (!flags.intersects(SymbolFlags::VARIABLE)).then_some(flags)
    }

    /// TS2588 — `Cannot assign to '{0}' because it is a constant.`
    /// TS2540 — `Cannot assign to '{0}' because it is a read-only property.`
    ///
    /// `checkIdentifier`'s **second** assignment arm (`checker.go:11095`), four
    /// lines below the one §243 ported and mutually exclusive with it: that one
    /// fires when the symbol has no `Variable` flag, this one when it does and
    /// the symbol is readonly.
    ///
    /// Upstream's `return c.errorType` is on both arms, so §253's arithmetic
    /// decline has to cover this one too — otherwise `const x = 1; x += 1`
    /// reports TS2588 *and* TS2362 where upstream reports one. §282.
    pub(crate) fn check_readonly_identifier_assignment(&mut self, node: NodeId, ambient: bool) {
        if ambient || self.file_has_parse_errors {
            return;
        }
        let Some(Node::Identifier(identifier)) = self.node_map.get(node) else { return };
        let Some((symbol, flags)) = self.assignment_target_symbol(node, identifier.text) else {
            return;
        };
        if !flags.intersects(SymbolFlags::VARIABLE) {
            // §243's arm owns this shape.
            return;
        }
        if !self.is_readonly_symbol(symbol) {
            return;
        }
        let message = if flags.intersects(SymbolFlags::VARIABLE) {
            &messages::CANNOT_ASSIGN_TO_0_BECAUSE_IT_IS_A_CONSTANT
        } else {
            &messages::CANNOT_ASSIGN_TO_0_BECAUSE_IT_IS_A_READ_ONLY_PROPERTY
        };
        let Some(file) = self.source_file_of_for_diagnostics(node) else { return };
        let span = self.error_span(node);
        self.report(file, Diagnostic::with_args(message, span, [identifier.text.to_string()]));
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
    /// §944: is `node` a property access standing as the LEFT side of a plain
    /// assignment, naming a `readonly` property?
    ///
    /// The type road's half of `check_readonly_assignment_target`. Restricted to
    /// `=` — a compound assignment (`+=`) reads the property as well as writing
    /// it, and upstream's own answer there is a different question this port has
    /// no row for.
    pub(crate) fn is_readonly_assignment_target(&mut self, node: NodeId) -> bool {
        let Some(parent) = self.nodes.parent(node) else { return false };
        let Some(Node::BinaryExpression(binary)) = self.node_map.get(parent) else {
            return false;
        };
        if binary.operator_token.is_none_or(|token| token.kind != SyntaxKind::EqualsToken)
            || binary.left.and_then(|left| left.node_id()) != Some(node)
        {
            return false;
        }
        let Some(Node::PropertyAccessExpression(access)) = self.node_map.get(node) else {
            return false;
        };
        let Some(tsr_ast::MemberName::Identifier(name)) = access.name else { return false };
        let name = name.text.to_string();
        let Some(receiver) = access.expression else { return false };
        let receiver_type = self.check_expression(receiver);
        // **No `declared_members_are_complete` gate here, and §944.1 measured
        // why.** The diagnostic road carries one — *"a receiver whose members
        // this port did not finish resolving cannot be asked whether one of them
        // is read-only either"* — and that is right for a DIAGNOSTIC, where a
        // false positive is a reported error the user did not earn.
        //
        // The type road's exposure is the other way round: it only ever answers
        // `any` for a property `get_property_of_type` **found** and
        // `property_signature_is_readonly` **confirmed**, so an incomplete
        // receiver costs a missed answer rather than an invented one.
        //
        // The gate was what kept §933's own four rows open: every readonly
        // member of `Int32Array`/`BigInt64Array` failed it while the same shape
        // written by hand — a generic interface, a defaulted parameter, a merged
        // declaration — passed. Lifting it is **+6 `WRONG->RIGHT`, zero
        // adverse**, and 4 of the 6 are `bigintWithLib`: §933's falsifier,
        // resolved.
        if self.is_error(receiver_type)
            || self.type_of(receiver_type).flags.intersects(TypeFlags::ANY_OR_UNKNOWN)
        {
            return false;
        }
        self.is_assignment_to_readonly_property(node, receiver_type, &name)
    }

    /// `isAssignmentToReadonlyEntity` (`checker.go:27279`) once the access is
    /// known to be an assignment target: the property `name` of
    /// `receiver_type` is readonly (`isReadonlySymbol`, including the
    /// `CheckFlagsReadonly` of union and intersection properties and of
    /// mapped `readonly` modifiers) and the write is not inside its declaring
    /// constructor. Shared by the type road and the TS2540 diagnostic.
    fn is_assignment_to_readonly_property(
        &mut self,
        node: NodeId,
        receiver_type: crate::types::TypeId,
        name: &str,
    ) -> bool {
        let name = name.to_string();
        // Tuple targets synthesize a readonly `length` property; there is no
        // binder symbol for it to carry `CheckFlagsReadonly` in this port.
        if name == "length" && self.tuple_is_readonly(receiver_type) {
            return true;
        }
        // §952: a homomorphic `readonly` mapping makes EVERY member read-only,
        // and the reused member owner cannot say so — the modifier lives beside
        // the mint. `Readonly<Bar>`'s `x4.a = 1` is upstream's error and the
        // target prints `any` (`mappedTypes6`); without this the members arm
        // answered `number` there and cost 2 `RIGHT->WRONG`.
        //
        // `-readonly` is the mirror: `Readwrite<Bar>`'s `x5.b = 1` is legal even
        // though `Bar.b` is declared `readonly`, so the modifier OVERRIDES the
        // source's own answer in both directions rather than only adding to it.
        if let Some(&(_, readonly)) = self.mapped_identity_optionality.get(&receiver_type)
            && let Some(readonly) = readonly
        {
            return readonly && self.get_property_of_type(receiver_type, &name).is_some();
        }
        if self.store.get(receiver_type).flags.contains(TypeFlags::INTERSECTION) {
            let properties = self.intersection_property_symbols(receiver_type, &name);
            return !properties.is_empty()
                && properties.into_iter().all(|property| {
                    (self.is_readonly_symbol(property)
                        || self.property_signature_is_readonly(property))
                        && !self.assignment_is_inside_the_declaring_constructor(node, property)
                });
        }
        // createUnionOrIntersectionProperty marks a union property readonly
        // when a constituent contributes a readonly index signature.
        if let crate::types::TypeData::Union { types, .. } =
            self.store.get(receiver_type).data.clone()
        {
            let key = self.store.intern_literal(
                TypeFlags::STRING_LITERAL,
                crate::types::TypeData::StringLiteral(name.clone()),
                false,
            );
            for part in types {
                let apparent = self.apparent_type(part);
                if let Some(property) = self.get_property_of_type(apparent, &name) {
                    if self.is_readonly_symbol(property)
                        || self.property_signature_is_readonly(property)
                    {
                        return true;
                    }
                } else if self
                    .get_applicable_index_info(apparent, key)
                    .is_some_and(|index| index.readonly)
                {
                    return true;
                }
            }
        }
        let Some(property) = self.get_property_of_type(receiver_type, &name) else { return false };
        // `checkObjectLiteral` (`checker.go:13175`) gives every member of a
        // const-context literal `CheckFlagsReadonly`; this port records it on
        // the literal's captured member image, not the binder symbol.
        let literal_member_readonly =
            self.anonymous_properties.get(&receiver_type).is_some_and(|(properties, _)| {
                properties.iter().any(|member| member.name == name && member.readonly)
            });
        if !literal_member_readonly
            && !self.is_readonly_symbol(property)
            && !self.property_signature_is_readonly(property)
        {
            return false;
        }
        // The constructor permission is upstream's own
        // (`isAssignmentToReadonlyEntity`, `checker.go:27296`) and applies to the
        // TYPE as much as the diagnostic: inside the declaring constructor the
        // assignment is legal and the reference is not erroneous.
        !self.assignment_is_inside_the_declaring_constructor(node, property)
    }

    /// `getDeclarationModifierFlagsFromSymbol(symbol)&ModifierFlagsReadonly`
    /// for the declaration kinds `is_readonly_symbol` does not read: a
    /// property signature and a `readonly` parameter property.
    pub(crate) fn property_signature_is_readonly(&self, symbol: SymbolId) -> bool {
        let is_readonly = |modifiers: &[tsr_ast::ModifierLike<'_>]| {
            modifiers.iter().any(|modifier| {
                matches!(modifier, tsr_ast::ModifierLike::Token(token)
                    if token.kind == SyntaxKind::ReadonlyKeyword)
            })
        };
        let entry = self.binder.symbols().get(symbol);
        entry.declarations.iter().any(|declaration| match self.node_map.get(*declaration) {
            Some(Node::PropertySignatureDeclaration(signature)) => is_readonly(signature.modifiers),
            Some(Node::ParameterDeclaration(parameter)) => {
                entry.flags.intersects(SymbolFlags::PROPERTY) && is_readonly(parameter.modifiers)
            }
            _ => false,
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

    /// TS2341/TS2445/TS2446 — `checkPropertyAccessibility` for a dotted
    /// access, instance or static; see [`Checker::inaccessible_property`].
    pub(crate) fn check_private_property_access(&mut self, node: NodeId, ambient: bool) {
        if ambient || self.file_has_parse_errors {
            return;
        }
        self.check_private_method_assignment(node);
        // The `#name` arm keeps its JS decline (`privateIdentifierExpando`);
        // the accessibility arm below reads JSDoc `@private`/`@protected`.
        if !self.in_js_file(node) {
            self.check_private_identifier_access(node);
        }
        let Some((message, arguments, at)) = self.inaccessible_property(node) else {
            return;
        };
        let Some(file) = self.source_file_of_for_diagnostics(at) else { return };
        let span = self.error_span(at);
        self.report(file, Diagnostic::with_args(message, span, arguments));
    }

    /// TS2803 — `Cannot assign to private method '{0}'. Private methods are
    /// not writable.`
    ///
    /// `checkPropertyAccessExpressionOrQualifiedName`'s private-name arm
    /// (`checker.go:11280`): an assignment target whose
    /// `lookupSymbolForPrivateIdentifierDeclaration` symbol has a method as
    /// its `valueDeclaration`, reported with `grammarErrorOnNode` at the name.
    /// It asks only the lexical symbol, never the receiver's type, so `b.#m =
    /// …` with `b: any` reports too.
    #[inline(never)]
    fn check_private_method_assignment(&mut self, node: NodeId) {
        let Some(Node::PropertyAccessExpression(access)) = self.node_map.get(node) else { return };
        let Some(tsr_ast::MemberName::PrivateIdentifier(name)) = access.name else { return };
        let Some(at) = name.node_id else { return };
        if self.assignment_target_kind(node) == AssignmentTargetKind::None {
            return;
        }
        let Some(class) = self.lexical_private_declaring_class(node, name.text) else { return };
        if !self.private_name_value_declaration_is_method(class, name.text) {
            return;
        }
        let Some(file) = self.source_file_of_for_diagnostics(at) else { return };
        let span = self.nodes.span(at);
        self.report(
            file,
            Diagnostic::with_args(
                &messages::CANNOT_ASSIGN_TO_PRIVATE_METHOD_0_PRIVATE_METHODS_ARE_NOT_WRITABLE,
                span,
                [name.text.to_string()],
            ),
        );
    }

    /// Is the `valueDeclaration` of `class`'s private member `text` a method?
    /// The binder's `valueDeclaration` is the first value declaration in
    /// member order (`SetValueDeclaration` keeps the first).
    fn private_name_value_declaration_is_method(&self, class: NodeId, text: &str) -> bool {
        let members: &[tsr_ast::ClassElement<'_>] = match self.node_map.get(class) {
            Some(Node::ClassDeclaration(n)) => n.members,
            Some(Node::ClassExpression(n)) => n.members,
            _ => return false,
        };
        let is_name = |name: tsr_ast::PropertyName<'_>| matches!(name, tsr_ast::PropertyName::PrivateIdentifier(p) if p.text == text);
        for member in members {
            match member {
                tsr_ast::ClassElement::MethodDeclaration(n) if is_name(n.name) => return true,
                tsr_ast::ClassElement::PropertyDeclaration(n) if is_name(n.name) => return false,
                tsr_ast::ClassElement::GetAccessorDeclaration(n) if is_name(n.name) => {
                    return false;
                }
                tsr_ast::ClassElement::SetAccessorDeclaration(n) if is_name(n.name) => {
                    return false;
                }
                _ => {}
            }
        }
        false
    }

    /// The private-name arm of `checkPropertyAccessExpressionOrQualifiedName`
    /// (`checker.go:11268`) after the lexical lookup, with
    /// `checkPrivateIdentifierPropertyAccess` (`checker.go:11494`):
    ///
    /// - an any-like receiver (`any`, the error type, `unknown` under
    ///   `strictNullChecks`, which `checkNonNullExpression` turns into the
    ///   error type) is silent when the name is lexically declared, and TS18016
    ///   when the access is outside every class body;
    /// - otherwise, when the lexical class's own member is not a property of
    ///   the receiver, a private-named property of that spelling **on the
    ///   receiver's type** is TS18013 — or TS18014 when its class lexically
    ///   encloses the lexical one, which [`Checker::check_private_name_shadowing`]
    ///   reports;
    /// - and with no such property the access falls to `reportNonexistentProperty`:
    ///   TS2339 on a receiver whose property list is certified complete
    ///   (`never`, an any-like type, or `declared_members_are_complete`).
    ///
    /// Declines: a union or intersection receiver (`getPropertiesOfType`
    /// over constituents), and an error-typed receiver inside a class with no
    /// lexical declaration (this port's error type may be a gap).
    /// `docs/parity/notes/property.md` §10.
    fn check_private_identifier_access(&mut self, node: NodeId) {
        let Some(Node::PropertyAccessExpression(access)) = self.node_map.get(node) else { return };
        let Some(tsr_ast::MemberName::PrivateIdentifier(name)) = access.name else { return };
        let Some(at) = name.node_id else { return };
        let Some(receiver) = access.expression else { return };
        let text = name.text;
        let lexical = self.lookup_private_declaring_class_for_diagnostic(at, text);
        let declared_receiver = self.check_expression(receiver);
        // A receiver this port could not type is a gap; `unknown` becoming
        // the error type below is `checkNonNullExpression`'s own answer.
        let is_error = self.is_error(declared_receiver);
        let receiver_type = self.check_non_null_type(declared_receiver);
        let flags = self.type_of(receiver_type).flags;
        let any_like = is_error
            || flags.intersects(TypeFlags::ANY)
            || self.strict_null_checks && flags.intersects(TypeFlags::UNKNOWN);
        if any_like {
            if lexical.is_some() {
                return;
            }
            if self.containing_class_excluding_class_decorators(at).is_none() {
                self.report_at_name(
                    at,
                    &messages::PRIVATE_IDENTIFIERS_ARE_NOT_ALLOWED_OUTSIDE_CLASS_BODIES,
                    Vec::new(),
                );
                return;
            }
        }
        if is_error {
            return;
        }
        if flags.intersects(TypeFlags::UNION | TypeFlags::INTERSECTION) {
            return;
        }
        let receiver_is_this =
            receiver.node_id().is_some_and(|id| self.nodes.kind(id) == SyntaxKind::ThisKeyword);
        let found = if any_like { None } else { self.get_property_of_type(receiver_type, text) };
        let declaration =
            found.and_then(|property| self.binder.symbols().get(property).value_declaration);
        let declared_in = declaration.and_then(|declaration| self.containing_class_of(declaration));
        // getPrivateIdentifierPropertyOfType: the lexical class's own member.
        if lexical.is_some() && declared_in == lexical {
            return;
        }
        // Upstream files each class's `#x` under its own mangled name, so an
        // instance of a subclass that redeclares `#x` still carries the
        // lexical class's member; this port's text-keyed table answers the
        // subclass's (`privateNamesConstructorChain-1`). An instance receiver
        // whose class inherits from the lexical class has the lexical member.
        if let Some(lexical) = lexical
            && let crate::types::TypeData::Named { members: Some(owner), .. } =
                self.type_of(receiver_type).data
            && self.symbol_inherits_from_class(owner, lexical)
        {
            return;
        }
        // checkPrivateIdentifierPropertyAccess: a private-named property of
        // this spelling on the receiver's type.
        if let Some(declaration) = declaration
            && self.declaration_names_a_private(declaration)
        {
            let Some(type_class) = declared_in else { return };
            if let Some(lexical) = lexical
                && (lexical == type_class
                    || self.nodes.ancestors(lexical).any(|ancestor| ancestor == type_class))
            {
                // TS18014, reported by `check_private_name_shadowing`.
                return;
            }
            let class_name = self.class_name_text(type_class).unwrap_or_default();
            self.report_at_name(
                at,
                &messages::PROPERTY_0_IS_NOT_ACCESSIBLE_OUTSIDE_CLASS_1_BECAUSE_IT_HAS_A_PRIVATE_IDENTIFIER,
                vec![text.to_string(), class_name],
            );
            return;
        }
        if found.is_some() {
            return;
        }
        // `this` inside a decorator: this port types it from the decorated
        // class rather than the enclosing function (a `check_this_expression`
        // gap), so a miss there is not certified.
        if receiver_is_this
            && self.nodes.ancestors(at).any(|a| self.nodes.kind(a) == SyntaxKind::Decorator)
        {
            return;
        }
        let certified = any_like
            || flags.intersects(TypeFlags::NEVER | TypeFlags::UNKNOWN)
            || self.private_names_are_complete(receiver_type);
        if !certified {
            return;
        }
        let printed = self.type_to_string(receiver_type);
        self.report_at_name(
            at,
            &messages::PROPERTY_0_DOES_NOT_EXIST_ON_TYPE_1,
            vec![text.to_string(), printed],
        );
    }

    /// `lookupSymbolForPrivateIdentifierDeclaration` (`checker.go`): the
    /// first class, from `getContainingClassExcludingClassDecorators` outward,
    /// that declares `text`. Unlike `lexical_private_declaring_class` (the type
    /// road's walk, `crate::members`), a name in a class's own decorator is not
    /// scoped by that class.
    fn lookup_private_declaring_class_for_diagnostic(
        &self,
        name: NodeId,
        text: &str,
    ) -> Option<NodeId> {
        let mut class = self.containing_class_excluding_class_decorators(name);
        while let Some(current) = class {
            if self.class_declares_private_name(current, text) {
                return Some(current);
            }
            class = self.containing_class_of(current);
        }
        None
    }

    /// Is every **private-named** property of `receiver` visible to
    /// `get_property_of_type`? Narrower than `declared_members_are_complete`
    /// because the question is narrower: an index signature never answers a
    /// `#name` (`checkPropertyAccessExpressionOrQualifiedName` skips index
    /// infos for private identifiers), a computed name never is one, and a
    /// static `#name` is never inherited (`addInheritedMembers` skips
    /// `isStaticPrivateIdentifierProperty`), so a class's `typeof` side is
    /// its own declarations. The instance side inherits base `#names`, so
    /// bases are followed; a base this port cannot follow declines.
    fn private_names_are_complete(&mut self, receiver: crate::types::TypeId) -> bool {
        match self.type_of(receiver).data {
            crate::types::TypeData::Anonymous { symbol, .. } => {
                self.binder.symbols().get(symbol).flags.intersects(SymbolFlags::CLASS)
                    && self.binder.symbols().get(symbol).declarations.iter().all(|&d| {
                        matches!(
                            self.nodes.kind(d),
                            SyntaxKind::ClassDeclaration
                                | SyntaxKind::ClassExpression
                                | SyntaxKind::InterfaceDeclaration
                                | SyntaxKind::ModuleDeclaration
                        )
                    })
            }
            crate::types::TypeData::Named { members: Some(owner), .. } => {
                let mut visiting = Vec::new();
                self.private_names_of_symbol_are_complete(owner, &mut visiting)
            }
            _ => false,
        }
    }

    fn private_names_of_symbol_are_complete(
        &mut self,
        owner: SymbolId,
        visiting: &mut Vec<SymbolId>,
    ) -> bool {
        if visiting.contains(&owner) || visiting.len() > 32 {
            return false;
        }
        visiting.push(owner);
        let plain = self.binder.symbols().get(owner).declarations.iter().all(|&d| {
            matches!(
                self.nodes.kind(d),
                SyntaxKind::ClassDeclaration
                    | SyntaxKind::ClassExpression
                    | SyntaxKind::InterfaceDeclaration
                    | SyntaxKind::TypeLiteral
            )
        });
        if !plain {
            return false;
        }
        let Some(bases) = self.base_symbols_of_ex(owner, false) else { return false };
        bases.into_iter().all(|base| self.private_names_of_symbol_are_complete(base, visiting))
    }

    /// Does `owner`'s base chain (through `base_symbols_of_ex`) reach the
    /// class declared by `class`? A base this port cannot follow answers
    /// `true`, the silent direction.
    fn symbol_inherits_from_class(&mut self, owner: SymbolId, class: NodeId) -> bool {
        let mut pending = vec![owner];
        let mut seen = Vec::new();
        while let Some(current) = pending.pop() {
            if seen.contains(&current) || seen.len() > 32 {
                continue;
            }
            seen.push(current);
            if current != owner && self.binder.symbols().get(current).declarations.contains(&class)
            {
                return true;
            }
            let Some(bases) = self.base_symbols_of_ex(current, false) else { return true };
            pending.extend(bases);
        }
        false
    }

    fn report_at_name(
        &mut self,
        at: NodeId,
        message: &'static tsr_diagnostics::Message,
        arguments: Vec<String>,
    ) {
        let Some(file) = self.source_file_of_for_diagnostics(at) else { return };
        let span = self.error_span(at);
        self.report(file, Diagnostic::with_args(message, span, arguments));
    }

    /// `getContainingClassExcludingClassDecorators` (`utilities.go:994`): a
    /// name written in a class's own decorator starts the search above that
    /// class.
    fn containing_class_excluding_class_decorators(&self, node: NodeId) -> Option<NodeId> {
        let decorator = self.nodes.ancestors(node).find(|&ancestor| {
            self.nodes.kind(ancestor) == SyntaxKind::Decorator
                && self.nodes.parent(ancestor).is_some_and(|parent| {
                    matches!(
                        self.nodes.kind(parent),
                        SyntaxKind::ClassDeclaration | SyntaxKind::ClassExpression
                    )
                })
        });
        let start = match decorator {
            Some(decorator) => self.nodes.parent(decorator)?,
            None => node,
        };
        self.containing_class_of(start)
    }

    /// Does any enclosing class declare this `#name`?
    /// TS18014 — `The property '{0}' cannot be accessed on type '{1}' within
    /// this class because it is shadowed by another private identifier with the
    /// same spelling.`
    ///
    /// `checker.go:11518`. Decided from the receiver's **written annotation**
    /// rather than its type (§580's shape): `x: Base` names a class that
    /// declares `#x`, the **nearest** enclosing class declares `#x` too, and the
    /// two are different declarations.
    ///
    /// The nearest one, not any ancestor — `class Derived` is declared inside
    /// `Base`'s constructor in the corpus's fixtures, so `Base` *is* an
    /// ancestor of the access and an `ancestors().any()` identity test declines
    /// every case. §627 bracketed the decline to this line. §628.
    pub(crate) fn check_private_name_shadowing(&mut self, node: NodeId) {
        if self.file_has_parse_errors {
            return;
        }
        let Some(Node::PropertyAccessExpression(access)) = self.node_map.get(node) else { return };
        let (Some(receiver), Some(tsr_ast::MemberName::PrivateIdentifier(name))) =
            (access.expression, access.name)
        else {
            return;
        };
        let text = name.text;
        let Some(at) = name.node_id else { return };
        let Some(nearest) = self.nearest_class_declaring_private_name(node, text) else { return };
        let Some(receiver_id) = receiver.node_id() else { return };
        let Some(other) = self.annotated_class_of(receiver_id) else { return };
        if other == nearest {
            return;
        }
        // `FindAncestor(lexicalClass, n == typeClass)`: shadowing only when the
        // receiver's class lexically encloses the lexical one; a sibling or a
        // subclass declared elsewhere is TS18013
        // (`privateNamesAndStaticFields`'s `B.#foo` inside `A`).
        if !self.nodes.ancestors(nearest).any(|ancestor| ancestor == other) {
            return;
        }
        if !self.class_declares_private_name(other, text) {
            return;
        }
        let Some(class_name) = self.class_name_text(other) else { return };
        let Some(file) = self.source_file_of_for_diagnostics(at) else { return };
        let span = self.nodes.span(at);
        self.report(
            file,
            Diagnostic::with_args(
                &messages::THE_PROPERTY_0_CANNOT_BE_ACCESSED_ON_TYPE_1_WITHIN_THIS_CLASS_BECAUSE_IT_IS_SHADOWED_BY_ANOTHER_PRIVATE_IDENTIFIER_WITH_THE_SAME_SPELLING,
                span,
                [text.to_string(), class_name],
            ),
        );
    }

    /// TS2540 — `Cannot assign to '{0}' because it is a read-only property.`
    ///
    /// `isAssignmentToReadonlyEntity` (`checker.go:11376`) for a **private
    /// accessor pair with a getter and no setter**, which is decidable from the
    /// declaring class's own members. §580's shape; §628 made the same move for
    /// TS18014. §666.
    pub(crate) fn check_private_accessor_is_writable(&mut self, node: NodeId) {
        if self.file_has_parse_errors {
            return;
        }
        let Some(Node::BinaryExpression(binary)) = self.node_map.get(node) else { return };
        if binary.operator_token.is_none_or(|t| !t.kind.is_assignment_operator()) {
            return;
        }
        let Some(left) = binary.left.and_then(|left| left.node_id()) else { return };
        let Some(Node::PropertyAccessExpression(access)) = self.node_map.get(left) else { return };
        let Some(tsr_ast::MemberName::PrivateIdentifier(name)) = access.name else { return };
        let Some(class) = self.nearest_class_declaring_private_name(left, name.text) else {
            return;
        };
        let members: &[tsr_ast::ClassElement<'_>] = match self.node_map.get(class) {
            Some(Node::ClassDeclaration(n)) => n.members,
            Some(Node::ClassExpression(n)) => n.members,
            _ => return,
        };
        let matches_name = |member_name: tsr_ast::PropertyName<'_>| matches!(member_name, tsr_ast::PropertyName::PrivateIdentifier(p) if p.text == name.text);
        let mut reads = false;
        let mut writes = false;
        for member in members {
            match member {
                tsr_ast::ClassElement::GetAccessorDeclaration(n) if matches_name(n.name) => {
                    reads = true;
                }
                tsr_ast::ClassElement::SetAccessorDeclaration(n) if matches_name(n.name) => {
                    writes = true;
                }
                _ => {}
            }
        }
        if !reads || writes {
            return;
        }
        let Some(at) = name.node_id else { return };
        let Some(file) = self.source_file_of_for_diagnostics(at) else { return };
        let span = self.nodes.span(at);
        self.report(
            file,
            Diagnostic::with_args(
                &messages::CANNOT_ASSIGN_TO_0_BECAUSE_IT_IS_A_READ_ONLY_PROPERTY,
                span,
                [name.text.to_string()],
            ),
        );
    }

    /// `isAssignmentToReadonlyEntity`'s namespace-import arm
    /// (`checker.go:27279`): for a receiver that is (through parentheses) an
    /// identifier resolving to an alias, `Some(declaration is a
    /// NamespaceImport)`; `None` for any other receiver.
    fn receiver_alias_is_namespace_import(
        &mut self,
        receiver: tsr_ast::Expression<'_>,
    ) -> Option<bool> {
        let mut receiver = receiver.node_id()?;
        while let Some(Node::ParenthesizedExpression(parenthesized)) = self.node_map.get(receiver) {
            receiver = parenthesized.expression?.node_id()?;
        }
        let text = self.identifier_text(receiver)?.to_string();
        let symbol = self.binder.resolve_name(
            self.nodes,
            self.node_map,
            receiver,
            &text,
            tsr_binder::SymbolFlags::VALUE | tsr_binder::SymbolFlags::ALIAS,
        )?;
        let entry = self.binder.symbols().get(symbol);
        if !entry.flags.intersects(tsr_binder::SymbolFlags::ALIAS) {
            return None;
        }
        Some(
            entry
                .declarations
                .iter()
                .any(|&declaration| self.nodes.kind(declaration) == SyntaxKind::NamespaceImport),
        )
    }

    /// The nearest enclosing class that declares `text`. §628.
    fn nearest_class_declaring_private_name(&self, node: NodeId, text: &str) -> Option<NodeId> {
        self.nodes
            .ancestors(node)
            .find(|&ancestor| self.class_declares_private_name(ancestor, text))
    }

    /// Does this class declaration declare the private name `text`? §628.
    fn class_declares_private_name(&self, class: NodeId, text: &str) -> bool {
        let members: &[tsr_ast::ClassElement<'_>] = match self.node_map.get(class) {
            Some(Node::ClassDeclaration(n)) => n.members,
            Some(Node::ClassExpression(n)) => n.members,
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
    }

    /// The class declaration a receiver's written annotation names. §628.
    fn annotated_class_of(&mut self, receiver: NodeId) -> Option<NodeId> {
        let text = self.identifier_text(receiver)?.to_string();
        let symbol = self.binder.resolve_name(
            self.nodes,
            self.node_map,
            receiver,
            &text,
            tsr_binder::SymbolFlags::VALUE,
        )?;
        let declarations =
            self.binder.symbols().get(self.binder.merged_symbol(symbol)).declarations.clone();
        for declaration in declarations {
            // **A class name used as a value names its own declaration.**
            // `A.#x` reaches the statics of `A`; the receiver is the class
            // itself, not something annotated with it. §630.
            if matches!(
                self.node_map.get(declaration),
                Some(Node::ClassDeclaration(_) | Node::ClassExpression(_))
            ) {
                return Some(declaration);
            }
            let annotation = match self.node_map.get(declaration) {
                Some(Node::ParameterDeclaration(p)) => p.r#type,
                Some(Node::VariableDeclaration(v)) => v.r#type,
                _ => None,
            };
            let Some(reference) = annotation.and_then(|a| a.node_id()) else { continue };
            let Some(Node::TypeReferenceNode(reference)) = self.node_map.get(reference) else {
                continue;
            };
            let Some(name) = reference.type_name.and_then(|n| n.node_id()) else { continue };
            let Some(name_text) = self.identifier_text(name).map(str::to_string) else { continue };
            let Some(class_symbol) = self.binder.resolve_name(
                self.nodes,
                self.node_map,
                name,
                &name_text,
                tsr_binder::SymbolFlags::TYPE,
            ) else {
                continue;
            };
            return self
                .binder
                .symbols()
                .get(self.binder.merged_symbol(class_symbol))
                .declarations
                .first()
                .copied();
        }
        None
    }

    /// The written name of a class declaration. §628.
    fn class_name_text(&self, class: NodeId) -> Option<String> {
        match self.node_map.get(class)? {
            Node::ClassDeclaration(n) => n.name.and_then(|name| name.node_id),
            Node::ClassExpression(n) => n.name.and_then(|name| name.node_id),
            _ => None,
        }
        .and_then(|id| self.identifier_text(id))
        .map(str::to_string)
    }

    /// Is this property access inaccessible, and with which message?
    ///
    /// `checkPropertyAccessibilityAtLocation` (`checker.go`) for a dotted
    /// access, reached from `checkPropertyAccessExpressionOrQualifiedName`
    /// once `getPropertyOfType(apparentType)` found the property. Answers the
    /// message, its arguments and the error node (the name).
    ///
    /// The private and protected arms are upstream's, over this port's class
    /// declarations: `isNodeWithinClass` for `private` (TS2341), the first
    /// enclosing class derived from the declaring one for `protected` (TS2445),
    /// a typed `this` parameter standing in for an enclosing class
    /// (`getEnclosingClassFromThisParameter`), and `hasBaseType` from the
    /// receiver's apparent class to that enclosing class for an instance member
    /// (TS2446). A link this port cannot follow — a base it cannot resolve,
    /// a receiver whose class it cannot name — declines.
    ///
    /// Split out because `assignreport` skips the assignment relation for an
    /// access reported here (`checker-notes-diag2.md` §70).
    pub(crate) fn inaccessible_property(
        &mut self,
        node: NodeId,
    ) -> Option<(&'static tsr_diagnostics::Message, Vec<String>, NodeId)> {
        let Some(Node::PropertyAccessExpression(access)) = self.node_map.get(node) else {
            return None;
        };
        let (Some(receiver), Some(member)) = (access.expression, access.name) else { return None };
        // `#x` is TS18013, a different code with its own row.
        let tsr_ast::MemberName::Identifier(name) = member else { return None };
        let name_id = name.node_id?;
        let is_super = receiver
            .node_id()
            .is_some_and(|receiver| self.nodes.kind(receiver) == SyntaxKind::SuperKeyword);
        let receiver_type = self.check_expression(receiver);
        if self.is_error(receiver_type)
            || self.type_of(receiver_type).flags.intersects(TypeFlags::ANY_OR_UNKNOWN)
        {
            return None;
        }
        let containing = self.apparent_type(receiver_type);
        let property = self.get_property_of_type(containing, name.text)?;
        let writing =
            self.assignment_target_kind(node) != crate::expressions::AssignmentTargetKind::None;
        let (message, arguments) = self.property_accessibility_error(
            node, is_super, writing, containing, property, name.text,
        )?;
        Some((message, arguments, name_id))
    }

    /// `checkVariableLikeDeclaration`'s binding-element arm (`checker.go`):
    /// an object binding element names a property of its parent's type,
    /// and `checkPropertyAccessibility` runs on it as a read, reported at
    /// `getBindingElementPropertyName` (the property name, else the name).
    pub(crate) fn check_binding_element_accessibility(&mut self, node: NodeId, ambient: bool) {
        if ambient || self.file_has_parse_errors || self.in_js_file(node) {
            return;
        }
        let Some(Node::BindingElement(element)) = self.node_map.get(node) else { return };
        let Some(pattern) = self.nodes.parent(node) else { return };
        if self.nodes.kind(pattern) != SyntaxKind::ObjectBindingPattern {
            return;
        }
        let Some(holder) = self.nodes.parent(pattern) else { return };
        let (name_text, at) = match (element.property_name, element.name) {
            (Some(property_name), _) => {
                let Some(at) = property_name.node_id() else { return };
                let text = match property_name {
                    tsr_ast::PropertyName::Identifier(identifier) => identifier.text.to_string(),
                    tsr_ast::PropertyName::StringLiteral(literal) => literal.text.to_string(),
                    tsr_ast::PropertyName::NumericLiteral(literal) => literal.text.to_string(),
                    // getLiteralTypeFromPropertyName: a computed name's
                    // checked type, when usable as a property name.
                    tsr_ast::PropertyName::ComputedPropertyName(computed) => {
                        let Some(expression) = computed.expression else { return };
                        let key = self.check_expression(expression);
                        match &self.type_of(key).data {
                            crate::types::TypeData::StringLiteral(text)
                            | crate::types::TypeData::NumberLiteral(text) => text.clone(),
                            _ => return,
                        }
                    }
                    _ => return,
                };
                (text, at)
            }
            (None, Some(tsr_ast::BindingName::Identifier(identifier))) => {
                let Some(at) = identifier.node_id else { return };
                (identifier.text.to_string(), at)
            }
            _ => return,
        };
        let parent_type = self.get_type_for_binding_element_parent(holder);
        if self.is_error(parent_type)
            || self.type_of(parent_type).flags.intersects(TypeFlags::ANY_OR_UNKNOWN)
        {
            return;
        }
        let Some(property) = self.get_property_of_type(parent_type, &name_text) else { return };
        let Some((message, arguments)) = self.property_accessibility_error(
            node,
            false,
            false,
            parent_type,
            property,
            &name_text,
        ) else {
            return;
        };
        let Some(file) = self.source_file_of_for_diagnostics(at) else { return };
        let span = self.error_span(at);
        self.report(file, Diagnostic::with_args(message, span, arguments));
    }

    /// `checkPropertyAccessibilityAtLocation` (`checker.go`) after the
    /// property is found: the message and arguments an inaccessible `prop`
    /// reports at `location`, or `None` when it is accessible or a link
    /// cannot be followed.
    pub(crate) fn property_accessibility_error(
        &mut self,
        location: NodeId,
        is_super: bool,
        writing: bool,
        containing: crate::types::TypeId,
        property: SymbolId,
        name: &str,
    ) -> Option<(&'static tsr_diagnostics::Message, Vec<String>)> {
        let node = location;
        let declaration = self.modifier_declaration_of(property, writing)?;
        let is_private = self.member_declaration_has(declaration, SyntaxKind::PrivateKeyword);
        if !is_private && !self.member_declaration_has(declaration, SyntaxKind::ProtectedKeyword) {
            return None;
        }
        // getClassLikeDeclarationOfSymbol(getParentOfSymbol(prop)): a parameter
        // property's parent symbol is its constructor's class.
        let declaring = self.containing_class_of(declaration)?;
        let declaring_name = self.class_declared_type_text(declaring)?;
        let enclosing = self.enclosing_classes_of(node);
        if is_private {
            if enclosing.contains(&declaring) {
                return None;
            }
            return Some((
                &messages::PROPERTY_0_IS_PRIVATE_AND_ONLY_ACCESSIBLE_WITHIN_CLASS_1,
                vec![name.to_string(), declaring_name],
            ));
        }
        if is_super {
            return None;
        }
        let is_static = self.member_declaration_has(declaration, SyntaxKind::StaticKeyword);
        let mut enclosing_class =
            enclosing.iter().copied().find(|&class| self.class_derives_from(class, declaring));
        if enclosing_class.is_none() {
            match self.enclosing_class_from_this_parameter(node) {
                ThisParameterClass::None => {}
                ThisParameterClass::Class(class) => {
                    if self.class_derives_from(class, declaring) {
                        enclosing_class = Some(class);
                    }
                }
                ThisParameterClass::Unsupported => return None,
            }
            if is_static || enclosing_class.is_none() {
                return Some((
                    &messages::PROPERTY_0_IS_PROTECTED_AND_ONLY_ACCESSIBLE_WITHIN_CLASS_1_AND_ITS_SUBCLASSES,
                    vec![name.to_string(), declaring_name],
                ));
            }
        }
        if is_static {
            return None;
        }
        let enclosing_class = enclosing_class?;
        // hasBaseType(containingType, enclosingClass), the type parameter
        // receiver read through its constraint (apparent type above).
        let crate::types::TypeData::Named { members: Some(owner), .. } =
            self.type_of(containing).data
        else {
            return None;
        };
        let receiver_class = self.class_declaration_of_symbol(owner)?;
        if self.class_derives_from(receiver_class, enclosing_class) {
            return None;
        }
        let enclosing_name = self.class_declared_type_text(enclosing_class)?;
        let containing_text = self.type_to_string(containing);
        Some((
            &messages::PROPERTY_0_IS_PROTECTED_AND_ONLY_ACCESSIBLE_THROUGH_AN_INSTANCE_OF_CLASS_1_THIS_IS_AN_INSTANCE_OF_CLASS_2,
            vec![name.to_string(), enclosing_name, containing_text],
        ))
    }

    /// `getDeclarationModifierFlagsFromSymbol(prop) & NonPublicAccessibilityModifier`.
    pub(crate) fn property_is_non_public(&self, property: SymbolId) -> bool {
        self.modifier_declaration_of(property, false).is_some_and(|declaration| {
            self.member_declaration_has(declaration, SyntaxKind::PrivateKeyword)
                || self.member_declaration_has(declaration, SyntaxKind::ProtectedKeyword)
        })
    }

    /// `getDeclarationModifierFlagsFromSymbolEx`'s declaration choice: a write
    /// reads the setter, any other access the getter, else the value
    /// declaration. A symbol without one is public.
    fn modifier_declaration_of(&self, property: SymbolId, writing: bool) -> Option<NodeId> {
        let entry = self.binder.symbols().get(property);
        entry.value_declaration?;
        let accessor = |kind: SyntaxKind| {
            entry
                .declarations
                .iter()
                .copied()
                .find(|&declaration| self.nodes.kind(declaration) == kind)
        };
        if writing && let Some(setter) = accessor(SyntaxKind::SetAccessor) {
            return Some(setter);
        }
        accessor(SyntaxKind::GetAccessor).or(entry.value_declaration)
    }

    /// The class or class expression declaring `symbol`, when it has exactly
    /// one class-like declaration (interfaces merged with it add none).
    fn class_declaration_of_symbol(&self, symbol: SymbolId) -> Option<NodeId> {
        let mut classes =
            self.binder.symbols().get(symbol).declarations.iter().copied().filter(|&declaration| {
                matches!(
                    self.nodes.kind(declaration),
                    SyntaxKind::ClassDeclaration | SyntaxKind::ClassExpression
                )
            });
        let class = classes.next()?;
        classes.next().is_none().then_some(class)
    }

    /// `TypeToString(getDeclaredTypeOfSymbol(class))` — `D<T>` for a generic.
    fn class_declared_type_text(&mut self, class: NodeId) -> Option<String> {
        let symbol = self.binder.symbol_of(class)?;
        let declared = self.get_declared_type_of_symbol(self.binder.merged_symbol(symbol));
        if self.is_error(declared) {
            return None;
        }
        Some(self.type_to_string(declared))
    }

    /// `getEnclosingClassFromThisParameter` (`checker.go`): the class a typed
    /// `this` parameter of the nearest non-arrow function names, read through
    /// a type parameter's constraint.
    fn enclosing_class_from_this_parameter(&mut self, node: NodeId) -> ThisParameterClass {
        let Some(container) = self.get_this_container(node, false) else {
            return ThisParameterClass::None;
        };
        let parameters: &[&tsr_ast::ParameterDeclaration<'_>] = match self.node_map.get(container) {
            Some(Node::FunctionDeclaration(n)) => n.parameters,
            Some(Node::FunctionExpression(n)) => n.parameters,
            Some(Node::MethodDeclaration(n)) => n.parameters,
            _ => return ThisParameterClass::None,
        };
        let annotation = parameters
            .first()
            .filter(|parameter| {
                matches!(parameter.name, Some(tsr_ast::BindingName::Identifier(name)) if name.text == "this")
            })
            .and_then(|parameter| parameter.r#type);
        let this_type = match annotation {
            Some(annotation) => self.get_type_from_type_node(annotation),
            // 3. "The 'this' parameter of a contextual type"
            // (`getContextualThisParameterType`). Its contextual-signature arm
            // is ported; the object-literal and `obj.m = function` arms (under
            // `noImplicitThis` or in JS) are not, and decline.
            None => match self.contextual_this_parameter_type(container) {
                Some(this_type) => this_type,
                None if (self.no_implicit_this || self.in_js_file(container))
                    && self.contextual_this_needs_unported_arm(container) =>
                {
                    return ThisParameterClass::Unsupported;
                }
                None => return ThisParameterClass::None,
            },
        };
        // A type parameter is read through its constraint; an unconstrained
        // or primitive one is no class (`ObjectFlagsClassOrInterface` unset).
        let this_type = self.apparent_type(this_type);
        if !self.type_of(this_type).flags.intersects(TypeFlags::OBJECT) {
            return ThisParameterClass::None;
        }
        let crate::types::TypeData::Named { members: Some(owner), .. } =
            self.type_of(this_type).data
        else {
            return ThisParameterClass::Unsupported;
        };
        match self.class_declaration_of_symbol(owner) {
            Some(class) => ThisParameterClass::Class(class),
            None => ThisParameterClass::None,
        }
    }

    /// Would `getContextualThisParameterType` reach its object-literal arm
    /// (`getContainingObjectLiteral`) or its assignment arm (`obj.m =
    /// function …`) for this function?
    fn contextual_this_needs_unported_arm(&self, function: NodeId) -> bool {
        let Some(mut parent) = self.nodes.parent(function) else { return false };
        while self.nodes.kind(parent) == SyntaxKind::ParenthesizedExpression {
            let Some(next) = self.nodes.parent(parent) else { return false };
            parent = next;
        }
        matches!(
            self.nodes.kind(parent),
            SyntaxKind::ObjectLiteralExpression
                | SyntaxKind::PropertyAssignment
                | SyntaxKind::BinaryExpression
        ) || self.nodes.kind(function) == SyntaxKind::MethodDeclaration
            && self
                .nodes
                .parent(function)
                .is_some_and(|p| self.nodes.kind(p) == SyntaxKind::ObjectLiteralExpression)
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

    /// Does this class member carry the given modifier? `ast.HasSyntacticModifier`,
    /// which sees the modifiers upstream reparses from JSDoc tags in a JS file
    /// ([`Checker::has_effective_modifier`]).
    fn member_declaration_has(&self, declaration: NodeId, keyword: SyntaxKind) -> bool {
        match self.nodes.kind(declaration) {
            SyntaxKind::PropertyDeclaration
            | SyntaxKind::MethodDeclaration
            | SyntaxKind::GetAccessor
            | SyntaxKind::SetAccessor
            | SyntaxKind::Parameter
            | SyntaxKind::BinaryExpression => self.has_effective_modifier(declaration, keyword),
            _ => false,
        }
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
}

/// `getEnclosingClassFromThisParameter`'s answer.
enum ThisParameterClass {
    /// No typed `this` parameter, or it names no class.
    None,
    /// The class the `this` parameter names.
    Class(NodeId),
    /// A `this` type this port cannot read as a class or interface.
    Unsupported,
}

/// TS2729 — `checkPropertyNotUsedBeforeDeclaration` (`checker.go:11709`) and
/// the declared-before-use machinery it asks (`isBlockScopedNameDeclaredBeforeUse`,
/// `checker.go:1922`). `docs/parity/notes/property.md` §5.
impl Checker<'_, '_> {
    /// TS2729 — `Property '{0}' is used before its initialization.`
    ///
    /// `checkPropertyNotUsedBeforeDeclaration` (`checker.go:11709`), reached
    /// from `checkPropertyAccessExpressionOrQualifiedName` once
    /// `getPropertyOfType(apparentType)` found `prop`. The conjuncts are
    /// upstream's, asked of the resolved property's `valueDeclaration`, in
    /// upstream's order; the cheap syntactic ones run before the lookup.
    ///
    /// Declines where this port cannot answer upstream's question:
    /// - a union or intersection receiver — upstream's synthetic property
    ///   carries a `valueDeclaration` only when every constituent agrees,
    ///   and this port's lookup answers a constituent's symbol;
    /// - `isPropertyInitializedInStaticBlocks` with a static block in range
    ///   (it asks the flow type at the block's end);
    /// - a class-like `valueDeclaration` (the computed-name/decorator arm);
    /// - an ancestor class whose first base type does not resolve.
    pub(crate) fn check_property_not_used_before_declaration(&mut self, node: NodeId) {
        if self.file_has_parse_errors || self.file_is_ambient {
            return;
        }
        let Some(Node::PropertyAccessExpression(access)) = self.node_map.get(node) else { return };
        let (Some(receiver), Some(member)) = (access.expression, access.name) else { return };
        let (text, right) = match member {
            tsr_ast::MemberName::Identifier(name) => (name.text, name.node_id),
            tsr_ast::MemberName::PrivateIdentifier(name) => (name.text, name.node_id),
        };
        let Some(right) = right else { return };
        if !self.is_in_property_initializer_or_class_static_block(node, false) {
            return;
        }
        // `!(IsAccessExpression(node) && IsAccessExpression(node.Expression()))`.
        let Some(receiver_id) = receiver.node_id() else { return };
        if matches!(
            self.nodes.kind(receiver_id),
            SyntaxKind::PropertyAccessExpression | SyntaxKind::ElementAccessExpression
        ) {
            return;
        }
        let Some(property) = self.property_of_access_for_use_before_init(node, receiver, text)
        else {
            return;
        };
        let record = self.binder.symbols().get(property);
        let Some(value_declaration) = record.value_declaration else { return };
        let parent_symbol = record.parent;
        if self.is_optional_property_declaration(value_declaration) {
            return;
        }
        match self.is_block_scoped_name_declared_before_use(value_declaration, right) {
            Some(false) => {}
            Some(true) | None => return,
        }
        if self.nodes.kind(value_declaration) == SyntaxKind::MethodDeclaration
            && self.node_has_syntactic_modifier(value_declaration, SyntaxKind::StaticKeyword)
        {
            return;
        }
        // `GetUseDefineForClassFields()`, which is what `standard_class_fields`
        // computes (the flag, else `target >= ES2022`).
        if !self.standard_class_fields {
            match self.is_property_declared_in_ancestor_class(parent_symbol, text) {
                Some(false) => {}
                Some(true) | None => return,
            }
        }
        let Some(file) = self.source_file_of_for_diagnostics(right) else { return };
        let span = self.nodes.span(right);
        self.report(
            file,
            Diagnostic::with_args(
                &messages::PROPERTY_0_IS_USED_BEFORE_ITS_INITIALIZATION,
                span,
                [text.to_string()],
            ),
        );
    }

    /// The `prop` `checkPropertyAccessExpressionOrQualifiedName` resolves:
    /// `getPropertyOfType(getApparentType(leftType), right.Text())`, or for a
    /// `#name` the lexically scoped class's own member
    /// (`getPrivateIdentifierPropertyOfType`). `None` when the receiver is
    /// any-like or an error, or the property is not found.
    fn property_of_access_for_use_before_init(
        &mut self,
        node: NodeId,
        receiver: tsr_ast::Expression<'_>,
        text: &str,
    ) -> Option<SymbolId> {
        let lexical = if text.starts_with('#') {
            Some(self.lexical_private_declaring_class(node, text)?)
        } else {
            None
        };
        let receiver_type = self.check_expression(receiver);
        if self.is_error(receiver_type)
            || self.type_of(receiver_type).flags.intersects(TypeFlags::ANY_OR_UNKNOWN)
        {
            return None;
        }
        let apparent = self.apparent_type(receiver_type);
        if self.type_of(apparent).flags.intersects(TypeFlags::UNION.union(TypeFlags::INTERSECTION))
        {
            return None;
        }
        let property = self.get_property_of_type(apparent, text)?;
        if let Some(lexical) = lexical {
            let declaration = self.binder.symbols().get(property).value_declaration?;
            if self.containing_class_of(declaration) != Some(lexical) {
                return None;
            }
        }
        Some(property)
    }

    /// `isOptionalPropertyDeclaration` (`checker.go:11731`).
    fn is_optional_property_declaration(&self, node: NodeId) -> bool {
        let Some(Node::PropertyDeclaration(property)) = self.node_map.get(node) else {
            return false;
        };
        !tsr_ast::has_syntactic_modifier(property.modifiers, SyntaxKind::AccessorKeyword)
            && property.postfix_token.is_some_and(|token| token.kind == SyntaxKind::QuestionToken)
    }

    /// `isPropertyDeclaredInAncestorClass` (`checker.go:11735`):
    /// `getPropertyOfType(getBaseTypes(declaredType(prop.Parent))[0], name)`
    /// has a `valueDeclaration`. `None` when the class has an `extends`
    /// clause whose base type this port cannot resolve.
    fn is_property_declared_in_ancestor_class(
        &mut self,
        parent: Option<SymbolId>,
        name: &str,
    ) -> Option<bool> {
        let Some(parent) = parent else { return Some(false) };
        if !self.binder.symbols().get(parent).flags.intersects(SymbolFlags::CLASS) {
            return Some(false);
        }
        let base = if let Some(base) = self.first_base_type_of_class_symbol(parent) {
            base
        } else {
            if !self.class_symbol_has_extends_clause(parent) {
                return Some(false);
            }
            self.value_base_type_of_class_symbol(parent)?
        };
        let Some(property) = self.get_property_of_type(base, name) else { return Some(false) };
        Some(self.binder.symbols().get(property).value_declaration.is_some())
    }

    /// `resolveBaseTypesOfClass` (`checker.go:19220`) for an `extends` entry
    /// that names a **value** (`declare const F: new () => B; class D extends
    /// F`), which [`Checker::first_base_type_of_class_symbol`] refuses because
    /// it resolves the entry in type meaning: the entry resolved as a value,
    /// then the first construct signature matching the type-argument count,
    /// whose return type is the base.
    fn value_base_type_of_class_symbol(&mut self, class: SymbolId) -> Option<crate::types::TypeId> {
        let declaration = self.binder.symbols().get(class).value_declaration?;
        let clauses = match self.node_map.get(declaration)? {
            Node::ClassDeclaration(node) => node.heritage_clauses,
            Node::ClassExpression(node) => node.heritage_clauses,
            _ => return None,
        };
        let entry = clauses
            .iter()
            .filter(|clause| clause.token.kind == SyntaxKind::ExtendsKeyword)
            .flat_map(|clause| clause.types.iter())
            .next()?;
        let base = self.heritage_entity_symbol(entry.expression?, SymbolFlags::VALUE)?;
        let t = self.instance_base_type_of_heritage_entry(base, entry);
        (!self.is_error(t)).then_some(t)
    }

    /// Does any declaration of this class symbol carry an `extends` clause?
    fn class_symbol_has_extends_clause(&self, class: SymbolId) -> bool {
        self.binder.symbols().get(class).declarations.iter().any(|&declaration| {
            let clauses = match self.node_map.get(declaration) {
                Some(Node::ClassDeclaration(node)) => node.heritage_clauses,
                Some(Node::ClassExpression(node)) => node.heritage_clauses,
                _ => return false,
            };
            clauses.iter().any(|clause| clause.token.kind == SyntaxKind::ExtendsKeyword)
        })
    }

    /// `HasSyntacticModifier(node, keyword)` for any modifier-bearing node.
    fn node_has_syntactic_modifier(&self, node: NodeId, keyword: SyntaxKind) -> bool {
        self.node_map
            .get(node)
            .and_then(crate::check::modifiers_of)
            .is_some_and(|modifiers| tsr_ast::has_syntactic_modifier(modifiers, keyword))
    }

    /// `isInPropertyInitializerOrClassStaticBlock` (`checker.go:13706`).
    pub(crate) fn is_in_property_initializer_or_class_static_block(
        &self,
        node: NodeId,
        ignore_arrow_functions: bool,
    ) -> bool {
        let mut current = Some(node);
        while let Some(id) = current {
            match self.nodes.kind(id) {
                SyntaxKind::PropertyDeclaration | SyntaxKind::ClassStaticBlockDeclaration => {
                    return true;
                }
                SyntaxKind::TypeQuery | SyntaxKind::JsxClosingElement => return false,
                SyntaxKind::ArrowFunction if !ignore_arrow_functions => return false,
                SyntaxKind::Block
                    if self.nodes.parent(id).is_some_and(|parent| {
                        is_function_like_declaration_kind(self.nodes.kind(parent))
                            && self.nodes.kind(parent) != SyntaxKind::ArrowFunction
                    }) =>
                {
                    return false;
                }
                _ => {}
            }
            current = self.nodes.parent(id);
        }
        false
    }

    /// `isBlockScopedNameDeclaredBeforeUse` (`checker.go:1922`), whole.
    /// `None` where an arm needs an answer this port cannot give (see
    /// [`Checker::check_property_not_used_before_declaration`]).
    fn is_block_scoped_name_declared_before_use(
        &mut self,
        declaration: NodeId,
        usage: NodeId,
    ) -> Option<bool> {
        if self.source_file_of_for_diagnostics(declaration)
            != self.source_file_of_for_diagnostics(usage)
        {
            return Some(true);
        }
        let container = self.block_scope_container_of(declaration);
        if self.nodes.flags(usage).contains(tsr_ast::NodeFlags::JSDOC)
            || self.is_in_type_query_for_use(usage)
            || self.is_in_ambient_or_type_node(usage)
        {
            return Some(true);
        }
        let declaration_kind = self.nodes.kind(declaration);
        let declaration_start = self.nodes.span(declaration).start;
        let usage_start = self.nodes.span(usage).start;
        let uninitialized_this_property = declaration_kind == SyntaxKind::PropertyDeclaration
            && self.nodes.parent(usage).is_some_and(|parent| self.is_this_property(parent))
            && matches!(
                self.node_map.get(declaration),
                Some(Node::PropertyDeclaration(property))
                    if property.initializer.is_none()
                        && property.postfix_token.is_none_or(|t| t.kind != SyntaxKind::ExclamationToken)
            );
        if declaration_start <= usage_start && !uninitialized_this_property {
            return match declaration_kind {
                SyntaxKind::BindingElement => {
                    if let Some(error_element) = self
                        .nodes
                        .ancestors(usage)
                        .find(|&a| self.nodes.kind(a) == SyntaxKind::BindingElement)
                    {
                        return Some(
                            Some(error_element) != Some(declaration)
                                && self.nearest_binding_element(error_element)
                                    != self.nearest_binding_element(declaration)
                                || declaration_start < self.nodes.span(error_element).start,
                        );
                    }
                    let variable = self
                        .nodes
                        .ancestors(declaration)
                        .find(|&a| self.nodes.kind(a) == SyntaxKind::VariableDeclaration)?;
                    self.is_block_scoped_name_declared_before_use(variable, usage)
                }
                SyntaxKind::VariableDeclaration => {
                    Some(!self.is_immediately_used_in_initializer_of_block_scoped_variable(
                        declaration,
                        usage,
                        container,
                    ))
                }
                SyntaxKind::ClassDeclaration | SyntaxKind::ClassExpression => None,
                SyntaxKind::PropertyDeclaration => {
                    Some(!self.is_property_immediately_referenced_within_declaration(
                        declaration,
                        usage,
                        false,
                    ))
                }
                _ if self.is_parameter_property_declaration(declaration) => Some(
                    !(self.emit_standard_class_fields()
                        && self.containing_class_of(declaration)
                            == self.containing_class_of(usage)
                        && self.is_used_in_function_or_instance_property_exact(
                            usage,
                            declaration,
                            container,
                        )?),
                ),
                _ => Some(true),
            };
        }
        if let Some(parent) = self.nodes.parent(usage) {
            match self.node_map.get(parent) {
                Some(Node::ExportSpecifier(_)) => return Some(true),
                Some(Node::ExportAssignment(assignment)) if assignment.is_export_equals => {
                    return Some(true);
                }
                _ => {}
            }
        }
        if matches!(self.node_map.get(usage), Some(Node::ExportAssignment(a)) if a.is_export_equals)
        {
            return Some(true);
        }
        if self.is_used_in_function_or_instance_property_exact(usage, declaration, container)? {
            if self.emit_standard_class_fields()
                && self.containing_class_of(declaration).is_some()
                && (declaration_kind == SyntaxKind::PropertyDeclaration
                    || self.is_parameter_property_declaration(declaration))
            {
                return Some(!self.is_property_immediately_referenced_within_declaration(
                    declaration,
                    usage,
                    true,
                ));
            }
            return Some(true);
        }
        Some(false)
    }

    /// `c.emitStandardClassFields` (`GetEmitStandardClassFields`): the flag
    /// not `false` **and** `target >= ES2022`. `standard_class_fields` is
    /// `GetUseDefineForClassFields`, which differs only when the flag is set
    /// on an older target; that combination is not distinguishable from here
    /// (the checker keeps no target), so this reads the stored flag.
    fn emit_standard_class_fields(&self) -> bool {
        self.standard_class_fields
    }

    /// `ast.FindAncestor(node, ast.IsBindingElement)`, including `node`.
    fn nearest_binding_element(&self, node: NodeId) -> Option<NodeId> {
        std::iter::once(node)
            .chain(self.nodes.ancestors(node))
            .find(|&a| self.nodes.kind(a) == SyntaxKind::BindingElement)
    }

    /// `isThisProperty` (`utilities.go`): a property or element access whose
    /// expression is `this`.
    fn is_this_property(&self, node: NodeId) -> bool {
        let expression = match self.node_map.get(node) {
            Some(Node::PropertyAccessExpression(access)) => access.expression,
            Some(Node::ElementAccessExpression(access)) => access.expression,
            _ => return false,
        };
        expression
            .and_then(|e| e.node_id())
            .is_some_and(|e| self.nodes.kind(e) == SyntaxKind::ThisKeyword)
    }

    /// `IsInTypeQuery` (`utilities.go`): an ancestor `typeof` query, stopping
    /// at an expression-with-type-arguments or a non-entity-name parent.
    fn is_in_type_query_for_use(&self, node: NodeId) -> bool {
        for ancestor in self.nodes.ancestors(node) {
            match self.nodes.kind(ancestor) {
                SyntaxKind::TypeQuery => return true,
                SyntaxKind::Identifier | SyntaxKind::QualifiedName => {}
                _ => return false,
            }
        }
        false
    }

    /// `isInAmbientOrTypeNode` (`checker.go:11238`).
    fn is_in_ambient_or_type_node(&self, node: NodeId) -> bool {
        self.declaration_is_in_an_ambient_context(node)
            || self.nodes.ancestors(node).any(|ancestor| {
                matches!(
                    self.nodes.kind(ancestor),
                    SyntaxKind::InterfaceDeclaration
                        | SyntaxKind::TypeAliasDeclaration
                        | SyntaxKind::JSTypeAliasDeclaration
                        | SyntaxKind::TypeLiteral
                )
            })
    }

    /// `IsParameterPropertyDeclaration(node, node.Parent)`: a parameter of a
    /// constructor carrying a `ParameterPropertyModifier`.
    fn is_parameter_property_declaration(&self, node: NodeId) -> bool {
        let Some(Node::ParameterDeclaration(parameter)) = self.node_map.get(node) else {
            return false;
        };
        self.nodes.parent(node).is_some_and(|p| self.nodes.kind(p) == SyntaxKind::Constructor)
            && [
                SyntaxKind::PublicKeyword,
                SyntaxKind::PrivateKeyword,
                SyntaxKind::ProtectedKeyword,
                SyntaxKind::ReadonlyKeyword,
                SyntaxKind::OverrideKeyword,
            ]
            .into_iter()
            .any(|keyword| tsr_ast::has_syntactic_modifier(parameter.modifiers, keyword))
    }

    /// `ast.GetEnclosingBlockScopeContainer` (`ast/utilities.go:2171`).
    fn block_scope_container_of(&self, node: NodeId) -> Option<NodeId> {
        self.nodes.ancestors(node).find(|&ancestor| match self.nodes.kind(ancestor) {
            SyntaxKind::Block => !self.nodes.parent(ancestor).is_some_and(|parent| {
                let kind = self.nodes.kind(parent);
                is_function_like_kind(kind) || kind == SyntaxKind::ClassStaticBlockDeclaration
            }),
            kind => is_block_scope_kind(kind),
        })
    }

    /// `isUsedInFunctionOrInstanceProperty` (`checker.go:2011`), every arm.
    /// `None` when `isPropertyInitializedInStaticBlocks` would be asked
    /// about a static block in range.
    fn is_used_in_function_or_instance_property_exact(
        &mut self,
        usage: NodeId,
        declaration: NodeId,
        container: Option<NodeId>,
    ) -> Option<bool> {
        let mut current = usage;
        loop {
            if Some(current) == container {
                return Some(false);
            }
            let kind = self.nodes.kind(current);
            if is_function_like_kind(kind) {
                if self.immediately_invoked_call(current).is_none() {
                    return Some(true);
                }
            } else if kind == SyntaxKind::ClassStaticBlockDeclaration {
                if self.nodes.span(declaration).start < self.nodes.span(usage).start {
                    return Some(true);
                }
            } else if let Some(parent) = self.nodes.parent(current) {
                if let Some(Node::PropertyDeclaration(property)) = self.node_map.get(parent)
                    && property.initializer.and_then(|i| i.node_id()) == Some(current)
                {
                    if tsr_ast::has_syntactic_modifier(
                        property.modifiers,
                        SyntaxKind::StaticKeyword,
                    ) {
                        if self.nodes.kind(declaration) == SyntaxKind::MethodDeclaration {
                            return Some(true);
                        }
                        if let Some(Node::PropertyDeclaration(declared)) =
                            self.node_map.get(declaration)
                            && self.containing_class_of(usage)
                                == self.containing_class_of(declaration)
                            && matches!(
                                declared.name,
                                tsr_ast::PropertyName::Identifier(_)
                                    | tsr_ast::PropertyName::PrivateIdentifier(_)
                            )
                        {
                            // isPropertyInitializedInStaticBlocks: blocks
                            // between the class start and this initializer.
                            let class = self.nodes.parent(declaration)?;
                            let low = self.nodes.span(class).start;
                            let high = self.nodes.span(current).start;
                            if self.class_has_static_block_in_range(class, low, high) {
                                return None;
                            }
                        }
                    } else {
                        let is_declaration_instance_property = self.nodes.kind(declaration)
                            == SyntaxKind::PropertyDeclaration
                            && !self.node_has_syntactic_modifier(
                                declaration,
                                SyntaxKind::StaticKeyword,
                            );
                        if !is_declaration_instance_property
                            || self.containing_class_of(usage)
                                != self.containing_class_of(declaration)
                        {
                            return Some(true);
                        }
                    }
                }
                if let Some(Node::Decorator(decorator)) = self.node_map.get(parent)
                    && decorator.expression.and_then(|e| e.node_id()) == Some(current)
                    && let Some(decorated) = self.nodes.parent(parent)
                {
                    let member = match self.nodes.kind(decorated) {
                        SyntaxKind::Parameter => {
                            self.nodes.parent(decorated).and_then(|f| self.nodes.parent(f))
                        }
                        SyntaxKind::MethodDeclaration => self.nodes.parent(decorated),
                        _ => None,
                    };
                    // Found → `FindAncestorTrue`; not found → `FindAncestorQuit`.
                    if let Some(member) = member {
                        return self.is_used_in_function_or_instance_property_exact(
                            member,
                            declaration,
                            container,
                        );
                    }
                }
            }
            let Some(parent) = self.nodes.parent(current) else { return Some(false) };
            current = parent;
        }
    }

    /// Does `class` hold a static block whose start lies in `[low, high]`?
    fn class_has_static_block_in_range(&self, class: NodeId, low: u32, high: u32) -> bool {
        let members: &[tsr_ast::ClassElement<'_>] = match self.node_map.get(class) {
            Some(Node::ClassDeclaration(n)) => n.members,
            Some(Node::ClassExpression(n)) => n.members,
            _ => return false,
        };
        members.iter().any(|member| {
            matches!(member, tsr_ast::ClassElement::ClassStaticBlockDeclaration(_))
                && member.node_id().is_some_and(|id| {
                    let start = self.nodes.span(id).start;
                    low <= start && start <= high
                })
        })
    }

    /// `isImmediatelyUsedInInitializerOfBlockScopedVariable` (`checker.go:2069`).
    fn is_immediately_used_in_initializer_of_block_scoped_variable(
        &self,
        declaration: NodeId,
        usage: NodeId,
        container: Option<NodeId>,
    ) -> bool {
        let Some(grandparent) = self.nodes.parent(declaration).and_then(|p| self.nodes.parent(p))
        else {
            return false;
        };
        let grandparent_kind = self.nodes.kind(grandparent);
        if matches!(
            grandparent_kind,
            SyntaxKind::VariableStatement | SyntaxKind::ForStatement | SyntaxKind::ForOfStatement
        ) && self.same_scope_descendent_of(usage, Some(declaration), container)
        {
            return true;
        }
        let Some(Node::ForInOrOfStatement(statement)) = self.node_map.get(grandparent) else {
            return false;
        };
        self.same_scope_descendent_of(
            usage,
            statement.expression.and_then(|e| e.node_id()),
            container,
        )
    }

    /// `isSameScopeDescendentOf` (`checker.go:2087`).
    fn same_scope_descendent_of(
        &self,
        initial: NodeId,
        parent: Option<NodeId>,
        stop_at: Option<NodeId>,
    ) -> bool {
        let Some(parent) = parent else { return false };
        let mut n = Some(initial);
        while let Some(id) = n {
            if id == parent {
                return true;
            }
            if Some(id) == stop_at
                || is_function_like_kind(self.nodes.kind(id))
                    && (self.immediately_invoked_call(id).is_none()
                        || self.is_async_or_generator_function(id))
            {
                return false;
            }
            n = self.nodes.parent(id);
        }
        false
    }

    /// `GetFunctionFlags(n) & FunctionFlagsAsyncGenerator != 0`: the mask is
    /// `Async | Generator`, so either an `async` modifier or a `*` sets it.
    fn is_async_or_generator_function(&self, node: NodeId) -> bool {
        let (modifiers, asterisk) = match self.node_map.get(node) {
            Some(Node::FunctionExpression(n)) => (n.modifiers, n.asterisk_token.is_some()),
            Some(Node::FunctionDeclaration(n)) => (n.modifiers, n.asterisk_token.is_some()),
            Some(Node::MethodDeclaration(n)) => (n.modifiers, n.asterisk_token.is_some()),
            Some(Node::ArrowFunction(n)) => (n.modifiers, false),
            _ => return false,
        };
        asterisk || tsr_ast::has_syntactic_modifier(modifiers, SyntaxKind::AsyncKeyword)
    }

    /// `isPropertyImmediatelyReferencedWithinDeclaration` (`checker.go:2106`).
    fn is_property_immediately_referenced_within_declaration(
        &self,
        declaration: NodeId,
        usage: NodeId,
        stop_at_any_property_declaration: bool,
    ) -> bool {
        if self.nodes.span(usage).end > self.nodes.span(declaration).end {
            return false;
        }
        let mut node = Some(usage);
        while let Some(id) = node {
            if id == declaration {
                break;
            }
            match self.nodes.kind(id) {
                SyntaxKind::ArrowFunction => return false,
                SyntaxKind::PropertyDeclaration => {
                    let same_class = if self.nodes.kind(declaration)
                        == SyntaxKind::PropertyDeclaration
                    {
                        self.nodes.parent(id) == self.nodes.parent(declaration)
                    } else {
                        self.is_parameter_property_declaration(declaration)
                            && self.nodes.parent(id)
                                == self.nodes.parent(declaration).and_then(|c| self.nodes.parent(c))
                    };
                    return stop_at_any_property_declaration && same_class;
                }
                SyntaxKind::Block
                    if self.nodes.parent(id).is_some_and(|parent| {
                        matches!(
                            self.nodes.kind(parent),
                            SyntaxKind::MethodDeclaration
                                | SyntaxKind::GetAccessor
                                | SyntaxKind::SetAccessor
                        )
                    }) =>
                {
                    return false;
                }
                _ => {}
            }
            node = self.nodes.parent(id);
        }
        true
    }
}

/// `ast.IsFunctionLikeDeclaration`'s kinds.
fn is_function_like_declaration_kind(kind: SyntaxKind) -> bool {
    matches!(
        kind,
        SyntaxKind::FunctionDeclaration
            | SyntaxKind::MethodDeclaration
            | SyntaxKind::Constructor
            | SyntaxKind::GetAccessor
            | SyntaxKind::SetAccessor
            | SyntaxKind::FunctionExpression
            | SyntaxKind::ArrowFunction
    )
}

/// `ast.IsFunctionLike`'s kinds (`IsFunctionLikeKind`): the declarations and
/// the signature kinds, not a class static block.
fn is_function_like_kind(kind: SyntaxKind) -> bool {
    is_function_like_declaration_kind(kind)
        || matches!(
            kind,
            SyntaxKind::MethodSignature
                | SyntaxKind::CallSignature
                | SyntaxKind::JSDocSignature
                | SyntaxKind::ConstructSignature
                | SyntaxKind::IndexSignature
                | SyntaxKind::FunctionType
                | SyntaxKind::ConstructorType
        )
}

/// `ast.IsBlockScope`'s non-`Block` kinds (`ast/utilities.go:2177`).
fn is_block_scope_kind(kind: SyntaxKind) -> bool {
    matches!(
        kind,
        SyntaxKind::SourceFile
            | SyntaxKind::CaseBlock
            | SyntaxKind::CatchClause
            | SyntaxKind::ModuleDeclaration
            | SyntaxKind::ForStatement
            | SyntaxKind::ForInStatement
            | SyntaxKind::ForOfStatement
            | SyntaxKind::Constructor
            | SyntaxKind::MethodDeclaration
            | SyntaxKind::GetAccessor
            | SyntaxKind::SetAccessor
            | SyntaxKind::FunctionDeclaration
            | SyntaxKind::FunctionExpression
            | SyntaxKind::ArrowFunction
            | SyntaxKind::PropertyDeclaration
            | SyntaxKind::ClassStaticBlockDeclaration
    )
}
