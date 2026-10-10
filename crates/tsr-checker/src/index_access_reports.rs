//! The final error arm of `getPropertyTypeForIndexType` (`checker.go:27001`):
//! an index type no property, index signature or valid key kind answers is
//! reported on the access node's index (`getIndexNodeForAccessExpression`).
//!
//! TS2538 (`Type '{0}' cannot be used as an index type.`) is the arm for an
//! index type that is nullable or not assignable to a string, number or
//! symbol kind; such an index never reaches the property or index-signature
//! lookups above it. TS2537 is the arm for a non-literal `string`/`number`
//! key with no applicable index signature. TS7015/TS7053 are the
//! `noImplicitAny` arms for an element access whose non-literal key no index
//! signature takes (`report_implicit_any_element_access`).
//!
//! These reports run at check sites over types `check_expression` and
//! `get_type_from_type_node` already cached; they add no table.

use tsr_ast::{Node, NodeId, SyntaxKind};
use tsr_diagnostics::{Diagnostic, messages};

use crate::checker::Checker;
use crate::flags::TypeFlags;
use crate::relater::{Relation, Ternary};
use crate::types::{TypeData, TypeId};

impl Checker<'_, '_> {
    /// `indexType.flags&TypeFlagsNullable == 0 &&
    /// isTypeAssignableToKind(indexType, StringLike|NumberLike|ESSymbolLike)`
    /// (`checker.go:27081`). `None` when the relation is undecided here.
    fn is_valid_index_access_key_type(&mut self, index_type: TypeId) -> Option<bool> {
        let flags = self.store.get(index_type).flags;
        if flags.intersects(TypeFlags::NULLABLE) {
            return Some(false);
        }
        if flags
            .intersects(TypeFlags::STRING_LIKE | TypeFlags::NUMBER_LIKE | TypeFlags::ES_SYMBOL_LIKE)
            || flags.intersects(TypeFlags::ANY)
        {
            return Some(true);
        }
        // A text-only named type is a shape this port has not built; its
        // kind is not evidence.
        if matches!(self.store.get(index_type).data, TypeData::Named { members: None, .. })
            && !self.type_reference_targets.contains_key(&index_type)
            && !self.tuple_element_lists.contains_key(&index_type)
        {
            return None;
        }
        let mut undecided = false;
        for primitive in [self.intrinsics.number, self.intrinsics.string, self.intrinsics.es_symbol]
        {
            if self.object_against_primitive(index_type, primitive) {
                continue;
            }
            match self.relate_ternary(index_type, primitive, Relation::Assignable) {
                Ternary::Related => return Some(true),
                Ternary::NotRelated => {}
                Ternary::Unknown => undecided = true,
            }
        }
        (!undecided).then_some(false)
    }

    /// Reports TS2538 for each index constituent that is not a valid key
    /// kind. `getIndexedAccessTypeOrUndefined` (`checker.go:26975`) visits a
    /// non-boolean union index per constituent; a generic object or index is
    /// deferred (`shouldDeferIndexedAccessType`) and reports nothing here.
    fn report_invalid_index_types(
        &mut self,
        object_type: TypeId,
        index_type: TypeId,
        index_node: NodeId,
    ) {
        if self.is_error(object_type)
            || self.is_gap(index_type)
            || self.has_instantiable_constituent(object_type)
            || self.has_instantiable_constituent(index_type)
            || self.indexed_access_index_is_generic(index_type)
            || self.mentions_registered_type_parameter(index_type)
        {
            return;
        }
        let constituents = match &self.store.get(index_type).data {
            TypeData::Union { types, .. }
                if !self.store.get(index_type).flags.intersects(TypeFlags::BOOLEAN) =>
            {
                types.clone()
            }
            _ => vec![index_type],
        };
        if constituents
            .iter()
            .any(|&part| self.store.get(part).flags.intersects(TypeFlags::INSTANTIABLE))
        {
            return;
        }
        for part in constituents {
            if self.is_valid_index_access_key_type(part) != Some(false) {
                continue;
            }
            let Some(file) = self.source_file_of_for_diagnostics(index_node) else { return };
            let text = if self.nodes.kind(index_node) == SyntaxKind::BigIntLiteral {
                "bigint".to_string()
            } else {
                self.type_to_string(part)
            };
            let span = self.error_span(index_node);
            self.report(
                file,
                Diagnostic::with_args(
                    &messages::TYPE_0_CANNOT_BE_USED_AS_AN_INDEX_TYPE,
                    span,
                    [text],
                ),
            );
        }
    }

