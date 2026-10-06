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
        if self.type_of(apparent).flags.intersects(TypeFlags::UNION | TypeFlags::INTERSECTION) {
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
        if !self.is_readonly_symbol(property) && !self.property_signature_is_readonly(property) {
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
    fn property_signature_is_readonly(&self, symbol: SymbolId) -> bool {
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
    fn property_accessibility_error(
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
        let Some(this_parameter) = parameters.first().filter(|parameter| {
            matches!(parameter.name, Some(tsr_ast::BindingName::Identifier(name)) if name.text == "this")
        }) else {
            return ThisParameterClass::None;
        };
        let Some(annotation) = this_parameter.r#type else { return ThisParameterClass::None };
        let this_type = self.get_type_from_type_node(annotation);
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
