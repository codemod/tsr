//! The final error arm of `getPropertyTypeForIndexType` (`checker.go:27001`):
//! an index type no property, index signature or valid key kind answers is
//! reported on the access node's index (`getIndexNodeForAccessExpression`).
//!
//! TS2538 (`Type '{0}' cannot be used as an index type.`) is the arm for an
//! index type that is nullable or not assignable to a string, number or
//! symbol kind; such an index never reaches the property or index-signature
//! lookups above it. TS2537 is the arm for a non-literal `string`/`number`
//! key with no applicable index signature.
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
            || self.is_error(index_type)
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
    pub(crate) fn check_element_access_index_type(&mut self, node: NodeId) {
        if self.file_has_parse_errors || self.in_js_file(node) {
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
            return;
        }
        self.report_invalid_index_types(object_type, index_type, argument_id);
    }

    /// `getTypeFromIndexedAccessTypeNode` (`checker.go:24164`): the access
    /// node is the type node, its index node the index type node.
    pub(crate) fn check_indexed_access_type_index_type(&mut self, node: NodeId) {
        if self.file_has_parse_errors || self.in_js_file(node) {
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

    /// `isConstEnumObjectType` (`checker.go:27664`).
    fn is_const_enum_object_type(&self, ty: TypeId) -> bool {
        let TypeData::Anonymous { symbol, .. } = self.store.get(ty).data else { return false };
        let merged = self.binder.merged_symbol(symbol);
        self.binder.symbols().get(merged).flags.contains(tsr_binder::SymbolFlags::CONST_ENUM)
    }
}