    /// `checkElementAccessExpression` (`checker.go:8146`): the access node is
    /// the element access, its index node the argument expression. An error
    /// object type or a const enum object returns before indexing.
    ///
    /// The const enum exit reports first (`:8157`): TS2476 at the index
    /// unless it is `IsStringLiteralLike` (r6-modules3 §2). It shares the
    /// rule's JS-file gate, so a JS file's access to a TypeScript const enum
    /// does not report it.
    pub(crate) fn check_element_access_index_type(&mut self, node: NodeId) {
        if self.in_js_file(node) {
            return;
        }
        let Some(Node::ElementAccessExpression(access)) = self.node_map.get(node) else { return };
        let (Some(expression), Some(argument)) = (access.expression, access.argument_expression)
        else {
            return;
        };
        let Some(argument_id) = argument.node_id() else { return };
        let object_type = self.check_expression(expression);
        let index_type = self.check_expression(argument);
        if self.is_const_enum_object_type(object_type) {
            if !matches!(
                argument,
                tsr_ast::Expression::StringLiteral(_)
                    | tsr_ast::Expression::NoSubstitutionTemplateLiteral(_)
            ) && let Some(file) = self.source_file_of_for_diagnostics(argument_id)
            {
                let span = self.error_span(argument_id);
                self.report(
                    file,
                    Diagnostic::new(
                        &messages::A_CONST_ENUM_MEMBER_CAN_ONLY_BE_ACCESSED_USING_A_STRING_LITERAL,
                        span,
                    ),
                );
            }
            return;
        }
        self.report_invalid_index_types(object_type, index_type, argument_id);
        self.report_implicit_any_element_access(node, object_type, index_type, argument_id);
    }

    /// `getPropertyTypeForIndexType`'s access-expression arm
    /// (`checker.go:27129-27184`) for a key that names no property: no
    /// applicable index info and no string index fallback (`:27085`) on the
    /// reduced apparent object type is TS7015 at the index when the object
    /// has a number index, else TS7053 at the access, under `noImplicitAny`.
    ///
    /// Scope: every visited key constituent is a NON-literal valid key
    /// (`string`, `number`, a pattern, a non-unique `symbol`, a tagged
    /// intersection), so `hasPropName` is false and the static-member and
    /// spelling-suggestion arms cannot apply. A literal key names a property
    /// and its miss is `check_nonexistent_property`'s; a union mixing both is
    /// declined whole. Only the first missing constituent reports
    /// (`AccessFlagsSuppressNoImplicitAnyError`, `:26985`).
    ///
    /// The access must itself have failed: `check_expression` of the element
    /// access is this port's `getIndexedAccessTypeOrUndefined` answer, so a
    /// resolved type (the for-in numeric substitution, a tuple, a mapped
    /// index) is never reported over. Declined: a nullable, generic, JS- or
    /// object-literal receiver (the last answers a property union, `:27135`),
    /// `typeof globalThis`, a `get`/`set` member (TS7052's suggestion,
    /// `getSuggestionForNonexistentIndexSignature`, is not ported), and an
    /// index-info table this port cannot complete. No cache or table:
    /// queries already cached by `check_expression` and
    /// `get_index_infos_of_type`. `docs/parity/notes/r4-index.md` §3.
    fn report_implicit_any_element_access(
        &mut self,
        node: NodeId,
        object_type: TypeId,
        index_type: TypeId,
        argument: NodeId,
    ) {
        if !self.no_implicit_any {
            return;
        }
        let Some(Node::ElementAccessExpression(access)) = self.node_map.get(node) else { return };
        // `NodeFlagsOptionalChain` on any link: the chain's own road.
        if access.question_dot_token.is_some()
            || self.nodes.flags(node).contains(tsr_ast::NodeFlags::OPTIONAL_CHAIN)
        {
            return;
        }
        let resolved = self.check_expression(tsr_ast::Expression::ElementAccessExpression(access));
        if !self.is_error(resolved) && resolved != self.intrinsics.unresolved {
            return;
        }
        let object_flags = self.store.get(object_type).flags;
        if self.is_error(object_type)
            || self.is_gap(index_type)
            || object_flags.intersects(
                TypeFlags::ANY | TypeFlags::UNKNOWN | TypeFlags::NEVER | TypeFlags::NULLABLE,
            )
            || matches!(&self.store.get(object_type).data, TypeData::Union { types, .. }
                if types.iter().any(|&part| self.store.get(part).flags.intersects(TypeFlags::NULLABLE)))
            || self.has_instantiable_constituent(object_type)
            || self.has_instantiable_constituent(index_type)
            || self.indexed_access_index_is_generic(index_type)
            || self.mentions_registered_type_parameter(object_type)
            || self.mentions_registered_type_parameter(index_type)
            || self.is_js_literal_type(object_type)
            // `:27135` answers a `string`/`number` key with the property union
            // and a literal key with TS2339; an `any` key falls through to the
            // TS7053 arm like any other receiver.
            || index_type != self.intrinsics.any && self.is_object_literal_type(object_type)
            || Some(object_type) == self.global_this_type
            || !self.index_infos_are_declared(object_type)
        {
            return;
        }
        let constituents = match &self.store.get(index_type).data {
            TypeData::Union { types, .. }
                if !self.store.get(index_type).flags.intersects(TypeFlags::BOOLEAN) =>
            {
                types.clone()
            }
            _ => vec![index_type],
        };
        for &part in &constituents {
            let flags = self.store.get(part).flags;
            if flags.intersects(
                TypeFlags::STRING_LITERAL
                    | TypeFlags::NUMBER_LITERAL
                    | TypeFlags::UNIQUE_ES_SYMBOL
                    | TypeFlags::ENUM_LITERAL
                    | TypeFlags::BOOLEAN_LITERAL
                    | TypeFlags::NEVER,
            ) || self.is_valid_index_access_key_type(part) != Some(true)
                || self.is_gap(part)
                || flags.intersects(TypeFlags::ANY) && part != self.intrinsics.any
            {
                return;
            }
        }
        let apparent = self.apparent_type(object_type);
        if self.is_gap(apparent)
            || self.store.get(apparent).flags.intersects(TypeFlags::ANY | TypeFlags::NEVER)
            || matches!(self.store.get(apparent).data, TypeData::Named { members: None, .. })
                && !self.type_reference_targets.contains_key(&apparent)
                && !self.tuple_element_lists.contains_key(&apparent)
        {
            return;
        }
        let Some(infos) = self.get_index_infos_of_type(apparent) else { return };
        if infos.iter().any(|info| self.is_gap(info.key) || self.is_gap(info.value)) {
            return;
        }
        let has_string = infos.iter().any(|info| info.key == self.intrinsics.string);
        // `isApplicableIndexType(any, key)` holds for every key kind (`any` is
        // assignable to each), so an `any` key misses only on a receiver with
        // no index info at all.
        if has_string
            || index_type == self.intrinsics.any && !infos.is_empty()
            || constituents
                .into_iter()
                .all(|part| self.get_applicable_index_info(apparent, part).is_some())
        {
            return;
        }
        // `getIndexTypeOfType(objectType, numberType)` reads the ORIGINAL
        // object type; its index infos are the apparent type's for every
        // non-primitive receiver admitted above.
        let Some(original_infos) = self.get_index_infos_of_type(object_type) else { return };
        let Some(file) = self.source_file_of_for_diagnostics(node) else { return };
        if original_infos.iter().any(|info| info.key == self.intrinsics.number) {
            let span = self.error_span(argument);
            self.report(
                file,
                Diagnostic::new(
                    &messages::ELEMENT_IMPLICITLY_HAS_AN_ANY_TYPE_BECAUSE_INDEX_EXPRESSION_IS_NOT_OF_TYPE_NUMBER,
                    span,
                ),
            );
            return;
        }
        // getSuggestionForNonexistentIndexSignature asks a `get`/`set` member's
        // single call signature; declined rather than guessed.
        if ["get", "set"].iter().any(|name| self.get_property_of_type(apparent, name).is_some()) {
            return;
        }
        let span = self.error_span(node);
        let index_text = self.type_to_string(index_type);
        let object_text = self.type_to_string(object_type);
        self.report(
            file,
            Diagnostic::with_args(
                &messages::ELEMENT_IMPLICITLY_HAS_AN_ANY_TYPE_BECAUSE_EXPRESSION_OF_TYPE_0_CAN_T_BE_USED_TO_INDEX_TYPE_1,
                span,
                [index_text, object_text],
            ),
        );
    }

