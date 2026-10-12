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

/// [`Checker::suggestion_for_nonexistent_index_signature`]'s answer.
pub(crate) enum IndexSignatureSuggestion {
    /// A member this port cannot read as native does; the report declines.
    Undecided,
    /// No `get`/`set` member takes the key.
    NoSuggestion,
    /// The call to suggest (`c.get`).
    Call(String),
}

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
        if self.indexed_access_index_is_generic(index_type) {
            // checkNonNullExpression's receiver: a written `null` or
            // `undefined` constituent is reported elsewhere and removed.
            let nullable = match &self.store.get(object_type).data {
                TypeData::Union { types, .. } => types
                    .iter()
                    .any(|&part| self.store.get(part).flags.intersects(TypeFlags::NULLABLE)),
                _ => false,
            };
            let object_type =
                if nullable { self.get_non_nullable_type(object_type) } else { object_type };
            self.check_deferred_access_index_type(object_type, index_type, node);
        }
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
        if has_string || index_type == self.intrinsics.any && !infos.is_empty() {
            return;
        }
        // The first constituent no index info applies to: the one
        // `getPropertyTypeForIndexType` reports for (`:26985`).
        let Some(missing) = constituents
            .into_iter()
            .find(|&part| self.get_applicable_index_info(apparent, part).is_none())
        else {
            return;
        };
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
        match self.suggestion_for_nonexistent_index_signature(apparent, node, index_type) {
            IndexSignatureSuggestion::Undecided => return,
            IndexSignatureSuggestion::Call(suggestion) => {
                let span = self.error_span(node);
                let printed = self.type_to_string(object_type);
                self.report(
                    file,
                    Diagnostic::with_args(
                        &messages::ELEMENT_IMPLICITLY_HAS_AN_ANY_TYPE_BECAUSE_TYPE_0_HAS_NO_INDEX_SIGNATURE_DID_YOU_MEAN_TO_CALL_1,
                        span,
                        [printed, suggestion],
                    ),
                );
                return;
            }
            IndexSignatureSuggestion::NoSuggestion => {}
        }
        let span = self.error_span(node);
        let index_text = self.type_to_string(index_type);
        let object_text = self.type_to_string(object_type);
        // The chained explanation (`:27165-27178`): a plain `string` or
        // `number` key's "No index signature with a parameter of type …";
        // any other key kind admitted here (a pattern literal, `symbol`)
        // has none.
        // `NewDiagnosticForNode(accessExpression, …)`: the child carries the
        // file.
        let child =
            self.store.get(missing).flags.intersects(TypeFlags::STRING | TypeFlags::NUMBER).then(
                || {
                    let missing_text = self.type_to_string(missing);
                    self.related_diagnostic_at(
                    node,
                    &messages::NO_INDEX_SIGNATURE_WITH_A_PARAMETER_OF_TYPE_0_WAS_FOUND_ON_TYPE_1,
                    span,
                    vec![missing_text, object_text.clone()],
                )
                },
            );
        let diagnostic = match child {
            Some(child) => Diagnostic::new_chain(
                Some(child),
                &messages::ELEMENT_IMPLICITLY_HAS_AN_ANY_TYPE_BECAUSE_EXPRESSION_OF_TYPE_0_CAN_T_BE_USED_TO_INDEX_TYPE_1,
                [index_text, object_text],
            ),
            None => Diagnostic::with_args(
                &messages::ELEMENT_IMPLICITLY_HAS_AN_ANY_TYPE_BECAUSE_EXPRESSION_OF_TYPE_0_CAN_T_BE_USED_TO_INDEX_TYPE_1,
                span,
                [index_text, object_text],
            ),
        };
        self.report(file, diagnostic);
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
        let Some(name_id) = name.node_id() else { return };
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
        if let Some(access_type) = self.node_map.get(node).and_then(|node| match node {
            Node::IndexedAccessTypeNode(access) => {
                Some(tsr_ast::TypeNode::IndexedAccessTypeNode(access))
            }
            _ => None,
        }) {
            let access_type = self.get_type_from_type_node(access_type);
            if self.indexed_access_type_node_is_checked(node)
                && let Some(&(object, index, _)) =
                    self.deferred_indexed_access_types.get(&access_type)
            {
                self.check_deferred_access_index_type(object, index, node);
            }
        }
    }

    /// TS2536 — `Type '{0}' cannot be used to index type '{1}'.` — the
    /// final arm of `checkIndexedAccessIndexType` (`checker.go:8219`), for
    /// an access whose type `getIndexedAccessTypeOrUndefined` deferred
    /// (`:26975`): for an indexed access type node, this port's deferred mint
    /// (`deferred_indexed_access_types`) of the node; for an element access,
    /// a generic index (`isGenericIndexType`) on the non-nullable receiver,
    /// whose own deferral this port only mints once the key is admitted
    /// (`deferred_indexed_access`). A deferred access every
    /// constituent of whose index is assignable to the object's
    /// `getIndexType` is admitted; any other is reported at the access node,
    /// printing the index and object types, unless the index names a
    /// non-public constituent property (`:8241`, TS4105, not ported here).
    ///
    /// Declined: an object or index this port could not compute, a
    /// remapping mapped object (`getIndexTypeForMappedType`, not
    /// reproduced), an object whose apparent type has a `number` index info
    /// (the `hasNumberIndexInfo` disjunct), and keys `resolved_keyof_type`
    /// cannot form. No table: the relation's own caches (r7-flow §48).
    fn check_deferred_access_index_type(
        &mut self,
        object_type: TypeId,
        index_type: TypeId,
        access_node: NodeId,
    ) {
        if self.is_gap(object_type)
            || self.is_gap(index_type)
            || self.is_type_any(object_type)
            || self.store.get(object_type).flags.intersects(TypeFlags::ANY_OR_UNKNOWN)
            || self.unresolved_types.contains(&object_type)
            || self.unresolved_types.contains(&index_type)
        {
            return;
        }
        if self.mapped_types.get(&object_type).is_some_and(|info| info.name_type.is_some()) {
            return;
        }
        let Some(keys) = self.resolved_keyof_type(object_type) else { return };
        let indexes = match &self.store.get(index_type).data {
            TypeData::Union { types, .. } => types.clone(),
            _ => vec![index_type],
        };
        // A constituent the relation cannot decide here is not evidence.
        let mut missing = false;
        for index in indexes {
            match self.relate_ternary(index, keys, Relation::Assignable) {
                Ternary::Related => {}
                Ternary::NotRelated => missing = true,
                Ternary::Unknown => return,
            }
        }
        if !missing {
            return;
        }
        let apparent = self.apparent_type(object_type);
        if self
            .get_index_infos_of_type(apparent)
            .is_none_or(|infos| infos.iter().any(|info| info.key == self.intrinsics.number))
        {
            return;
        }
        // getConstituentProperty (`checker.go:8257`): the first constituent
        // of the distributed apparent type that has the property.
        if let Some(name) = literal_key_name(&self.store.get(index_type).data)
            && self.is_generic_object_for_index_report(object_type)
        {
            let parts = match &self.store.get(apparent).data {
                TypeData::Union { types, .. } => types.clone(),
                _ => vec![apparent],
            };
            let property =
                parts.into_iter().find_map(|part| self.get_property_of_type(part, &name));
            if property.is_some_and(|property| self.property_is_non_public(property)) {
                let Some(file) = self.source_file_of_for_diagnostics(access_node) else { return };
                let span = self.error_span(access_node);
                self.report(
                    file,
                    Diagnostic::with_args(
                        &messages::PRIVATE_OR_PROTECTED_MEMBER_0_CANNOT_BE_ACCESSED_ON_A_TYPE_PARAMETER,
                        span,
                        [name],
                    ),
                );
                return;
            }
        }
        let Some(file) = self.source_file_of_for_diagnostics(access_node) else { return };
        let span = self.error_span(access_node);
        let index_text = self.type_to_string(index_type);
        let object_text = self.type_to_string(object_type);
        self.report(
            file,
            Diagnostic::with_args(
                &messages::TYPE_0_CANNOT_BE_USED_TO_INDEX_TYPE_1,
                span,
                [index_text, object_text],
            ),
        );
    }

    /// `isGenericObjectType` for the TS4105 arm: an instantiable
    /// non-primitive type, a generic mapped type, or a union or
    /// intersection with one. A generic tuple is not distinguished; it has
    /// no non-public members to find.
    fn is_generic_object_for_index_report(&mut self, ty: TypeId) -> bool {
        if self.store.get(ty).flags.intersects(TypeFlags::INSTANTIABLE_NON_PRIMITIVE)
            || self.is_generic_mapped_type(ty)
        {
            return true;
        }
        match &self.store.get(ty).data {
            TypeData::Union { types, .. } | TypeData::Intersection { types, .. } => {
                let types = types.clone();
                types.into_iter().any(|part| self.is_generic_object_for_index_report(part))
            }
            _ => false,
        }
    }

    /// Whether native's check walk reaches the indexed access type `node`
    /// as written: not inside the true branch of an enclosing conditional
    /// type, not inside the template of a mapped type without an `as`
    /// clause, and not inside an import type's type arguments.
    ///
    /// getConditionalFlowTypeOfType (`checker.go`) substitutes a type
    /// written in a conditional's true branch (getImpliedConstraint), and a
    /// mapped type's own parameter in its template when the homomorphic
    /// type variable's constraint is every array or tuple (`number` or a
    /// numeric string); this port's type node road has no substitution
    /// types, so such an access is not certified (the conditional half is
    /// the walk of `constraints.rs`' private `is_in_conditional_true_branch`). `checkImportType`
    /// (`checker.go:3324`) checks its argument and the reference but never
    /// `checkSourceElements` its type arguments, which `checkTypeReferenceNode`
    /// (`:2994`) does; this port's walk visits them.
    fn indexed_access_type_node_is_checked(&self, node: NodeId) -> bool {
        let mut child = node;
        while let Some(parent) = self.nodes.parent(child) {
            match self.node_map.get(parent) {
                Some(Node::ConditionalTypeNode(conditional))
                    if conditional.true_type.and_then(|true_type| true_type.node_id())
                        == Some(child) =>
                {
                    return false;
                }
                Some(Node::MappedTypeNode(mapped))
                    if mapped.name_type.is_none()
                        && mapped.r#type.and_then(|template| template.node_id()) == Some(child) =>
                {
                    return false;
                }
                Some(Node::ImportTypeNode(import))
                    if import
                        .type_arguments
                        .iter()
                        .any(|argument| argument.node_id() == Some(child)) =>
                {
                    return false;
                }
                _ => {}
            }
            child = parent;
        }
        true
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
                // `resolveAnonymousTypeMembers`' globalThis arm
                // (`checker.go:20674-20680`): neither block-scoped nor a value
                // module every declaration of which is an ambient module
                // (`declare module "m"`).
                let present = name == "globalThis"
                    || self.binder.global(&name).is_some_and(|symbol| {
                        let entry = self.binder.symbols().get(symbol);
                        let ambient_module =
                            entry.flags.intersects(tsr_binder::SymbolFlags::VALUE_MODULE)
                                && !entry.declarations.is_empty()
                                && entry.declarations.iter().all(|&declaration| {
                                    matches!(
                                        self.node_map.get(declaration),
                                        Some(Node::ModuleDeclaration(module))
                                            if matches!(
                                                module.name,
                                                Some(tsr_ast::ModuleName::StringLiteral(_))
                                            )
                                    )
                                });
                        !ambient_module
                            && !entry.flags.intersects(
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
                let Some(id) = name.node_id() else { return };
                (name.text.to_string(), id)
            }
            Some(tsr_ast::PropertyName::StringLiteral(name)) => {
                let Some(id) = name.node_id() else { return };
                (name.text.to_string(), id)
            }
            Some(tsr_ast::PropertyName::NumericLiteral(name)) => {
                let Some(id) = name.node_id() else { return };
                (crate::printing::normalise_number(name.text), id)
            }
            Some(_) => return,
            None => {
                let Some(tsr_ast::BindingName::Identifier(name)) = element.name else { return };
                let Some(id) = name.node_id() else { return };
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
        // An unannotated parameter's `unknown` is this port's contextual
        // parameter type, which can differ from native's where inference
        // fixes the context from the pattern (`inferredRestTypeFixedOnce`:
        // native `{ cancelable: any; }`, here `unknown`). `unknown` misses
        // every name, so only a written or catch-clause `unknown` is
        // certified; the contextual road waits on the implied pattern type
        // (BINDING-PATTERN-IMPLIED-TYPE, r7-contextual).
        if annotation.is_none()
            && self.nodes.kind(root) == SyntaxKind::Parameter
            && self.store.get(parent_type).flags.contains(TypeFlags::UNKNOWN)
        {
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
                    let Some(id) = name.node_id() else { continue };
                    literal_keys.push((name.text.to_string(), id, has_default));
                }
                tsr_ast::PropertyName::StringLiteral(name) => {
                    let Some(id) = name.node_id() else { continue };
                    literal_keys.push((name.text.to_string(), id, has_default));
                }
                tsr_ast::PropertyName::NumericLiteral(name) => {
                    let Some(id) = name.node_id() else { continue };
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
    /// `getSuggestionForNonexistentIndexSignature` (`checker.go:27231`):
    /// `get` (or `set` for an assignment target) when the object type has
    /// that member, a single call signature taking at least one argument,
    /// and the key assignable to its first parameter; prefixed by the
    /// receiver's dotted text (`tryGetPropertyAccessOrIdentifierToString`)
    /// when it has one. `Undecided` is a member whose single signature this
    /// port cannot read as native does (an interface type, a generic
    /// signature or parameter), which declines the report.
    /// `object_type` is `getPropertyOfObjectType`'s: only an object type's
    /// own resolved members are asked. No cache: run once per reported
    /// element access.
    pub(crate) fn suggestion_for_nonexistent_index_signature(
        &mut self,
        object_type: TypeId,
        access: NodeId,
        keyed_type: TypeId,
    ) -> IndexSignatureSuggestion {
        let Some(Node::ElementAccessExpression(element)) = self.node_map.get(access) else {
            return IndexSignatureSuggestion::NoSuggestion;
        };
        let method = if self.assignment_target_kind(access)
            == crate::expressions::AssignmentTargetKind::None
        {
            "get"
        } else {
            "set"
        };
        if !self.store.get(object_type).flags.intersects(TypeFlags::OBJECT) {
            return IndexSignatureSuggestion::NoSuggestion;
        }
        let Some(property) = self.get_property_of_type(object_type, method) else {
            return IndexSignatureSuggestion::NoSuggestion;
        };
        let member_type = self.get_type_of_symbol(property);
        // getSingleCallSignature: an object type with one call signature and
        // no construct signature, index info or property. A type whose
        // signatures this port does not hold (an interface or class
        // instance) is not read here.
        if !self.store.get(member_type).flags.intersects(TypeFlags::OBJECT) {
            return IndexSignatureSuggestion::NoSuggestion;
        }
        if !self.signature_types.contains_key(&member_type)
            && !matches!(self.store.get(member_type).data, TypeData::Anonymous { .. })
        {
            return IndexSignatureSuggestion::Undecided;
        }
        let (Some(infos), Some(constructs), Some(signatures)) = (
            self.get_index_infos_of_type(member_type),
            self.signatures_of_type_kind(member_type, crate::signatures::SignatureKind::Construct),
            self.call_signatures_of_type(member_type),
        ) else {
            return IndexSignatureSuggestion::Undecided;
        };
        if !self.property_names_of(member_type).is_empty()
            || !infos.is_empty()
            || !constructs.is_empty()
        {
            return IndexSignatureSuggestion::NoSuggestion;
        }
        let [signature] = signatures.as_slice() else {
            return IndexSignatureSuggestion::NoSuggestion;
        };
        if !signature.type_parameters.is_empty() {
            return IndexSignatureSuggestion::Undecided;
        }
        let signature = signature.clone();
        if self.signature_min_argument_count(&signature) < 1 {
            return IndexSignatureSuggestion::NoSuggestion;
        }
        let parameter =
            self.signature_type_at_position(&signature, 0).unwrap_or(self.intrinsics.any);
        // A parameter still mentioning a type parameter is a member read
        // without its receiver's instantiation here; not certified.
        if self.has_instantiable_constituent(parameter) || self.is_gap(parameter) {
            return IndexSignatureSuggestion::Undecided;
        }
        if !self.is_type_assignable_to(keyed_type, parameter) {
            return IndexSignatureSuggestion::NoSuggestion;
        }
        let receiver = element.expression.and_then(|expression| expression.node_id());
        IndexSignatureSuggestion::Call(
            match receiver.and_then(|receiver| self.property_access_or_identifier_text(receiver)) {
                Some(text) => format!("{text}.{method}"),
                None => method.to_string(),
            },
        )
    }

    /// `tryGetPropertyAccessOrIdentifierToString` (`utilities.go:1558`): an
    /// identifier's text, or a dotted chain of property accesses and
    /// element accesses with a property-name argument over one.
    fn property_access_or_identifier_text(&self, node: NodeId) -> Option<String> {
        match self.node_map.get(node)? {
            Node::Identifier(identifier) => Some(identifier.text.to_string()),
            Node::PropertyAccessExpression(access) => {
                let base =
                    self.property_access_or_identifier_text(access.expression?.node_id()?)?;
                let name = match access.name? {
                    tsr_ast::MemberName::Identifier(name) => name.text,
                    tsr_ast::MemberName::PrivateIdentifier(name) => name.text,
                };
                Some(format!("{base}.{name}"))
            }
            Node::ElementAccessExpression(access) => {
                let base =
                    self.property_access_or_identifier_text(access.expression?.node_id()?)?;
                let name = match access.argument_expression? {
                    tsr_ast::Expression::StringLiteral(literal) => literal.text.to_string(),
                    tsr_ast::Expression::NumericLiteral(literal) => literal.text.to_string(),
                    tsr_ast::Expression::NoSubstitutionTemplateLiteral(literal) => {
                        literal.text.to_string()
                    }
                    tsr_ast::Expression::Identifier(identifier) => identifier.text.to_string(),
                    _ => return None,
                };
                Some(format!("{base}.{name}"))
            }
            _ => None,
        }
    }

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