    /// TS7017 for a dotted name `typeof globalThis` does not have: the
    /// `leftType.symbol == c.globalThisSymbol` arm of
    /// `checkPropertyAccessExpressionOrQualifiedName` (`checker.go:11337`).
    ///
    /// Native reaches it when `getPropertyOfType(typeof globalThis, name)`
    /// misses and no index info applies (`typeof globalThis` has none). Its
    /// members are the non-block-scoped global exports
    /// (`resolveAnonymousTypeMembers`), read through `symbolIsValue`; the
    /// same lookup this port's member road performs for that receiver
    /// (`get_type_of_property_of_type`'s `global_this_type` arm). A
    /// block-scoped global is the arm's TS2339 half, reported by
    /// `check_nonexistent_property`; every other miss is TS7017 at the name
    /// under `noImplicitAny`, for reads and writes alike.
    /// `docs/parity/notes/r5-index4.md` §1.
    pub(crate) fn check_global_this_property_access(&mut self, node: NodeId) {
        if !self.no_implicit_any {
            return;
        }
        let Some(Node::PropertyAccessExpression(access)) = self.node_map.get(node) else { return };
        let (Some(receiver), Some(tsr_ast::MemberName::Identifier(name))) =
            (access.expression, access.name)
        else {
            return;
        };
        // `right.Text() != ""` guards the sibling arm; a recovered empty name
        // has no member to miss.
        if name.text.is_empty() {
            return;
        }
        let Some(name_id) = name.node_id else { return };
        if self.identifier_in_non_emitting_heritage_clause(name_id) {
            return;
        }
        let receiver_type = self.check_expression(receiver);
        // The checker files `globalThisSymbol` (a value module) and
        // `undefinedSymbol` (a property) in `globals` itself (`checker.go:964`,
        // `addUndefinedToGlobalsOrErrorOnRedeclaration`), so both names are
        // members; this binder's globals table carries neither.
        if Some(receiver_type) != self.global_this_type
            || matches!(name.text, "globalThis" | "undefined")
        {
            return;
        }
        if let Some(symbol) = self.binder.global(name.text) {
            let flags = self.binder.symbols().get(symbol).flags;
            if flags.intersects(
                tsr_binder::SymbolFlags::BLOCK_SCOPED_VARIABLE
                    | tsr_binder::SymbolFlags::CLASS
                    | tsr_binder::SymbolFlags::ENUM,
            ) || self.symbol_is_value(symbol)
            {
                return;
            }
        }
        let Some(file) = self.source_file_of_for_diagnostics(name_id) else { return };
        let span = self.error_span(name_id);
        let printed = self.type_to_string(receiver_type);
        self.report(
            file,
            Diagnostic::with_args(
                &messages::ELEMENT_IMPLICITLY_HAS_AN_ANY_TYPE_BECAUSE_TYPE_0_HAS_NO_INDEX_SIGNATURE,
                span,
                [printed],
            ),
        );
    }

    /// `getTypeFromIndexedAccessTypeNode` (`checker.go:24164`): the access
    /// node is the type node, its index node the index type node.
    pub(crate) fn check_indexed_access_type_index_type(&mut self, node: NodeId) {
        if self.in_js_file(node) {
            return;
        }
        let Some(Node::IndexedAccessTypeNode(access)) = self.node_map.get(node) else { return };
        let (Some(object_node), Some(index_node)) = (access.object_type, access.index_type) else {
            return;
        };
        let Some(index_id) = index_node.node_id() else { return };
        let object_type = self.get_type_from_type_node(object_node);
        let index_type = self.get_type_from_type_node(index_node);
        self.report_invalid_index_types(object_type, index_type, index_id);
        // getConditionalFlowTypeOfType (checker.go) intersects an object type
        // written in a conditional's true branch with the extends type when
        // it is the check type (getImpliedConstraint); this port's type-node
        // road has no such substitution, so that object is not certified.
        if let Some(object_id) = object_node.node_id()
            && self.type_has_implied_constraint(object_id, object_type)
        {
            return;
        }
        self.report_indexed_access_type_misses(object_type, index_type, index_id);
    }

    /// Is `node` (of type `ty`) inside the true branch of an enclosing
    /// conditional type node whose check type is `ty`? The walk is the
    /// node's ancestor chain, run only for a checked indexed access type.
    fn type_has_implied_constraint(&mut self, node: NodeId, ty: TypeId) -> bool {
        let mut child = node;
        while let Some(parent) = self.nodes.parent(child) {
            if let Some(Node::ConditionalTypeNode(conditional)) = self.node_map.get(parent)
                && conditional.true_type.and_then(|true_type| true_type.node_id()) == Some(child)
                && let Some(check) = conditional.check_type
                && self.get_type_from_type_node(check) == ty
            {
                return true;
            }
            child = parent;
        }
        false
    }

    /// `getIndexedAccessTypeOrUndefined` (`checker.go:26975`) into
    /// `getPropertyTypeForIndexType`'s final arm (`checker.go:27001`) for an
    /// indexed access type node: no access expression, so a string or number
    /// literal key (each constituent of a non-boolean union key) that names
    /// no property and no applicable index signature of the reduced apparent
    /// object type reports TS2339 at the index node. A generic object or
    /// index is deferred (`shouldDeferIndexedAccessType`) and reports
    /// nothing; a tuple's numeric key has its own arm. `typeof globalThis`
    /// lists only its non-block-scoped globals (`resolveAnonymousTypeMembers`).
    /// No cache: one certified lookup per literal key of a checked node.
    fn report_indexed_access_type_misses(
        &mut self,
        object_type: TypeId,
        index_type: TypeId,
        index_node: NodeId,
    ) {
        if self.is_type_any(object_type)
            || self.is_gap(index_type)
            || self.has_instantiable_constituent(object_type)
            || self.has_instantiable_constituent(index_type)
            || self.indexed_access_index_is_generic(index_type)
            || self.mentions_registered_type_parameter(object_type)
            || self.mentions_registered_type_parameter(index_type)
            || self.tuple_element_lists.contains_key(&object_type)
        {
            return;
        }
        let constituents = match &self.store.get(index_type).data {
            TypeData::Union { types, .. }
                if !self.store.get(index_type).flags.intersects(TypeFlags::BOOLEAN) =>
            {
                types.clone()
            }
            _ => vec![index_type],
        };
        for part in constituents {
            let Some(name) = literal_key_name(&self.store.get(part).data) else { continue };
            let printed_type = if Some(object_type) == self.global_this_type {
                // `globals["globalThis"]` is the `globalThis` module symbol
                // itself (`initializeChecker`), which this binder does not
                // declare.
                let present = name == "globalThis"
                    || self.binder.global(&name).is_some_and(|symbol| {
                        !self.binder.symbols().get(symbol).flags.intersects(
                            tsr_binder::SymbolFlags::BLOCK_SCOPED_VARIABLE
                                | tsr_binder::SymbolFlags::CLASS
                                | tsr_binder::SymbolFlags::ENUM,
                        )
                    });
                if present {
                    continue;
                }
                object_type
            } else {
                let Some(apparent) =
                    self.destructured_property_is_absent(None, object_type, &name, false)
                else {
                    continue;
                };
                apparent
            };
            let Some(file) = self.source_file_of_for_diagnostics(index_node) else { return };
            let span = self.error_span(index_node);
            let printed = self.type_to_string(printed_type);
            self.report(
                file,
                Diagnostic::with_args(
                    &messages::PROPERTY_0_DOES_NOT_EXIST_ON_TYPE_1,
                    span,
                    [name, printed],
                ),
            );
        }
    }

    /// TS2537 for a non-literal `string`/`number` key (`checker.go:27216`):
    /// no applicable index signature and no string index fallback
    /// (`checker.go:27085`). The access node is a computed property name, so
    /// the report lands on its expression. A key with a default is
    /// `AccessFlagsAllowMissing`, which answers `undefined` for an object
    /// literal; it is declined here rather than classified.
    fn report_missing_index_signature(
        &mut self,
        object_type: TypeId,
        key_type: TypeId,
        has_default: bool,
        index_node: NodeId,
    ) {
        // A key that is `any` (including the errorType of an invalid computed
        // name) is a valid key kind with no access expression to report
        // through, so it reaches the final arm as TS2538 when no index
        // signature applies.
        let any_key = self.is_type_any(key_type);
        let key_flags = self.store.get(key_type).flags;
        let symbol_key = key_flags.intersects(TypeFlags::ES_SYMBOL_LIKE);
        if has_default
            || self.is_type_any(object_type)
            || object_type == self.intrinsics.any
            || self.has_instantiable_constituent(object_type)
            || self.mentions_registered_type_parameter(object_type)
            || !(any_key
                || symbol_key
                || key_type == self.intrinsics.string
                || key_type == self.intrinsics.number)
        {
            return;
        }
        let object_type = self.apparent_type(object_type);
        if self.store.get(object_type).flags.intersects(TypeFlags::ANY | TypeFlags::NEVER)
            || matches!(self.store.get(object_type).data, TypeData::Named { members: None, .. })
                && !self.type_reference_targets.contains_key(&object_type)
                && !self.tuple_element_lists.contains_key(&object_type)
                // A binding pattern's implied object type publishes its
                // complete member image here (`binding_patterns.rs`).
                && !self.object_literal_members.contains_key(&object_type)
        {
            return;
        }
        // A unique symbol key names a property; only a complete table without
        // symbol-keyed members proves it absent.
        if key_flags.intersects(TypeFlags::UNIQUE_ES_SYMBOL)
            && self
                .declared_property_table(object_type)
                .is_none_or(|table| table.iter().any(|(name, _)| name.starts_with('[')))
        {
            return;
        }
        let Some(infos) = self.get_index_infos_of_type(object_type) else { return };
        let string_info = infos.iter().find(|info| info.key == self.intrinsics.string).copied();
        let info = self.get_applicable_index_info(object_type, key_type).or(string_info);
        let Some(file) = self.source_file_of_for_diagnostics(index_node) else { return };
        let span = self.error_span(index_node);
        let diagnostic = match info {
            // `indexInfo.keyType == stringType && !isTypeAssignableToKind(
            // indexType, String|Number)` (`checker.go:27095`).
            Some(info) if info.key == self.intrinsics.string && symbol_key => {
                Diagnostic::with_args(
                    &messages::TYPE_0_CANNOT_BE_USED_AS_AN_INDEX_TYPE,
                    span,
                    [self.type_to_string(key_type)],
                )
            }
            Some(_) => return,
            None if any_key => Diagnostic::with_args(
                &messages::TYPE_0_CANNOT_BE_USED_AS_AN_INDEX_TYPE,
                span,
                ["any".to_string()],
            ),
            None if symbol_key => Diagnostic::with_args(
                &messages::TYPE_0_CANNOT_BE_USED_AS_AN_INDEX_TYPE,
                span,
                [self.type_to_string(key_type)],
            ),
            None => Diagnostic::with_args(
                &messages::TYPE_0_HAS_NO_MATCHING_INDEX_SIGNATURE_FOR_TYPE_1,
                span,
                [self.type_to_string(object_type), self.type_to_string(key_type)],
            ),
        };
        self.report(file, diagnostic);
    }

    /// `getBindingElementTypeFromParentType`'s object arm (`checker.go:17707`):
    /// the element's property name (or, for `{ p }`, its name) indexes the
    /// parent with `getLiteralTypeFromPropertyName(name)` and the name as
    /// access node. A computed name reports a key no index signature takes;
    /// a literal key (written or computed) that names no property reports
    /// TS2339 at the index node.
    pub(crate) fn check_binding_element_index_access(&mut self, node: NodeId) {
        if self.in_js_file(node) {
            return;
        }
        let Some(Node::BindingElement(element)) = self.node_map.get(node) else { return };
        if element.dot_dot_dot_token.is_some() {
            return;
        }
        let has_default = element.initializer.is_some();
        let Some(pattern_id) = self.nodes.parent(node) else { return };
        if self.nodes.kind(pattern_id) != SyntaxKind::ObjectBindingPattern {
            return;
        }
        let Some(holder) = self.nodes.parent(pattern_id) else { return };
        let (name, index_node) = match element.property_name {
            Some(tsr_ast::PropertyName::ComputedPropertyName(computed)) => {
                let Some(expression) = computed.expression else { return };
                let Some(expression_id) = expression.node_id() else { return };
                let key_type = self.check_expression(expression);
                let parent_type = self.get_type_for_binding_element_parent(holder);
                if self.is_type_any(parent_type) {
                    return;
                }
                let Some(name) = literal_key_name(&self.store.get(key_type).data) else {
                    let parent_type = self.destructuring_parent_adjusted(node, holder, parent_type);
                    self.report_missing_index_signature(
                        parent_type,
                        key_type,
                        has_default,
                        expression_id,
                    );
                    return;
                };
                (name, expression_id)
            }
            Some(tsr_ast::PropertyName::Identifier(name)) => {
                let Some(id) = name.node_id else { return };
                (name.text.to_string(), id)
            }
            Some(tsr_ast::PropertyName::StringLiteral(name)) => {
                let Some(id) = name.node_id else { return };
                (name.text.to_string(), id)
            }
            Some(tsr_ast::PropertyName::NumericLiteral(name)) => {
                let Some(id) = name.node_id else { return };
                (crate::printing::normalise_number(name.text), id)
            }
            Some(_) => return,
            None => {
                let Some(tsr_ast::BindingName::Identifier(name)) = element.name else { return };
                let Some(id) = name.node_id else { return };
                (name.text.to_string(), id)
            }
        };
        // The root declaration the outermost pattern's type was read from.
        let mut root = holder;
        while self.nodes.kind(root) == SyntaxKind::BindingElement {
            let Some(holder) =
                self.nodes.parent(root).and_then(|pattern| self.nodes.parent(pattern))
            else {
                return;
            };
            root = holder;
        }
        let annotation = self.type_annotation_of(root);
        let initializer = self.initializer_of(root);
        // getTypeForVariableLikeDeclaration (checker.go:16678): a catch
        // variable annotated with anything but any/unknown is errorType.
        if let Some(annotation) = annotation
            && self
                .nodes
                .parent(root)
                .is_some_and(|parent| self.nodes.kind(parent) == SyntaxKind::CatchClause)
        {
            let declared = self.get_type_from_type_node(annotation);
            if !self.store.get(declared).flags.intersects(TypeFlags::ANY | TypeFlags::UNKNOWN) {
                return;
            }
        }
        // An unannotated parameter with no initializer is contextually typed
        // or implied by its pattern (getTypeFromBindingPattern), roads this
        // port's parent type does not take.
        if annotation.is_none()
            && initializer.is_none()
            && self.nodes.kind(root) == SyntaxKind::Parameter
        {
            return;
        }
        // A defaulted name under an object literal initializer: the literal
        // is checked with the pattern as contextual type, which adds the
        // name as an optional property (checkObjectLiteral's pattern arm,
        // padObjectLiteralType for a parameter), so nothing is missing.
        let initializer = initializer
            .and_then(|initializer| initializer.node_id())
            .map(|initializer| self.skip_outer_expressions(initializer));
        if annotation.is_none()
            && has_default
            && initializer.is_some_and(|initializer| {
                self.nodes.kind(initializer) == SyntaxKind::ObjectLiteralExpression
            })
        {
            return;
        }
        let parent_type = self.get_type_for_binding_element_parent(holder);
        if self.is_type_any(parent_type) {
            return;
        }
        // The strict-mode parent adjustments (`checker.go:17713`-`:17718`).
        let parent_type = if self.strict_null_checks {
            self.destructuring_parent_adjusted(node, holder, parent_type)
        } else {
            parent_type
        };
        // An annotation is declared; an initializer is a flow type.
        let source = if annotation.is_some() {
            None
        } else {
            initializer.map(|initializer| self.destructuring_source_receiver(initializer))
        };
        self.report_destructured_property_miss(source, parent_type, &name, has_default, index_node);
    }

    /// The receiver whose flow type certifies a destructuring source: an
    /// `await` operand stands for its awaited value, so a call under it is
    /// declined as a call receiver is.
    fn destructuring_source_receiver(&self, mut node: NodeId) -> NodeId {
        for _ in 0..64 {
            let next = match self.node_map.get(node) {
                Some(Node::AwaitExpression(wrapper)) => wrapper.expression,
                _ => return node,
            };
            let Some(next) = next.and_then(|expression| expression.node_id()) else {
                return node;
            };
            node = self.skip_outer_expressions(next);
        }
        node
    }

    /// `checkObjectLiteralDestructuringPropertyAssignment` (`checker.go:12613`)
    /// for an object literal target: each property name indexes the target's
    /// source type with the name as access node. A computed name reports a
    /// key no index signature takes; a literal key that names no property
    /// reports TS2339 at the index node.
    pub(crate) fn check_object_assignment_index_access(&mut self, node: NodeId) {
        if self.in_js_file(node) {
            return;
        }
        self.check_object_assignment_accessibility(node);
        let Some(Node::ObjectLiteralExpression(literal)) = self.node_map.get(node) else { return };
        let mut computed_keys = Vec::new();
        let mut literal_keys = Vec::new();
        for property in literal.properties {
            let Some(id) = property.node_id() else { continue };
            let (name, has_default) = match self.node_map.get(id) {
                // hasDefaultValue: `{ k: x = 1 }`.
                Some(Node::PropertyAssignment(assignment)) => (
                    assignment.name,
                    assignment.initializer.is_some_and(|value| {
                        matches!(value, tsr_ast::Expression::BinaryExpression(binary)
                            if binary.operator_token.is_some_and(|t| t.kind == SyntaxKind::EqualsToken))
                    }),
                ),
                Some(Node::ShorthandPropertyAssignment(shorthand)) => {
                    (shorthand.name, shorthand.object_assignment_initializer.is_some())
                }
                _ => continue,
            };
            match name {
                tsr_ast::PropertyName::ComputedPropertyName(computed) => {
                    let Some(expression) = computed.expression else { continue };
                    let Some(expression_id) = expression.node_id() else { continue };
                    computed_keys.push((expression, expression_id, has_default));
                }
                tsr_ast::PropertyName::Identifier(name) => {
                    let Some(id) = name.node_id else { continue };
                    literal_keys.push((name.text.to_string(), id, has_default));
                }
                tsr_ast::PropertyName::StringLiteral(name) => {
                    let Some(id) = name.node_id else { continue };
                    literal_keys.push((name.text.to_string(), id, has_default));
                }
                tsr_ast::PropertyName::NumericLiteral(name) => {
                    let Some(id) = name.node_id else { continue };
                    literal_keys.push((
                        crate::printing::normalise_number(name.text),
                        id,
                        has_default,
                    ));
                }
                _ => {}
            }
        }
        if computed_keys.is_empty() && literal_keys.is_empty() {
            return;
        }
        let Some(source) = self.destructuring_assignment_source(node) else { return };
        for (expression, expression_id, has_default) in computed_keys {
            let key_type = self.check_expression(expression);
            if let Some(name) = literal_key_name(&self.store.get(key_type).data) {
                literal_keys.push((name, expression_id, has_default));
                continue;
            }
            self.report_missing_index_signature(source, key_type, has_default, expression_id);
        }
        if literal_keys.is_empty() {
            return;
        }
        // The right operand the outermost target's source was read from.
        let mut target = node;
        let source_expression = loop {
            let Some(parent) = self.nodes.parent(target) else { return };
            match self.node_map.get(parent) {
                Some(Node::ArrayLiteralExpression(_)) => target = parent,
                Some(Node::BinaryExpression(binary)) => {
                    let Some(right) = binary.right.and_then(|right| right.node_id()) else {
                        return;
                    };
                    break self.destructuring_source_receiver(self.skip_outer_expressions(right));
                }
                _ => return,
            }
        };
        for (name, index_node, has_default) in literal_keys {
            self.report_destructured_property_miss(
                Some(source_expression),
                source,
                &name,
                has_default,
                index_node,
            );
        }
    }

    /// `getPropertyTypeForIndexType`'s final arm (`checker.go:27001`) for a
    /// literal key with no access expression: `Property '{0}' does not exist
    /// on type '{1}'` at the index node, printing the reduced apparent type.
    fn report_destructured_property_miss(
        &mut self,
        source: Option<NodeId>,
        parent_type: TypeId,
        name: &str,
        has_default: bool,
        index_node: NodeId,
    ) {
        let Some(object_type) =
            self.destructured_property_is_absent(source, parent_type, name, has_default)
        else {
            return;
        };
        let Some(file) = self.source_file_of_for_diagnostics(index_node) else { return };
        let span = self.error_span(index_node);
        let printed = self.type_to_string(object_type);
        self.report(
            file,
            Diagnostic::with_args(
                &messages::PROPERTY_0_DOES_NOT_EXIST_ON_TYPE_1,
                span,
                [name.to_string(), printed],
            ),
        );
    }

    /// `checkObjectLiteralDestructuringPropertyAssignment`'s accessibility
    /// arm (`checker.go:12608`): each `name: target` or shorthand `name` of an
    /// object literal assignment target whose name is usable as a property
    /// name and names a property of the source type is checked as a **write**
    /// by `checkPropertyAccessibility`, reported at the property's name.
    fn check_object_assignment_accessibility(&mut self, node: NodeId) {
        let Some(Node::ObjectLiteralExpression(literal)) = self.node_map.get(node) else { return };
        let properties: Vec<_> = literal
            .properties
            .iter()
            .filter_map(|property| {
                let id = property.node_id()?;
                let name = match self.node_map.get(id)? {
                    Node::PropertyAssignment(assignment) => assignment.name,
                    Node::ShorthandPropertyAssignment(shorthand) => shorthand.name,
                    _ => return None,
                };
                Some((id, name))
            })
            .collect();
        if properties.is_empty() {
            return;
        }
        let Some(source) = self.object_assignment_target_source(node) else { return };
        if source == self.intrinsics.any
            || self.type_of(source).flags.intersects(TypeFlags::ANY_OR_UNKNOWN)
        {
            return;
        }
        for (property, name) in properties {
            // getLiteralTypeFromPropertyName + isTypeUsableAsPropertyName.
            let text = match name {
                tsr_ast::PropertyName::Identifier(identifier) => identifier.text.to_string(),
                tsr_ast::PropertyName::StringLiteral(literal) => literal.text.to_string(),
                tsr_ast::PropertyName::NumericLiteral(literal) => literal.text.to_string(),
                tsr_ast::PropertyName::ComputedPropertyName(computed) => {
                    let Some(expression) = computed.expression else { continue };
                    let key = self.check_expression(expression);
                    match &self.type_of(key).data {
                        TypeData::StringLiteral(text) | TypeData::NumberLiteral(text) => {
                            text.clone()
                        }
                        _ => continue,
                    }
                }
                _ => continue,
            };
            let Some(prop) = self.get_property_of_type(source, &text) else { continue };
            let Some((message, arguments)) =
                self.property_accessibility_error(property, false, true, source, prop, &text)
            else {
                continue;
            };
            let Some(at) = name.node_id() else { continue };
            let Some(file) = self.source_file_of_for_diagnostics(at) else { continue };
            let span = self.error_span(at);
            self.report(file, Diagnostic::with_args(message, span, arguments));
        }
    }

    /// The source type an object literal assignment target destructures:
    /// [`Checker::destructuring_assignment_source`] for the top level and
    /// array elements, and for a nested `{ a: { … } }` target the outer
    /// source's `a` property (`checkObjectLiteralDestructuringPropertyAssignment`
    /// recursing through `getIndexedAccessType`). A nested target with a
    /// default (`{ a: { … } = d }`) declines: its type is the union with the
    /// default's.
    fn object_assignment_target_source(&mut self, node: NodeId) -> Option<TypeId> {
        let parent = self.nodes.parent(node)?;
        if let Some(Node::PropertyAssignment(assignment)) = self.node_map.get(parent) {
            if assignment.initializer.and_then(|i| i.node_id()) != Some(node) {
                return None;
            }
            let outer = self.nodes.parent(parent)?;
            if self.nodes.kind(outer) != SyntaxKind::ObjectLiteralExpression {
                return None;
            }
            let outer_source = self.object_assignment_target_source(outer)?;
            if outer_source == self.intrinsics.any {
                return Some(outer_source);
            }
            let text = match assignment.name {
                tsr_ast::PropertyName::Identifier(identifier) => identifier.text,
                tsr_ast::PropertyName::StringLiteral(literal) => literal.text,
                _ => return None,
            };
            let t = self.get_type_of_property_of_type(outer_source, text)?;
            return (!self.is_gap(t)
                && !self.type_of(t).flags.intersects(TypeFlags::UNION | TypeFlags::INTERSECTION))
            .then_some(t);
        }
        self.destructuring_assignment_source(node)
    }

    /// Whether `get_index_infos_of_type` answers `ty`'s index infos from
    /// declarations it reads completely — a class, interface or type literal
    /// (`index_infos_of_symbol`), a class or enum object, a primitive's
    /// apparent interface, a mapped type that resolved an index — so an empty answer
    /// is upstream's "no index signature". Tuples (whose `Array` base index
    /// is not represented there) and mapped or other alias instantiations are
    /// not certified.
    fn index_infos_are_declared(&mut self, ty: TypeId) -> bool {
        if let TypeData::Union { types, .. } | TypeData::Intersection { types, .. } =
            &self.store.get(ty).data
        {
            let types = types.clone();
            return types.into_iter().all(|part| self.index_infos_are_declared(part));
        }
        if self.tuple_element_lists.contains_key(&ty)
            || self.variadic_tuple_elements.contains_key(&ty)
        {
            return false;
        }
        // A mapped type (or an alias instantiation, `Record<string, V>`)
        // publishes the index infos its key set resolved to; one whose key set
        // this port did not resolve (`{ [P in keyof any]: V }`) publishes none,
        // so only a non-empty answer is certified.
        let alias_reference = self.type_reference_targets.get(&ty).is_some_and(|&(symbol, _)| {
            self.binder
                .symbols()
                .get(self.binder.merged_symbol(symbol))
                .flags
                .contains(tsr_binder::SymbolFlags::TYPE_ALIAS)
        });
        if alias_reference || self.mapped_types.contains_key(&ty) {
            return self.get_index_infos_of_type(ty).is_some_and(|infos| !infos.is_empty());
        }
        let flags = self.store.get(ty).flags;
        if flags.intersects(
            TypeFlags::STRING_LIKE
                | TypeFlags::NUMBER_LIKE
                | TypeFlags::BOOLEAN_LIKE
                | TypeFlags::BIG_INT_LIKE
                | TypeFlags::ES_SYMBOL_LIKE,
        ) {
            return true;
        }
        match self.store.get(ty).data {
            TypeData::Named { members: Some(_), .. } => true,
            TypeData::Anonymous { symbol, .. } => self
                .binder
                .symbols()
                .get(symbol)
                .flags
                .intersects(tsr_binder::SymbolFlags::CLASS | tsr_binder::SymbolFlags::ENUM),
            _ => false,
        }
    }

    /// Whether `ty` or a union/intersection constituent is instantiable, or is
    /// an alias reference this port keeps unevaluated over type arguments.
    fn has_instantiable_constituent(&self, ty: TypeId) -> bool {
        if self.store.get(ty).flags.intersects(TypeFlags::INSTANTIABLE) {
            return true;
        }
        match &self.store.get(ty).data {
            TypeData::Union { types, .. } | TypeData::Intersection { types, .. } => {
                types.iter().any(|&part| self.has_instantiable_constituent(part))
            }
            _ => self.type_reference_targets.get(&ty).is_some_and(|(symbol, arguments)| {
                self.binder
                    .symbols()
                    .get(self.binder.merged_symbol(*symbol))
                    .flags
                    .contains(tsr_binder::SymbolFlags::TYPE_ALIAS)
                    && arguments.iter().any(|&argument| self.has_instantiable_constituent(argument))
            }),
        }
    }
}

/// `getPropertyNameFromIndex` for a string or number literal key: its value,
/// the text TS2339 prints (`indexType.AsLiteralType().value`).
fn literal_key_name(data: &TypeData) -> Option<String> {
    match data {
        TypeData::StringLiteral(text) | TypeData::NumberLiteral(text) => Some(text.clone()),
        _ => None,
    }
}
